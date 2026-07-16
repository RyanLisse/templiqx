//go:build integration

package templiqx

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"syscall"
	"testing"
	"time"
)

var fingerprintPattern = regexp.MustCompile(`^(?:sha256:)?[a-f0-9]{64}$`)

func TestIntegrationAgainstRealServer(t *testing.T) {
	if os.Getenv("TEMPLIQX_SDK_IT") != "1" {
		t.Skip("set TEMPLIQX_SDK_IT=1 to run the live-server integration test")
	}

	repoRoot, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatalf("repo root: %v", err)
	}
	tempDir := t.TempDir()
	packagesRoot := filepath.Join(tempDir, "packages")
	workspaceRoot := filepath.Join(tempDir, "workspace")
	for _, directory := range []string{packagesRoot, workspaceRoot} {
		if err := os.MkdirAll(directory, 0o755); err != nil {
			t.Fatalf("create %s: %v", directory, err)
		}
	}
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("reserve port: %v", err)
	}
	port := listener.Addr().(*net.TCPAddr).Port
	if err := listener.Close(); err != nil {
		t.Fatalf("release port: %v", err)
	}
	baseURL := fmt.Sprintf("http://127.0.0.1:%d", port)

	command := exec.Command("cargo", "run", "--quiet", "-p", "templiqx-http-server")
	command.Dir = repoRoot
	command.Env = append(filteredEnvironment(
		"MODEL_API_KEY", "TEMPLIQX_HTTP_ADDR", "TEMPLIQX_ROOT", "TEMPLIQX_WORKSPACE", "TEMPLIQX_RUNTIME_MODE",
	),
		fmt.Sprintf("TEMPLIQX_HTTP_ADDR=127.0.0.1:%d", port),
		"TEMPLIQX_ROOT="+packagesRoot,
		"TEMPLIQX_WORKSPACE="+workspaceRoot,
		"TEMPLIQX_RUNTIME_MODE=deterministic-fake",
	)
	var serverOutput bytes.Buffer
	command.Stdout = &serverOutput
	command.Stderr = &serverOutput
	if err := command.Start(); err != nil {
		t.Fatalf("start server: %v", err)
	}
	exit := make(chan error, 1)
	go func() { exit <- command.Wait() }()
	serverExited := false
	t.Cleanup(func() {
		if serverExited || command.Process == nil {
			return
		}
		_ = command.Process.Signal(syscall.SIGTERM)
		select {
		case <-exit:
		case <-time.After(5 * time.Second):
			_ = command.Process.Kill()
			<-exit
		}
	})

	readinessClient := &http.Client{Timeout: 500 * time.Millisecond}
	deadline := time.Now().Add(120 * time.Second)
	ready := false
	for time.Now().Before(deadline) {
		select {
		case err := <-exit:
			serverExited = true
			t.Fatalf("server exited during startup: %v\n%s", err, serverOutput.String())
		default:
		}
		response, requestErr := readinessClient.Get(baseURL + "/operations/v1/health/ready")
		if requestErr == nil {
			_ = response.Body.Close()
			if response.StatusCode >= http.StatusOK && response.StatusCode < http.StatusMultipleChoices {
				ready = true
				break
			}
		}
		time.Sleep(100 * time.Millisecond)
	}
	if !ready {
		t.Fatalf("server did not become ready:\n%s", serverOutput.String())
	}

	client, err := NewClient(baseURL, WithClientTimeout(5*time.Second))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	ctx := context.Background()
	live, err := client.GetOperationsV1Liveness(ctx)
	if err != nil || live.Data.Status != Ok {
		t.Fatalf("liveness = %#v, %v", live, err)
	}
	readyStatus, err := client.GetOperationsV1Readiness(ctx)
	if err != nil || readyStatus.Data.Status != Ready {
		t.Fatalf("readiness = %#v, %v", readyStatus, err)
	}
	catalog, err := client.Catalog(ctx)
	if err != nil || !catalog.Data.Ok || catalog.Data.Result == nil || !contains(*catalog.Data.Result, "execute_contract") {
		t.Fatalf("catalog = %#v, %v", catalog, err)
	}

	created, err := client.CreatePackage(ctx, CreatePackageRequest{Name: "sdk-go-it", Version: "0.1.0"})
	if err != nil {
		t.Fatalf("CreatePackage: %v", err)
	}
	packageFingerprint := created.Data.Fingerprints["package"]
	if !fingerprintPattern.MatchString(packageFingerprint) {
		t.Fatalf("package fingerprint = %q", packageFingerprint)
	}
	description := "Go SDK integration"
	if _, err := client.UpdatePackage(
		ctx, "sdk-go-it", UpdatePackageRequest{Description: &description}, WithIfMatch(packageFingerprint),
	); err != nil {
		t.Fatalf("UpdatePackage: %v", err)
	}

	contractSource, err := os.ReadFile(filepath.Join(repoRoot, "examples", "packages", "demo", "contracts", "greeting.yaml"))
	if err != nil {
		t.Fatalf("read greeting contract: %v", err)
	}
	if _, err := client.PutContract(ctx, "sdk-go-it", "greeting", string(contractSource)); err != nil {
		t.Fatalf("PutContract: %v", err)
	}
	inputs := map[string]JsonValue{"name": "Ryan"}
	contractContext := map[string]JsonValue{"organization": "Blinqx"}
	capabilities := []string{"structured_output"}
	render := &RenderRequest{Inputs: &inputs, Context: &contractContext}
	compiled, err := client.CompileContract(
		ctx, "sdk-go-it", "greeting", CompileRequest{Render: render, Capabilities: &capabilities},
	)
	if err != nil || !compiled.Data.Ok {
		t.Fatalf("CompileContract = %#v, %v", compiled, err)
	}
	fixtureOutput := JsonValue(map[string]any{"greeting": "Hello Ryan"})
	stream := false
	executed, err := client.ExecuteContract(ctx, "sdk-go-it", "greeting", ExecuteRequest{
		Render: render, Capabilities: &capabilities, FixtureOutput: &fixtureOutput, Stream: &stream,
	})
	if err != nil || !executed.Data.Ok || executed.Data.Result == nil {
		t.Fatalf("ExecuteContract = %#v, %v", executed, err)
	}
	fingerprint := executed.Data.Result.OutputFingerprint
	if !fingerprintPattern.MatchString(fingerprint) {
		t.Fatalf("ExecutionReceipt output fingerprint = %q", fingerprint)
	}
	t.Logf("ExecutionReceipt fingerprint: %s", fingerprint)

	_, err = client.InspectContract(ctx, "missing", "greeting")
	var httpError *TempliqxHTTPError
	if !errors.As(err, &httpError) || httpError.StatusCode != http.StatusNotFound ||
		httpError.Envelope == nil || httpError.Envelope.Diagnostics[0].Code != "TQX_NOT_FOUND" {
		t.Fatalf("missing contract error = %#v", err)
	}
	cancelled, cancel := context.WithCancel(ctx)
	cancel()
	_, err = client.Catalog(cancelled, WithRequestID("sdk-go-it-abort"))
	var transportError *TempliqxTransportError
	if !errors.As(err, &transportError) {
		t.Fatalf("cancel error = %#v", err)
	}
}

func filteredEnvironment(excluded ...string) []string {
	prefixes := make([]string, len(excluded))
	for index, name := range excluded {
		prefixes[index] = name + "="
	}
	result := make([]string, 0, len(os.Environ()))
	for _, entry := range os.Environ() {
		keep := true
		for _, prefix := range prefixes {
			if strings.HasPrefix(entry, prefix) {
				keep = false
				break
			}
		}
		if keep {
			result = append(result, entry)
		}
	}
	return result
}

func contains(values []string, expected string) bool {
	for _, value := range values {
		if value == expected {
			return true
		}
	}
	return false
}
