use templiqx_application::{sign_package_identity, verify_package_signatures};
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
