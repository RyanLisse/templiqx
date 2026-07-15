package templiqx

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

const defaultTimeout = 30 * time.Second

// TempliqxResponse pairs a decoded operation response with its effective
// request ID.
type TempliqxResponse[T any] struct {
	Data      T
	RequestID string
}

// Client is a thin net/http façade over the Templiqx Operations API.
type Client struct {
	baseURL        string
	httpClient     *http.Client
	timeout        time.Duration
	defaultHeaders http.Header
}

type clientOptions struct {
	httpClient     *http.Client
	timeout        time.Duration
	defaultHeaders http.Header
}

// Option configures transport behavior shared by all calls.
type Option func(*clientOptions)

// WithHTTPClient supplies the net/http client used for every request.
func WithHTTPClient(client *http.Client) Option {
	return func(options *clientOptions) { options.httpClient = client }
}

// WithClientTimeout sets the default per-call timeout. A non-positive value
// disables the SDK-level deadline; context and http.Client deadlines still apply.
func WithClientTimeout(timeout time.Duration) Option {
	return func(options *clientOptions) { options.timeout = timeout }
}

// WithDefaultHeaders supplies headers copied onto every request.
func WithDefaultHeaders(headers http.Header) Option {
	return func(options *clientOptions) { options.defaultHeaders = headers.Clone() }
}

// NewClient constructs an Operations API client.
func NewClient(baseURL string, options ...Option) (*Client, error) {
	parsed, err := url.Parse(baseURL)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return nil, fmt.Errorf("invalid Templiqx base URL %q", baseURL)
	}
	settings := clientOptions{
		httpClient:     http.DefaultClient,
		timeout:        defaultTimeout,
		defaultHeaders: make(http.Header),
	}
	for _, option := range options {
		option(&settings)
	}
	if settings.httpClient == nil {
		settings.httpClient = http.DefaultClient
	}
	defaultHeaders := settings.defaultHeaders.Clone()
	if defaultHeaders == nil {
		defaultHeaders = make(http.Header)
	}
	return &Client{
		baseURL:        strings.TrimRight(baseURL, "/"),
		httpClient:     settings.httpClient,
		timeout:        settings.timeout,
		defaultHeaders: defaultHeaders,
	}, nil
}

type callOptions struct {
	requestID string
	ifMatch   string
	timeout   time.Duration
}

// CallOption configures one Operations API request.
type CallOption interface {
	apply(*callOptions)
}

type callOptionFunc func(*callOptions)

func (option callOptionFunc) apply(options *callOptions) { option(options) }

// IfMatchOption is a typed functional option required by mandatory CAS
// operations. Construct one with WithIfMatch.
type IfMatchOption struct {
	fingerprint string
}

func (option IfMatchOption) apply(options *callOptions) {
	options.ifMatch = option.fingerprint
}

// WithRequestID sets the x-request-id correlation header.
func WithRequestID(requestID string) CallOption {
	return callOptionFunc(func(options *callOptions) { options.requestID = requestID })
}

// WithIfMatch sets the If-Match header for a CAS operation. It is ignored by
// operations that are not marked as CAS mutations.
func WithIfMatch(fingerprint string) IfMatchOption {
	return IfMatchOption{fingerprint: fingerprint}
}

// WithCallTimeout overrides the default timeout for one request.
func WithCallTimeout(timeout time.Duration) CallOption {
	return callOptionFunc(func(options *callOptions) { options.timeout = timeout })
}

type dispatchRequest struct {
	method      string
	path        string
	body        any
	rawBody     *string
	contentType string
	cas         bool
}

func dispatchJSON[T any](ctx context.Context, client *Client, request dispatchRequest, options ...CallOption) (*TempliqxResponse[T], error) {
	return dispatch(ctx, client, request, func(body []byte) (T, error) {
		var value T
		err := json.Unmarshal(body, &value)
		return value, err
	}, options...)
}

func dispatchText(ctx context.Context, client *Client, request dispatchRequest, options ...CallOption) (*TempliqxResponse[string], error) {
	return dispatch(ctx, client, request, func(body []byte) (string, error) {
		return string(body), nil
	}, options...)
}

func dispatch[T any](
	ctx context.Context,
	client *Client,
	request dispatchRequest,
	decode func([]byte) (T, error),
	options ...CallOption,
) (*TempliqxResponse[T], error) {
	settings := callOptions{timeout: client.timeout}
	for _, option := range options {
		option.apply(&settings)
	}
	requestID := settings.requestID
	if requestID == "" {
		var err error
		requestID, err = newRequestID()
		if err != nil {
			return nil, &TempliqxTransportError{Err: err}
		}
	}

	if settings.timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, settings.timeout)
		defer cancel()
	}

	var body io.Reader
	if request.rawBody != nil {
		body = strings.NewReader(*request.rawBody)
	} else if request.body != nil {
		encoded, err := json.Marshal(request.body)
		if err != nil {
			return nil, fmt.Errorf("encode Templiqx request: %w", err)
		}
		body = bytes.NewReader(encoded)
	}

	httpRequest, err := http.NewRequestWithContext(ctx, request.method, client.baseURL+request.path, body)
	if err != nil {
		return nil, fmt.Errorf("build Templiqx request: %w", err)
	}
	httpRequest.Header = client.defaultHeaders.Clone()
	httpRequest.Header.Set("Accept", "application/json, application/yaml")
	httpRequest.Header.Set("X-Request-Id", requestID)
	if body != nil {
		contentType := request.contentType
		if contentType == "" {
			contentType = "application/json"
		}
		httpRequest.Header.Set("Content-Type", contentType)
	}
	if request.cas && settings.ifMatch != "" {
		httpRequest.Header.Set("If-Match", settings.ifMatch)
	}

	httpResponse, err := client.httpClient.Do(httpRequest)
	if err != nil {
		return nil, &TempliqxTransportError{RequestID: requestID, Err: err}
	}
	defer httpResponse.Body.Close()
	effectiveRequestID := httpResponse.Header.Get("X-Request-Id")
	if effectiveRequestID == "" {
		effectiveRequestID = requestID
	}
	responseBody, readErr := io.ReadAll(httpResponse.Body)
	if httpResponse.StatusCode < http.StatusOK || httpResponse.StatusCode >= http.StatusMultipleChoices {
		var envelope *OperationEnvelopeBase
		if readErr == nil {
			envelope = decodeOperationEnvelope(responseBody)
		}
		httpError := &TempliqxHTTPError{
			StatusCode: httpResponse.StatusCode,
			Envelope:   envelope,
			RequestID:  effectiveRequestID,
		}
		if envelope == nil {
			httpError.RawBody = string(responseBody)
		}
		return nil, httpError
	}
	if readErr != nil {
		return nil, fmt.Errorf("read Templiqx response: %w", readErr)
	}
	value, err := decode(responseBody)
	if err != nil {
		return nil, fmt.Errorf("decode Templiqx response: %w", err)
	}
	return &TempliqxResponse[T]{Data: value, RequestID: effectiveRequestID}, nil
}

func decodeOperationEnvelope(body []byte) *OperationEnvelopeBase {
	var shape map[string]json.RawMessage
	if err := json.Unmarshal(body, &shape); err != nil {
		return nil
	}
	var diagnostics []json.RawMessage
	if err := json.Unmarshal(shape["diagnostics"], &diagnostics); err != nil || diagnostics == nil {
		return nil
	}
	var envelope OperationEnvelopeBase
	if err := json.Unmarshal(body, &envelope); err != nil {
		return nil
	}
	return &envelope
}

func newRequestID() (string, error) {
	var value [16]byte
	if _, err := rand.Read(value[:]); err != nil {
		return "", err
	}
	value[6] = (value[6] & 0x0f) | 0x40
	value[8] = (value[8] & 0x3f) | 0x80
	encoded := make([]byte, 36)
	hex.Encode(encoded[0:8], value[0:4])
	encoded[8] = '-'
	hex.Encode(encoded[9:13], value[4:6])
	encoded[13] = '-'
	hex.Encode(encoded[14:18], value[6:8])
	encoded[18] = '-'
	hex.Encode(encoded[19:23], value[8:10])
	encoded[23] = '-'
	hex.Encode(encoded[24:36], value[10:16])
	return string(encoded), nil
}

func segment(value string) string { return url.PathEscape(value) }

func artifactPath(value string) string {
	parts := strings.Split(value, "/")
	for index := range parts {
		parts[index] = segment(parts[index])
	}
	return strings.Join(parts, "/")
}

func packagePath(packageName string) string {
	return "/operations/v1/packages/" + segment(packageName)
}

func contractPath(packageName, contract string) string {
	return packagePath(packageName) + "/contracts/" + segment(contract)
}

// GetOperationsV1Liveness calls getOperationsV1Liveness.
func (c *Client) GetOperationsV1Liveness(ctx context.Context, options ...CallOption) (*TempliqxResponse[HealthStatus], error) {
	return dispatchJSON[HealthStatus](ctx, c, dispatchRequest{method: http.MethodGet, path: "/operations/v1/health/live"}, options...)
}

// GetOperationsV1Readiness calls getOperationsV1Readiness.
func (c *Client) GetOperationsV1Readiness(ctx context.Context, options ...CallOption) (*TempliqxResponse[HealthStatus], error) {
	return dispatchJSON[HealthStatus](ctx, c, dispatchRequest{method: http.MethodGet, path: "/operations/v1/health/ready"}, options...)
}

// GetOperationsV1OpenAPIYaml calls getOperationsV1OpenApiYaml.
func (c *Client) GetOperationsV1OpenAPIYaml(ctx context.Context, options ...CallOption) (*TempliqxResponse[string], error) {
	return dispatchText(ctx, c, dispatchRequest{method: http.MethodGet, path: "/operations/v1/openapi.yaml"}, options...)
}

// GetOperationsV1OpenAPI calls getOperationsV1OpenApi.
func (c *Client) GetOperationsV1OpenAPI(ctx context.Context, options ...CallOption) (*TempliqxResponse[map[string]any], error) {
	return dispatchJSON[map[string]any](ctx, c, dispatchRequest{method: http.MethodGet, path: "/operations/v1/openapi.json"}, options...)
}

// Catalog calls catalog.
func (c *Client) Catalog(ctx context.Context, options ...CallOption) (*TempliqxResponse[CatalogEnvelope], error) {
	return dispatchJSON[CatalogEnvelope](ctx, c, dispatchRequest{method: http.MethodGet, path: "/operations/v1/catalog"}, options...)
}

// DiscoverPackages calls discoverPackages.
func (c *Client) DiscoverPackages(ctx context.Context, options ...CallOption) (*TempliqxResponse[PackageListEnvelope], error) {
	return dispatchJSON[PackageListEnvelope](ctx, c, dispatchRequest{method: http.MethodGet, path: "/operations/v1/packages"}, options...)
}

// CreatePackage calls createPackage.
func (c *Client) CreatePackage(ctx context.Context, body CreatePackageRequest, options ...CallOption) (*TempliqxResponse[PackageEnvelope], error) {
	return dispatchJSON[PackageEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: "/operations/v1/packages", body: body}, options...)
}

// InspectContract calls inspectContract.
func (c *Client) InspectContract(ctx context.Context, packageName, contract string, options ...CallOption) (*TempliqxResponse[ContractEnvelope], error) {
	return dispatchJSON[ContractEnvelope](ctx, c, dispatchRequest{method: http.MethodGet, path: contractPath(packageName, contract)}, options...)
}

// PutContract calls putContract.
func (c *Client) PutContract(ctx context.Context, packageName, contract, source string, options ...CallOption) (*TempliqxResponse[SummaryEnvelope], error) {
	return dispatchJSON[SummaryEnvelope](ctx, c, dispatchRequest{
		method: http.MethodPut, path: contractPath(packageName, contract), rawBody: &source,
		contentType: "application/yaml", cas: true,
	}, options...)
}

// DeleteContract calls deleteContract.
func (c *Client) DeleteContract(ctx context.Context, packageName, contract string, ifMatch IfMatchOption, options ...CallOption) (*TempliqxResponse[SummaryEnvelope], error) {
	return dispatchJSON[SummaryEnvelope](ctx, c, dispatchRequest{method: http.MethodDelete, path: contractPath(packageName, contract), cas: true}, append([]CallOption{ifMatch}, options...)...)
}

// ValidateContract calls validateContract.
func (c *Client) ValidateContract(ctx context.Context, packageName, contract string, options ...CallOption) (*TempliqxResponse[SummaryEnvelope], error) {
	return dispatchJSON[SummaryEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: contractPath(packageName, contract) + "/validate"}, options...)
}

// CompileContract calls compileContract.
func (c *Client) CompileContract(ctx context.Context, packageName, contract string, body CompileRequest, options ...CallOption) (*TempliqxResponse[CompiledInteractionEnvelope], error) {
	return dispatchJSON[CompiledInteractionEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: contractPath(packageName, contract) + "/compile", body: body}, options...)
}

// ExecuteContract calls executeContract.
func (c *Client) ExecuteContract(ctx context.Context, packageName, contract string, body ExecuteRequest, options ...CallOption) (*TempliqxResponse[ExecutionReceiptEnvelope], error) {
	return dispatchJSON[ExecutionReceiptEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: contractPath(packageName, contract) + "/execute", body: body}, options...)
}

// UpdatePackage calls updatePackage.
func (c *Client) UpdatePackage(ctx context.Context, packageName string, body UpdatePackageRequest, ifMatch IfMatchOption, options ...CallOption) (*TempliqxResponse[PackageEnvelope], error) {
	return dispatchJSON[PackageEnvelope](ctx, c, dispatchRequest{method: http.MethodPatch, path: packagePath(packageName), body: body, cas: true}, append([]CallOption{ifMatch}, options...)...)
}

// DeletePackage calls deletePackage.
func (c *Client) DeletePackage(ctx context.Context, packageName string, ifMatch IfMatchOption, options ...CallOption) (*TempliqxResponse[PackageEnvelope], error) {
	return dispatchJSON[PackageEnvelope](ctx, c, dispatchRequest{method: http.MethodDelete, path: packagePath(packageName), cas: true}, append([]CallOption{ifMatch}, options...)...)
}

// ValidatePackage calls validatePackage.
func (c *Client) ValidatePackage(ctx context.Context, packageName string, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: packagePath(packageName) + "/validate"}, options...)
}

// TestPackage calls testPackage.
func (c *Client) TestPackage(ctx context.Context, packageName string, body CapabilitiesRequest, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: packagePath(packageName) + "/test", body: body}, options...)
}

// ExportPackageIdentity calls exportPackageIdentity.
func (c *Client) ExportPackageIdentity(ctx context.Context, packageName string, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodGet, path: packagePath(packageName) + "/identity"}, options...)
}

// SignPackage calls signPackage.
func (c *Client) SignPackage(ctx context.Context, packageName string, body SignPackageRequest, ifMatch IfMatchOption, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: packagePath(packageName) + "/sign", body: body, cas: true}, append([]CallOption{ifMatch}, options...)...)
}

// VerifyPackageTrust calls verifyPackageTrust.
func (c *Client) VerifyPackageTrust(ctx context.Context, packageName string, body VerifyPackageTrustRequest, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: packagePath(packageName) + "/verify-trust", body: body}, options...)
}

// ListEvals calls listEvals.
func (c *Client) ListEvals(ctx context.Context, packageName string, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodGet, path: packagePath(packageName) + "/evals"}, options...)
}

// RunEval calls runEval.
func (c *Client) RunEval(ctx context.Context, packageName string, body RunEvalRequest, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: packagePath(packageName) + "/evals/run", body: body}, options...)
}

// RenderContract calls renderContract.
func (c *Client) RenderContract(ctx context.Context, packageName, contract string, body CompileRequest, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: contractPath(packageName, contract) + "/render", body: body}, options...)
}

// DiffContract calls diffContract.
func (c *Client) DiffContract(ctx context.Context, packageName, contract string, body DiffContractRequest, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: contractPath(packageName, contract) + "/diff", body: body}, options...)
}

// ExplainContract calls explainContract.
func (c *Client) ExplainContract(ctx context.Context, packageName, contract string, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodGet, path: contractPath(packageName, contract) + "/explain"}, options...)
}

// MigrateLegacy calls migrateLegacy.
func (c *Client) MigrateLegacy(ctx context.Context, body MigrateLegacyRequest, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: "/operations/v1/legacy/migrate", body: body}, options...)
}

// RenderDocument calls renderDocument.
func (c *Client) RenderDocument(ctx context.Context, body RenderDocumentRequest, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: "/operations/v1/documents/render", body: body}, options...)
}

// InspectDocument calls inspectDocument.
func (c *Client) InspectDocument(ctx context.Context, body InspectDocumentRequest, options ...CallOption) (*TempliqxResponse[InspectDocumentEnvelope], error) {
	return dispatchJSON[InspectDocumentEnvelope](ctx, c, dispatchRequest{method: http.MethodPost, path: "/operations/v1/documents/inspect", body: body}, options...)
}

// ListWorkspaceArtifacts calls listWorkspaceArtifacts.
func (c *Client) ListWorkspaceArtifacts(ctx context.Context, packageName string, workspace, prefix *string, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	query := url.Values{"package": {packageName}}
	if workspace != nil {
		query.Set("workspace", *workspace)
	}
	if prefix != nil {
		query.Set("prefix", *prefix)
	}
	path := "/operations/v1/artifacts?" + query.Encode()
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodGet, path: path}, options...)
}

// ReadArtifact calls readArtifact.
func (c *Client) ReadArtifact(ctx context.Context, artifact, packageName string, workspace *string, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	query := url.Values{"package": {packageName}}
	if workspace != nil {
		query.Set("workspace", *workspace)
	}
	path := "/operations/v1/artifacts/" + artifactPath(artifact) + "?" + query.Encode()
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodGet, path: path}, options...)
}

// DeleteWorkspaceArtifact calls deleteWorkspaceArtifact.
func (c *Client) DeleteWorkspaceArtifact(ctx context.Context, artifact, packageName string, workspace *string, ifMatch IfMatchOption, options ...CallOption) (*TempliqxResponse[JsonValueEnvelope], error) {
	query := url.Values{"package": {packageName}}
	if workspace != nil {
		query.Set("workspace", *workspace)
	}
	path := "/operations/v1/artifacts/" + artifactPath(artifact) + "?" + query.Encode()
	return dispatchJSON[JsonValueEnvelope](ctx, c, dispatchRequest{method: http.MethodDelete, path: path, cas: true}, append([]CallOption{ifMatch}, options...)...)
}
