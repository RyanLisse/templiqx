package templiqx

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"reflect"
	"regexp"
	"strings"
	"testing"
	"time"
)

type roundTripFunc func(*http.Request) (*http.Response, error)

func (function roundTripFunc) RoundTrip(request *http.Request) (*http.Response, error) {
	return function(request)
}

type failingReadCloser struct{}

func (failingReadCloser) Read([]byte) (int, error) { return 0, errors.New("response read failed") }
func (failingReadCloser) Close() error             { return nil }

func jsonHTTPResponse(status int, body string, headers http.Header) *http.Response {
	if headers == nil {
		headers = make(http.Header)
	}
	headers.Set("Content-Type", "application/json")
	return &http.Response{
		StatusCode: status,
		Header:     headers,
		Body:       io.NopCloser(strings.NewReader(body)),
	}
}

func TestCompileContractBuildsRequestAndDecodesEnvelope(t *testing.T) {
	var captured *http.Request
	transport := roundTripFunc(func(request *http.Request) (*http.Response, error) {
		captured = request
		return jsonHTTPResponse(http.StatusOK, `{
            "api_version":"templiqx/v1alpha1",
            "operation":"compile_contract",
            "ok":true,
            "diagnostics":[],
            "fingerprints":{},
            "result":{"contract_id":"greeting","messages":[],"output_schema":{},"required_capabilities":[]}
        }`, http.Header{"X-Request-Id": {"server-request-42"}}), nil
	})
	client, err := NewClient(
		"https://templiqx.example/",
		WithHTTPClient(&http.Client{Transport: transport}),
		WithDefaultHeaders(http.Header{"X-Tenant-Id": {"tenant-a"}}),
	)
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	inputs := map[string]JsonValue{"name": "Ryan"}
	capabilities := []string{"structured_output"}
	response, err := client.CompileContract(
		context.Background(),
		"demo package",
		"greeting",
		CompileRequest{Render: &RenderRequest{Inputs: &inputs}, Capabilities: &capabilities},
		WithRequestID("sdk-request-42"),
		WithIfMatch("ignored-for-non-cas"),
	)
	if err != nil {
		t.Fatalf("CompileContract: %v", err)
	}
	if response.RequestID != "server-request-42" {
		t.Fatalf("request ID = %q", response.RequestID)
	}
	if response.Data.Result == nil || response.Data.Result.ContractId != "greeting" {
		t.Fatalf("decoded result = %#v", response.Data.Result)
	}
	if captured.Method != http.MethodPost {
		t.Fatalf("method = %s", captured.Method)
	}
	if captured.URL.EscapedPath() != "/operations/v1/packages/demo%20package/contracts/greeting/compile" {
		t.Fatalf("path = %s", captured.URL.EscapedPath())
	}
	if captured.Header.Get("X-Request-Id") != "sdk-request-42" {
		t.Fatalf("x-request-id = %q", captured.Header.Get("X-Request-Id"))
	}
	if captured.Header.Get("X-Tenant-Id") != "tenant-a" {
		t.Fatalf("x-tenant-id = %q", captured.Header.Get("X-Tenant-Id"))
	}
	if captured.Header.Get("If-Match") != "" {
		t.Fatalf("non-CAS If-Match = %q", captured.Header.Get("If-Match"))
	}
	var body map[string]any
	if err := json.NewDecoder(captured.Body).Decode(&body); err != nil {
		t.Fatalf("decode request body: %v", err)
	}
	render := body["render"].(map[string]any)
	requestInputs := render["inputs"].(map[string]any)
	if requestInputs["name"] != "Ryan" {
		t.Fatalf("request body = %#v", body)
	}
}

func TestAssessQualityProposalsUsesTypedPackageScopedRoute(t *testing.T) {
	transport := roundTripFunc(func(request *http.Request) (*http.Response, error) {
		if request.Method != http.MethodPost {
			t.Fatalf("method = %s", request.Method)
		}
		if request.URL.EscapedPath() != "/operations/v1/packages/demo%20package/quality/proposals:assess" {
			t.Fatalf("path = %s", request.URL.EscapedPath())
		}
		var body map[string]any
		if err := json.NewDecoder(request.Body).Decode(&body); err != nil {
			t.Fatalf("decode request: %v", err)
		}
		if body["contract_id"] != "greeting" {
			t.Fatalf("request body = %#v", body)
		}
		return jsonHTTPResponse(http.StatusOK, `{
            "api_version":"templiqx/v1alpha1","operation":"assess_quality_proposals",
            "ok":true,"diagnostics":[],"fingerprints":{}
        }`, nil), nil
	})
	client, err := NewClient("https://example.test", WithHTTPClient(&http.Client{Transport: transport}))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	fingerprint := QualityFingerprint(strings.Repeat("a", 64))
	body := QualityProposalRequest{
		Package:                         "demo package",
		ContractId:                      "greeting",
		ExpectedPackageFingerprint:      fingerprint,
		ExpectedBaseContractFingerprint: fingerprint,
		ExpectedFixtureSetFingerprint:   fingerprint,
		Policy: QualityPolicy{
			Id: "quality-policy", ReplicatesPerFixture: 1, MinimumSemanticCases: 1,
			ClaimedEvaluatorProfileFingerprint: fingerprint,
			ClaimedModelProfileFingerprint:     fingerprint,
			BinaryScorers:                      []BinaryScorer{}, Objectives: []QualityObjective{}, EligibilityRules: []EligibilityRule{},
		},
		Candidates: []QualityCandidateSubmission{},
	}
	response, err := client.AssessQualityProposals(context.Background(), "demo package", body)
	if err != nil {
		t.Fatalf("AssessQualityProposals: %v", err)
	}
	if response.Data.Operation != "assess_quality_proposals" {
		t.Fatalf("response = %#v", response.Data)
	}
}

func TestGeneratedQualityIntegersUseInt64AndRoundTripMaxSafe(t *testing.T) {
	const maxPublicInteger int64 = 9_007_199_254_740_991
	fingerprint := QualityFingerprint(strings.Repeat("a", 64))
	observation := MetricObservation{
		MetricId: "total_tokens", Unit: TokenCount, Value: maxPublicInteger,
		ClaimedMeasurementProfileFingerprint: fingerprint,
	}

	if kind := reflect.TypeOf(observation.Value).Kind(); kind != reflect.Int64 {
		t.Fatalf("MetricObservation.Value kind = %s, want int64", kind)
	}
	encoded, err := json.Marshal(observation)
	if err != nil {
		t.Fatalf("marshal observation: %v", err)
	}
	var decoded MetricObservation
	if err := json.Unmarshal(encoded, &decoded); err != nil {
		t.Fatalf("unmarshal observation: %v", err)
	}
	if decoded.Value != maxPublicInteger {
		t.Fatalf("round-tripped value = %d, want %d", decoded.Value, maxPublicInteger)
	}

	identities := ClaimedQualityIdentities{
		ClaimedCandidateContractFingerprint:   fingerprint,
		ClaimedEvaluatorProfileFingerprint:    fingerprint,
		ClaimedModelProfileFingerprint:        fingerprint,
		ClaimedScorerFingerprints:             map[string]QualityFingerprint{"grounded": fingerprint},
		ClaimedMeasurementProfileFingerprints: map[string]QualityFingerprint{"total_tokens": fingerprint},
	}
	encoded, err = json.Marshal(identities)
	if err != nil {
		t.Fatalf("marshal identities: %v", err)
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(encoded, &fields); err != nil {
		t.Fatalf("unmarshal identity fields: %v", err)
	}
	if _, ok := fields["claimed_candidate_contract_fingerprint"]; !ok {
		t.Fatalf("claimed identity fields = %v", fields)
	}
	if _, ok := fields["candidate_contract_fingerprint"]; ok {
		t.Fatalf("legacy unclaimed identity field present: %v", fields)
	}

	var invalidAssessment CandidateAssessment
	if err := json.Unmarshal([]byte(`{
		"eligibility":{"eligible":false,"total_trial_count":0,"semantic_trial_count":0,"infrastructure_trial_count":0,"semantic_coverage_ppm":0,"infrastructure_failure_ppm":0,"gates":[]},
		"aggregates":[],"trial_summaries":[],"proposal_change_paths":[],"diagnostics":[]
	}`), &invalidAssessment); err != nil {
		t.Fatalf("unmarshal assessment without claims: %v", err)
	}
	if invalidAssessment.ClaimedIdentities != nil {
		t.Fatalf("invalid assessment reflected claims: %#v", invalidAssessment.ClaimedIdentities)
	}
	encoded, err = json.Marshal(invalidAssessment)
	if err != nil {
		t.Fatalf("marshal assessment without claims: %v", err)
	}
	if err := json.Unmarshal(encoded, &fields); err != nil {
		t.Fatalf("unmarshal assessment fields: %v", err)
	}
	if _, ok := fields["claimed_identities"]; ok {
		t.Fatalf("optional claimed identities were not omitted: %s", encoded)
	}
}

func TestDefaultRequestIDIsUUID(t *testing.T) {
	var requestID string
	transport := roundTripFunc(func(request *http.Request) (*http.Response, error) {
		requestID = request.Header.Get("X-Request-Id")
		return jsonHTTPResponse(http.StatusOK, `{
            "api_version":"templiqx/v1alpha1","status":"ok"
        }`, nil), nil
	})
	client, err := NewClient(
		"https://example.test",
		WithHTTPClient(&http.Client{Transport: transport}),
		WithDefaultHeaders(nil),
	)
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if _, err := client.GetOperationsV1Liveness(context.Background()); err != nil {
		t.Fatalf("GetOperationsV1Liveness: %v", err)
	}
	if !regexp.MustCompile(`^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$`).MatchString(requestID) {
		t.Fatalf("x-request-id = %q", requestID)
	}
}

func TestCASHeaderAndArtifactPath(t *testing.T) {
	transport := roundTripFunc(func(request *http.Request) (*http.Response, error) {
		if request.Method != http.MethodDelete {
			t.Fatalf("method = %s", request.Method)
		}
		if request.URL.EscapedPath() != "/operations/v1/artifacts/reports/annual%20report.json" {
			t.Fatalf("path = %s", request.URL.EscapedPath())
		}
		if request.URL.Query().Get("package") != "demo package" || request.URL.Query().Get("workspace") != "review" {
			t.Fatalf("query = %s", request.URL.RawQuery)
		}
		if request.Header.Get("If-Match") != "sha256:abc" {
			t.Fatalf("If-Match = %q", request.Header.Get("If-Match"))
		}
		return jsonHTTPResponse(http.StatusOK, `{
            "api_version":"templiqx/v1alpha1","operation":"delete_workspace_artifact",
            "ok":true,"diagnostics":[],"fingerprints":{}
        }`, nil), nil
	})
	client, err := NewClient("https://example.test", WithHTTPClient(&http.Client{Transport: transport}))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	workspace := "review"
	if _, err := client.DeleteWorkspaceArtifact(
		context.Background(), "reports/annual report.json", "demo package", &workspace, WithIfMatch("sha256:abc"),
	); err != nil {
		t.Fatalf("DeleteWorkspaceArtifact: %v", err)
	}
}

func TestHTTPAndTransportErrors(t *testing.T) {
	envelopeClient, err := NewClient("https://example.test", WithHTTPClient(&http.Client{Transport: roundTripFunc(
		func(*http.Request) (*http.Response, error) {
			return jsonHTTPResponse(http.StatusNotFound, `{
                "api_version":"templiqx/v1alpha1","operation":"inspect_contract","ok":false,
                "diagnostics":[{"code":"TQX_NOT_FOUND","severity":"error","message":"missing"}],
                "fingerprints":{}
            }`, nil), nil
		},
	)}))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	_, err = envelopeClient.InspectContract(context.Background(), "missing", "greeting", WithRequestID("not-found"))
	var httpError *TempliqxHTTPError
	if !errors.As(err, &httpError) || httpError.StatusCode != http.StatusNotFound {
		t.Fatalf("HTTP error = %#v", err)
	}
	if httpError.Envelope == nil || httpError.Envelope.Diagnostics[0].Code != "TQX_NOT_FOUND" || httpError.RawBody != "" {
		t.Fatalf("HTTP envelope = %#v, raw = %q", httpError.Envelope, httpError.RawBody)
	}

	rawClient, err := NewClient("https://example.test", WithHTTPClient(&http.Client{Transport: roundTripFunc(
		func(*http.Request) (*http.Response, error) {
			return jsonHTTPResponse(http.StatusBadGateway, "gateway unavailable", nil), nil
		},
	)}))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	_, err = rawClient.Catalog(context.Background(), WithRequestID("raw-error"))
	if !errors.As(err, &httpError) || httpError.RawBody != "gateway unavailable" || httpError.RequestID != "raw-error" {
		t.Fatalf("raw HTTP error = %#v", err)
	}

	nullDiagnostics := `{"api_version":"templiqx/v1alpha1","diagnostics":null,"ok":false}`
	nullDiagnosticsClient, err := NewClient("https://example.test", WithHTTPClient(&http.Client{Transport: roundTripFunc(
		func(*http.Request) (*http.Response, error) {
			return jsonHTTPResponse(http.StatusBadRequest, nullDiagnostics, nil), nil
		},
	)}))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	_, err = nullDiagnosticsClient.Catalog(context.Background(), WithRequestID("null-diagnostics"))
	if !errors.As(err, &httpError) || httpError.Envelope != nil || httpError.RawBody != nullDiagnostics {
		t.Fatalf("null diagnostics HTTP error = %#v", err)
	}

	readFailureClient, err := NewClient("https://example.test", WithHTTPClient(&http.Client{Transport: roundTripFunc(
		func(*http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode: http.StatusBadGateway,
				Header:     make(http.Header),
				Body:       failingReadCloser{},
			}, nil
		},
	)}))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	_, err = readFailureClient.Catalog(context.Background(), WithRequestID("read-error"))
	if !errors.As(err, &httpError) || httpError.StatusCode != http.StatusBadGateway {
		t.Fatalf("HTTP read error = %#v", err)
	}

	malformedClient, err := NewClient("https://example.test", WithHTTPClient(&http.Client{Transport: roundTripFunc(
		func(*http.Request) (*http.Response, error) {
			return jsonHTTPResponse(http.StatusOK, "not-json", nil), nil
		},
	)}))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	_, err = malformedClient.Catalog(context.Background(), WithRequestID("malformed"))
	var malformedTransportError *TempliqxTransportError
	if err == nil || errors.As(err, &malformedTransportError) || errors.As(err, &httpError) {
		t.Fatalf("malformed success error = %#v", err)
	}

	waiting := roundTripFunc(func(request *http.Request) (*http.Response, error) {
		<-request.Context().Done()
		return nil, request.Context().Err()
	})
	timeoutClient, err := NewClient("https://example.test", WithHTTPClient(&http.Client{Transport: waiting}))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	_, err = timeoutClient.Catalog(context.Background(), WithRequestID("timeout"), WithCallTimeout(time.Millisecond))
	var transportError *TempliqxTransportError
	if !errors.As(err, &transportError) || transportError.RequestID != "timeout" || !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("transport error = %#v", err)
	}
}

func TestCompatibilityAndOperationCoverage(t *testing.T) {
	if err := AssertCompatibility(); err != nil {
		t.Fatalf("AssertCompatibility: %v", err)
	}
	if Compatibility.OpsApiVersion != "1.0.0-alpha.2" || Compatibility.ContractFormat != "templiqx/v1alpha1" {
		t.Fatalf("compatibility = %#v", Compatibility)
	}
	expected := []string{
		"AssessQualityProposals", "Catalog", "CompileContract", "CreatePackage", "DeleteContract", "DeletePackage",
		"DeleteWorkspaceArtifact", "DiffContract", "DiscoverPackages", "ExecuteContract",
		"ExplainContract", "ExportPackageIdentity", "GetOperationsV1Liveness",
		"GetOperationsV1OpenAPI", "GetOperationsV1OpenAPIYaml", "GetOperationsV1Readiness",
		"InspectContract", "InspectDocument", "ListEvals", "ListWorkspaceArtifacts", "MigrateLegacy",
		"PutContract", "ReadArtifact", "RenderContract", "RenderDocument", "RunEval", "SignPackage",
		"TestPackage", "UpdatePackage", "ValidateContract", "ValidatePackage", "VerifyPackageTrust",
	}
	typeOfClient := reflect.TypeOf((*Client)(nil))
	if typeOfClient.NumMethod() != len(expected) {
		t.Fatalf("Client method count = %d, want %d", typeOfClient.NumMethod(), len(expected))
	}
	for _, method := range expected {
		if _, ok := typeOfClient.MethodByName(method); !ok {
			t.Errorf("missing operation method %s", method)
		}
	}
}
