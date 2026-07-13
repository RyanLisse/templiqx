use templiqx_application::{
    sign_package_identity, verify_package_signatures, verify_package_signatures_with_mode,
};
use templiqx_contracts::{Diagnostic, PackageSignature, Severity};
use templiqx_local::FilesystemPackageStore;
use templiqx_ports::PackageStore;

fn packages_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/packages")
        .canonicalize()
        .expect("examples/packages root")
}

#[test]
fn unsigned_package_passes_default_validation() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let service =
        templiqx_local::compose_with_workspace(packages_root(), workspace.path()).expect("service");
    let envelope = service.validate_package("demo");
    assert!(envelope.ok, "{:?}", envelope.diagnostics);
}

#[test]
fn signed_package_verifies_with_matching_key() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(temp.path().join("signed/contracts")).expect("package dirs");
    std::fs::write(
        temp.path().join("signed/templiqx.yaml"),
        r#"api_version: templiqx/v1alpha1
package: signed
version: 0.1.0
description: signed package fixture
contracts:
  - greeting
components: []
evals: []
migrations: []
templates: []
provenance:
  owner: templiqx
"#,
    )
    .expect("manifest");
    std::fs::copy(
        packages_root().join("demo/contracts/greeting.yaml"),
        temp.path().join("signed/contracts/greeting.yaml"),
    )
    .expect("contract copy");

    let store = FilesystemPackageStore::new(temp.path()).expect("store");
    let manifest = store.manifest("signed").expect("manifest");
    let mut normalized = manifest.clone();
    normalized.signatures.clear();
    normalized.contracts.sort();
    let artifact_hashes = std::collections::BTreeMap::from([(
        "contracts/greeting.yaml".to_string(),
        templiqx_contracts::fingerprint_bytes(
            &store
                .artifact_bytes("signed", "contracts/greeting.yaml")
                .expect("contract bytes"),
        ),
    )]);
    let package_identity =
        serde_json::json!({"manifest": normalized, "artifacts": artifact_hashes});
    let signature =
        sign_package_identity(&package_identity, b"templiqx-test-signing-key", "ci-test")
            .expect("signature");
    let mut diagnostics = Vec::<Diagnostic>::new();
    verify_package_signatures(
        &package_identity,
        std::slice::from_ref(&signature),
        Some("templiqx-test-signing-key".into()),
        &mut diagnostics,
    );
    assert!(
        diagnostics.is_empty(),
        "expected valid signature: {diagnostics:?}"
    );
}

#[test]
fn signature_attachment_rejects_stale_full_identity() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(temp.path().join("signed/contracts")).expect("package dirs");
    std::fs::write(
        temp.path().join("signed/templiqx.yaml"),
        r#"api_version: templiqx/v1alpha1
package: signed
version: 0.1.0
contracts: [greeting]
"#,
    )
    .expect("manifest");
    std::fs::copy(
        packages_root().join("demo/contracts/greeting.yaml"),
        temp.path().join("signed/contracts/greeting.yaml"),
    )
    .expect("contract copy");
    let store = FilesystemPackageStore::new(temp.path()).expect("store");
    let manifest = store.manifest("signed").expect("manifest");
    let manifest_fingerprint = templiqx_contracts::fingerprint(&manifest).expect("fingerprint");
    let identity = store.package_identity("signed").expect("identity");
    let identity_fingerprint = templiqx_contracts::fingerprint(&identity).expect("fingerprint");
    let signature = sign_package_identity(&identity, b"key", "dev").expect("signature");

    std::fs::write(
        temp.path().join("signed/contracts/greeting.yaml"),
        b"changed after identity export",
    )
    .expect("mutate artifact");
    let error = store
        .attach_package_signature(
            "signed",
            signature,
            &manifest_fingerprint,
            &identity_fingerprint,
        )
        .expect_err("stale identity must conflict");
    assert!(matches!(error, templiqx_ports::PortError::Conflict(_)));
    assert!(
        store
            .manifest("signed")
            .expect("manifest")
            .signatures
            .is_empty()
    );
}

#[test]
fn package_lockfile_tampering_invalidates_signed_identity() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(temp.path().join("signed/contracts")).expect("package dirs");
    std::fs::write(
        temp.path().join("signed/templiqx.yaml"),
        r#"api_version: templiqx/v1alpha1
package: signed
version: 0.1.0
contracts: [greeting]
"#,
    )
    .expect("manifest");
    std::fs::copy(
        packages_root().join("demo/contracts/greeting.yaml"),
        temp.path().join("signed/contracts/greeting.yaml"),
    )
    .expect("contract copy");
    std::fs::write(
        temp.path().join("signed/templiqx.lock"),
        "dependencies:\n  shared:\n    path: ../shared\n    fingerprint: sha256:abc\n",
    )
    .expect("lockfile");
    let store = FilesystemPackageStore::new(temp.path()).expect("store");
    let identity = store.package_identity("signed").expect("identity");
    assert!(identity.artifacts.contains_key("templiqx.lock"));
    let signature = sign_package_identity(&identity, b"key", "dev").expect("signature");

    std::fs::write(
        temp.path().join("signed/templiqx.lock"),
        "dependencies:\n  shared:\n    path: ../attacker-controlled\n    fingerprint: sha256:abc\n",
    )
    .expect("tamper lockfile");
    let tampered = store.package_identity("signed").expect("tampered identity");
    let mut diagnostics = Vec::new();
    verify_package_signatures_with_mode(
        &tampered,
        &[signature],
        Some("key".into()),
        true,
        &mut diagnostics,
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "TQX_PACKAGE_SIGNATURE_INVALID" && diagnostic.severity == Severity::Error
    }));
}

#[test]
fn tampered_signature_fails_validation() {
    let package_identity = serde_json::json!({"manifest": {"package": "signed"}, "artifacts": {}});
    let signature = PackageSignature {
        key_id: "test".into(),
        algorithm: "sha256-keyed".into(),
        value: "deadbeef".into(),
    };
    let mut diagnostics = Vec::<Diagnostic>::new();
    verify_package_signatures(
        &package_identity,
        std::slice::from_ref(&signature),
        Some("templiqx-test-signing-key".into()),
        &mut diagnostics,
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "TQX_PACKAGE_SIGNATURE_INVALID" && diagnostic.severity == Severity::Error
    }));
}

#[test]
fn wrong_key_and_cross_package_replay_fail() {
    let first = serde_json::json!({"manifest":{"package":"first"},"artifacts":{}});
    let second = serde_json::json!({"manifest":{"package":"second"},"artifacts":{}});
    let signature = sign_package_identity(&first, b"right-key", "dev").expect("signature");
    for (identity, key) in [(&first, "wrong-key"), (&second, "right-key")] {
        let mut diagnostics = Vec::new();
        verify_package_signatures_with_mode(
            identity,
            std::slice::from_ref(&signature),
            Some(key.into()),
            true,
            &mut diagnostics,
        );
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "TQX_PACKAGE_SIGNATURE_INVALID"
                && diagnostic.severity == Severity::Error
        }));
    }
}

#[test]
fn signature_binds_key_identity_and_algorithm() {
    let identity = serde_json::json!({"manifest":{"package":"signed"},"artifacts":{}});
    let valid = sign_package_identity(&identity, b"key", "dev").expect("signature");
    for forged in [
        PackageSignature {
            key_id: "other".into(),
            ..valid.clone()
        },
        PackageSignature {
            algorithm: "sha256-keyed-v2".into(),
            ..valid.clone()
        },
    ] {
        let mut diagnostics = Vec::new();
        verify_package_signatures_with_mode(
            &identity,
            &[forged],
            Some("key".into()),
            true,
            &mut diagnostics,
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error)
        );
    }
}

#[test]
fn mixed_valid_and_forged_supported_signatures_fail_closed() {
    let identity = serde_json::json!({"manifest":{"package":"signed"},"artifacts":{}});
    let valid = sign_package_identity(&identity, b"key", "valid").expect("signature");
    let mut forged = sign_package_identity(&identity, b"key", "forged").expect("signature");
    forged.value = "00".repeat(32);
    let mut diagnostics = Vec::new();
    let verified = verify_package_signatures_with_mode(
        &identity,
        &[valid, forged],
        Some("key".into()),
        true,
        &mut diagnostics,
    );
    assert!(
        verified.is_empty(),
        "partial verification must not be accepted"
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "TQX_PACKAGE_SIGNATURE_INVALID" && diagnostic.severity == Severity::Error
    }));
}

#[test]
fn strict_unsigned_is_a_publication_error() {
    let identity = serde_json::json!({"manifest":{"package":"unsigned"},"artifacts":{}});
    let mut diagnostics = Vec::new();
    verify_package_signatures_with_mode(&identity, &[], Some("key".into()), true, &mut diagnostics);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "TQX_PACKAGE_UNSIGNED" && diagnostic.severity == Severity::Error
    }));
}

#[test]
fn duplicate_and_unsupported_signatures_fail_closed() {
    let identity = serde_json::json!({"manifest":{"package":"signed"},"artifacts":{}});
    let valid = sign_package_identity(&identity, b"key", "dev").expect("signature");
    let unsupported = PackageSignature {
        key_id: "external".into(),
        algorithm: "cosign".into(),
        value: "not-an-embedded-cosign-signature".into(),
    };
    let mut diagnostics = Vec::new();
    verify_package_signatures_with_mode(
        &identity,
        &[valid.clone(), valid, unsupported],
        Some("key".into()),
        true,
        &mut diagnostics,
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TQX_PACKAGE_SIGNATURE_DUPLICATE")
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "TQX_PACKAGE_SIGNATURE_ALGORITHM_UNSUPPORTED" })
    );
}

#[test]
fn signed_without_verification_key_fails_closed() {
    let identity = serde_json::json!({"manifest":{"package":"signed"},"artifacts":{}});
    let signature = sign_package_identity(&identity, b"key", "dev").expect("signature");
    let mut diagnostics = Vec::new();
    verify_package_signatures_with_mode(&identity, &[signature], None, true, &mut diagnostics);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TQX_PACKAGE_SIGNATURE_UNVERIFIED")
    );
}
