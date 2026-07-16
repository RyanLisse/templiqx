//! Host-supplied authorized merge context validation.

use chrono::{DateTime, Utc};
use serde_json::json;
use templiqx_contracts::{
    AUTHORIZED_MERGE_CONTEXT_KEY, AuthorizedMergeContext, Diagnostic, PackageManifest,
    RenderRequest, fingerprint,
};

const REQUIRES_AUTHORIZED_CONTEXT: &str = "requires_authorized_context";

/// Whether a package manifest declares that operations require host authorization.
///
/// # Errors
///
/// Returns a diagnostic when the manifest contains an unsupported requirement value.
pub fn package_requires_authorized_context(
    manifest: &PackageManifest,
) -> Result<bool, Vec<Diagnostic>> {
    match manifest.provenance.get(REQUIRES_AUTHORIZED_CONTEXT) {
        None => Ok(false),
        Some(value) if value == "true" => Ok(true),
        Some(value) if value == "false" => Ok(false),
        Some(value) => Err(vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_REQUIREMENT_INVALID",
            format!("requires_authorized_context must be 'true' or 'false', got '{value}'"),
            "/provenance/requires_authorized_context",
        )]),
    }
}

/// Fingerprints the binding fields of an authorized merge context, excluding the
/// host-declared fingerprint claim itself.
///
/// # Errors
///
/// Returns a serialization error when binding fields cannot be canonicalized.
pub fn binding_fingerprint(context: &AuthorizedMergeContext) -> Result<String, serde_json::Error> {
    fingerprint(&json!({
        "scope_id": context.scope_id,
        "policy_decision_id": context.policy_decision_id,
        "policy_version": context.policy_version,
        "evidence_provenance_id": context.evidence_provenance_id,
        "issued_at": context.issued_at,
        "expires_at": context.expires_at,
    }))
}

/// Validates presence, freshness, and fingerprint binding for packages that
/// require host authorization.
pub fn validate_authorized_context(
    manifest: &PackageManifest,
    request: &RenderRequest,
) -> Result<Option<AuthorizedMergeContext>, Vec<Diagnostic>> {
    if !package_requires_authorized_context(manifest)? {
        return Ok(None);
    }
    let Some(raw) = request.context.get(AUTHORIZED_MERGE_CONTEXT_KEY) else {
        return Err(vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_MISSING",
            "authorized merge context is required for this package",
            "/context/_templiqx_authorized_merge",
        )]);
    };
    let context: AuthorizedMergeContext = serde_json::from_value(raw.clone()).map_err(|error| {
        vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_INVALID",
            error.to_string(),
            "/context/_templiqx_authorized_merge",
        )]
    })?;
    if context.scope_id.trim().is_empty() {
        return Err(vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_REDACTED",
            "authorized scope is missing or redacted",
            "/context/_templiqx_authorized_merge/scope_id",
        )]);
    }
    let issued_at = DateTime::parse_from_rfc3339(&context.issued_at).map_err(|error| {
        vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_INVALID",
            format!("issued_at is not RFC3339: {error}"),
            "/context/_templiqx_authorized_merge/issued_at",
        )]
    })?;
    let expires_at = DateTime::parse_from_rfc3339(&context.expires_at).map_err(|error| {
        vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_INVALID",
            format!("expires_at is not RFC3339: {error}"),
            "/context/_templiqx_authorized_merge/expires_at",
        )]
    })?;
    let now = Utc::now();
    if expires_at < now {
        return Err(vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_EXPIRED",
            "authorized merge context has expired",
            "/context/_templiqx_authorized_merge/expires_at",
        )]);
    }
    if issued_at > expires_at {
        return Err(vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_INVALID",
            "authorized merge context was issued after it expires",
            "/context/_templiqx_authorized_merge/issued_at",
        )]);
    }
    if issued_at > now {
        return Err(vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_INVALID",
            "authorized merge context was issued in the future",
            "/context/_templiqx_authorized_merge/issued_at",
        )]);
    }
    let expected = binding_fingerprint(&context).map_err(|error| {
        vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_INVALID",
            error.to_string(),
            "/context/_templiqx_authorized_merge",
        )]
    })?;
    if context.fingerprint != expected {
        return Err(vec![Diagnostic::error(
            "TQX_AUTHORIZED_CONTEXT_MISMATCH",
            "authorized merge context fingerprint does not match binding fields",
            "/context/_templiqx_authorized_merge/fingerprint",
        )]);
    }
    Ok(Some(context))
}

/// Builds a sanitized authorized context for conformance fixtures.
#[cfg(test)]
#[must_use]
pub fn synthetic_authorized_context(scope_id: &str) -> AuthorizedMergeContext {
    let mut context = AuthorizedMergeContext {
        scope_id: scope_id.into(),
        policy_decision_id: "SYN-POLICY-DEC-001".into(),
        policy_version: "1.0.0".into(),
        evidence_provenance_id: "SYN-EVID-PROV-001".into(),
        issued_at: "2026-07-15T10:00:00Z".into(),
        expires_at: "2099-12-31T23:59:59Z".into(),
        fingerprint: String::new(),
    };
    context.fingerprint = binding_fingerprint(&context).expect("synthetic context fingerprint");
    context
}

/// Injects a synthetic authorized merge context into a render request.
#[cfg(test)]
#[must_use]
pub fn with_synthetic_authorized_context(
    mut request: RenderRequest,
    scope_id: &str,
) -> RenderRequest {
    let context = synthetic_authorized_context(scope_id);
    request.context.insert(
        AUTHORIZED_MERGE_CONTEXT_KEY.into(),
        serde_json::to_value(context).expect("authorized context serializes"),
    );
    request
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use templiqx_contracts::API_VERSION;

    fn manifest_with_requirement(requirement: Option<&str>) -> PackageManifest {
        let mut provenance = BTreeMap::new();
        if let Some(requirement) = requirement {
            provenance.insert(REQUIRES_AUTHORIZED_CONTEXT.into(), requirement.into());
        }
        PackageManifest {
            api_version: API_VERSION.into(),
            package: "test".into(),
            version: "0.1.0".into(),
            description: String::new(),
            contracts: vec![],
            components: vec![],
            evals: vec![],
            migrations: vec![],
            templates: vec![],
            provenance,
            signatures: vec![],
            dependencies: BTreeMap::new(),
            tool_contracts: BTreeMap::new(),
            translations: vec![],
        }
    }

    fn manifest(requires: bool) -> PackageManifest {
        manifest_with_requirement(requires.then_some("true"))
    }

    #[test]
    fn synthetic_context_passes_validation() {
        let request = with_synthetic_authorized_context(
            RenderRequest {
                inputs: BTreeMap::new(),
                context: BTreeMap::new(),
            },
            "SYN-SCOPE-001",
        );
        let validated =
            validate_authorized_context(&manifest(true), &request).expect("validation succeeds");
        assert!(validated.is_some());
    }

    #[test]
    fn print_scope_fingerprints() {
        for scope_id in [
            "SYN-LEGAL-SCOPE-001",
            "SYN-ADVICE-SCOPE-001",
            "SYN-PROJECT-SCOPE-001",
        ] {
            let context = synthetic_authorized_context(scope_id);
            println!("{scope_id}: {}", context.fingerprint);
        }
    }

    #[test]
    fn missing_context_fails_closed() {
        let request = RenderRequest {
            inputs: BTreeMap::new(),
            context: BTreeMap::new(),
        };
        let error = validate_authorized_context(&manifest(true), &request).expect_err("missing");
        assert_eq!(error[0].code, "TQX_AUTHORIZED_CONTEXT_MISSING");
    }

    #[test]
    fn requirement_accepts_absent_and_boolean_strings() {
        assert!(!package_requires_authorized_context(&manifest_with_requirement(None)).unwrap());
        assert!(
            package_requires_authorized_context(&manifest_with_requirement(Some("true"))).unwrap()
        );
        assert!(
            !package_requires_authorized_context(&manifest_with_requirement(Some("false")))
                .unwrap()
        );
    }

    #[test]
    fn invalid_requirement_is_diagnostic() {
        let error = package_requires_authorized_context(&manifest_with_requirement(Some("yes")))
            .expect_err("unsupported value");
        assert_eq!(error[0].code, "TQX_AUTHORIZED_CONTEXT_REQUIREMENT_INVALID");
        assert_eq!(
            error[0].json_pointer.as_deref(),
            Some("/provenance/requires_authorized_context")
        );
    }

    #[test]
    fn malformed_issued_at_is_invalid() {
        let mut context = synthetic_authorized_context("SYN-SCOPE-001");
        context.issued_at = "not-a-timestamp".into();
        let request = RenderRequest {
            inputs: BTreeMap::new(),
            context: BTreeMap::from([(
                AUTHORIZED_MERGE_CONTEXT_KEY.into(),
                serde_json::to_value(context).unwrap(),
            )]),
        };
        let error = validate_authorized_context(&manifest(true), &request)
            .expect_err("malformed issued_at");
        assert_eq!(error[0].code, "TQX_AUTHORIZED_CONTEXT_INVALID");
        assert_eq!(
            error[0].json_pointer.as_deref(),
            Some("/context/_templiqx_authorized_merge/issued_at")
        );
    }

    #[test]
    fn future_issued_at_is_invalid() {
        let mut context = synthetic_authorized_context("SYN-SCOPE-001");
        context.issued_at = "2099-01-01T00:00:00Z".into();
        let request = RenderRequest {
            inputs: BTreeMap::new(),
            context: BTreeMap::from([(
                AUTHORIZED_MERGE_CONTEXT_KEY.into(),
                serde_json::to_value(context).unwrap(),
            )]),
        };
        let error =
            validate_authorized_context(&manifest(true), &request).expect_err("future issued_at");
        assert_eq!(error[0].code, "TQX_AUTHORIZED_CONTEXT_INVALID");
        assert!(error[0].message.contains("future"));
    }

    #[test]
    fn issued_at_after_expiration_is_invalid() {
        let mut context = synthetic_authorized_context("SYN-SCOPE-001");
        context.issued_at = "2099-12-31T23:59:59Z".into();
        context.expires_at = "2099-01-01T00:00:00Z".into();
        let request = RenderRequest {
            inputs: BTreeMap::new(),
            context: BTreeMap::from([(
                AUTHORIZED_MERGE_CONTEXT_KEY.into(),
                serde_json::to_value(context).unwrap(),
            )]),
        };
        let error = validate_authorized_context(&manifest(true), &request)
            .expect_err("issued after expiration");
        assert_eq!(error[0].code, "TQX_AUTHORIZED_CONTEXT_INVALID");
        assert!(error[0].message.contains("after it expires"));
    }
}
