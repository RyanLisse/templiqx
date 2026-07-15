package templiqx

import "fmt"

// TempliqxTransportError reports a failure before a valid HTTP response was
// available, including network failures and context cancellation.
type TempliqxTransportError struct {
	RequestID string
	Err       error
}

func (e *TempliqxTransportError) Error() string {
	if e.RequestID == "" {
		return fmt.Sprintf("templiqx transport error: %v", e.Err)
	}
	return fmt.Sprintf("templiqx transport error (request %s): %v", e.RequestID, e.Err)
}

// Unwrap exposes the underlying net/http or context error.
func (e *TempliqxTransportError) Unwrap() error { return e.Err }

// TempliqxHTTPError reports a non-success HTTP response. Envelope is populated
// when the response body is a Templiqx operation envelope; otherwise RawBody is
// preserved for transport diagnosis.
type TempliqxHTTPError struct {
	StatusCode int
	Envelope   *OperationEnvelopeBase
	RawBody    string
	RequestID  string
}

func (e *TempliqxHTTPError) Error() string {
	return fmt.Sprintf("templiqx HTTP error %d (request %s)", e.StatusCode, e.RequestID)
}
