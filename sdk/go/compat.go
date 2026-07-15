package templiqx

import (
	"fmt"
	"regexp"
)

// CompatibilityMetadata is the SDK compatibility record generated from the
// repository compatibility matrix.
type CompatibilityMetadata struct {
	EngineVersion  string
	OpsApiVersion  string
	OpenApiDigest  string
	ContractFormat string
	SdkVersion     string
}

// Compatibility is derived exclusively from the checked-in generated markers.
var Compatibility = CompatibilityMetadata{
	EngineVersion:  GeneratedEngineVersion,
	OpsApiVersion:  GeneratedOpenAPIVersion,
	OpenApiDigest:  GeneratedOpenAPIDigest,
	ContractFormat: GeneratedContractFormat,
	SdkVersion:     GeneratedSDKVersion,
}

var openAPIDigestPattern = regexp.MustCompile(`^sha256:[a-f0-9]{64}$`)

// AssertCompatibility checks that the public metadata remains wired to the
// generated OpenAPI digest marker.
func AssertCompatibility() error {
	if Compatibility.OpenApiDigest != GeneratedOpenAPIDigest {
		return fmt.Errorf("compatibility digest does not match generated marker")
	}
	if !openAPIDigestPattern.MatchString(Compatibility.OpenApiDigest) {
		return fmt.Errorf("generated OpenAPI digest is malformed")
	}
	if Compatibility.EngineVersion != GeneratedEngineVersion {
		return fmt.Errorf("compatibility engine version does not match generated marker")
	}
	return nil
}
