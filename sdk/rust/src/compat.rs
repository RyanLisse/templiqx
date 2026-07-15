//! Generated-contract compatibility metadata.

use crate::generated::{
    GENERATED_CONTRACT_FORMAT, GENERATED_ENGINE_VERSION, GENERATED_OPENAPI_DIGEST,
    GENERATED_OPENAPI_VERSION, GENERATED_SDK_VERSION,
};

/// Compatibility facts carried by this SDK build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compatibility {
    pub engine_version: &'static str,
    pub ops_api_version: &'static str,
    pub openapi_digest: &'static str,
    pub contract_format: &'static str,
    pub sdk_version: &'static str,
}

/// Compatibility facts derived at DTO generation time.
pub const COMPATIBILITY: Compatibility = Compatibility {
    engine_version: GENERATED_ENGINE_VERSION,
    ops_api_version: GENERATED_OPENAPI_VERSION,
    openapi_digest: GENERATED_OPENAPI_DIGEST,
    contract_format: GENERATED_CONTRACT_FORMAT,
    sdk_version: GENERATED_SDK_VERSION,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_digest_is_current_marker() {
        assert!(COMPATIBILITY.openapi_digest.starts_with("sha256:"));
        assert_eq!(COMPATIBILITY.openapi_digest.len(), "sha256:".len() + 64);
        assert_eq!(COMPATIBILITY.openapi_digest, GENERATED_OPENAPI_DIGEST);
    }
}
