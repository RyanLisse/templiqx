use std::fs;
use templiqx_contracts::RenderRequest;

const CONTRACT: &str = r#"
api_version: templiqx/v1alpha1
id: greeting
version: 0.1.0
inputs:
  name:
    schema: {type: string}
    required: true
messages:
  - role: user
    content:
      - kind: text
        value: "Hello "
      - kind: interpolate
        expression: {kind: ref, path: inputs.name}
output_schema: {type: object, required: [message], properties: {message: {type: string}}}
evals:
  - id: simple
    inputs: {name: Ryan}
    fake_output: {message: "Hello Ryan"}
"#;

#[test]
fn service_create_compile_test_and_cas() {
    let temp = tempfile::tempdir().unwrap();
    templiqx_local::create_package(temp.path(), "demo", "0.1.0").unwrap();
    let service = templiqx_local::compose(temp.path()).unwrap();
    let created = service.put_contract("demo", "greeting", CONTRACT, None);
    assert!(created.ok, "{:?}", created.diagnostics);
    let hash = created.fingerprints["contract"].clone();
    let manifest = fs::read_to_string(temp.path().join("demo/templiqx.yaml")).unwrap();
    assert!(manifest.contains("greeting"));
    let request: RenderRequest =
        serde_json::from_value(serde_json::json!({"inputs":{"name":"Ryan"}})).unwrap();
    let compiled = service.compile_contract("demo", "greeting", &request, &[]);
    assert!(compiled.ok, "{:?}", compiled.diagnostics);
    assert_eq!(compiled.result.unwrap().messages[0].content, "Hello Ryan");
    assert!(service.test_package("demo", &[]).ok);
    let conflict = service.put_contract("demo", "greeting", CONTRACT, Some("wrong"));
    assert!(!conflict.ok);
    assert_eq!(conflict.diagnostics[0].code, "TQX_CAS_CONFLICT");
    let updated = service.put_contract(
        "demo",
        "greeting",
        &CONTRACT.replace("version: 0.1.0", "version: 0.1.1"),
        Some(&hash),
    );
    assert!(updated.ok, "{:?}", updated.diagnostics);
}

fn package_with_contract() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    templiqx_local::create_package(temp.path(), "demo", "0.1.0").unwrap();
    let service = templiqx_local::compose_core(temp.path()).unwrap();
    assert!(service.put_contract("demo", "greeting", CONTRACT, None).ok);
    temp
}

fn update_manifest(
    temp: &tempfile::TempDir,
    update: impl FnOnce(&mut templiqx_contracts::PackageManifest),
) {
    let path = temp.path().join("demo/templiqx.yaml");
    let mut manifest: templiqx_contracts::PackageManifest =
        serde_yaml_ng::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    update(&mut manifest);
    fs::write(path, serde_yaml_ng::to_string(&manifest).unwrap()).unwrap();
}

#[test]
fn package_identity_hashes_every_artifact_byte_and_is_inventory_order_independent() {
    let temp = package_with_contract();
    fs::write(temp.path().join("demo/evals/a.json"), b"one").unwrap();
    fs::write(temp.path().join("demo/evals/b.json"), b"two").unwrap();
    update_manifest(&temp, |manifest| {
        manifest.evals = vec!["evals/b.json".into(), "evals/a.json".into()];
    });
    let service = templiqx_local::compose_core(temp.path()).unwrap();
    let first = service.validate_package("demo");
    assert!(first.ok, "{:?}", first.diagnostics);
    let first_hash = first.fingerprints["package"].clone();

    update_manifest(&temp, |manifest| manifest.evals.reverse());
    let reordered = service.validate_package("demo");
    assert!(reordered.ok, "{:?}", reordered.diagnostics);
    assert_eq!(first_hash, reordered.fingerprints["package"]);

    fs::write(temp.path().join("demo/evals/a.json"), b"changed").unwrap();
    let changed = service.validate_package("demo");
    assert!(changed.ok, "{:?}", changed.diagnostics);
    assert_ne!(first_hash, changed.fingerprints["package"]);

    let contract_path = temp.path().join("demo/contracts/greeting.yaml");
    fs::write(&contract_path, format!("{CONTRACT}\n# byte-only change\n")).unwrap();
    let changed_contract_bytes = service.validate_package("demo");
    assert!(
        changed_contract_bytes.ok,
        "{:?}",
        changed_contract_bytes.diagnostics
    );
    assert_ne!(
        changed.fingerprints["package"],
        changed_contract_bytes.fingerprints["package"]
    );
}

#[test]
fn package_validation_rejects_missing_duplicate_and_traversal_inventory() {
    let temp = package_with_contract();
    update_manifest(&temp, |manifest| {
        manifest.evals = vec!["evals/missing.json".into()]
    });
    let service = templiqx_local::compose_core(temp.path()).unwrap();
    let missing = service.validate_package("demo");
    assert!(!missing.ok);
    assert!(!missing.fingerprints.contains_key("package"));
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TQX_NOT_FOUND")
    );

    fs::write(temp.path().join("demo/evals/shared.bin"), b"shared").unwrap();
    update_manifest(&temp, |manifest| {
        manifest.evals = vec!["evals/shared.bin".into()];
        manifest.migrations = vec!["evals/shared.bin".into()];
    });
    let duplicate = service.validate_package("demo");
    assert!(!duplicate.ok);
    assert!(
        duplicate
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TQX_INVENTORY_DUPLICATE")
    );

    update_manifest(&temp, |manifest| {
        manifest.evals = vec!["../outside.bin".into()];
        manifest.migrations.clear();
    });
    let traversal = service.validate_package("demo");
    assert!(!traversal.ok);
    assert!(
        traversal
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TQX_PATH_INVALID")
    );
}

#[cfg(unix)]
#[test]
fn package_validation_rejects_symlinked_inventory_artifacts() {
    let temp = package_with_contract();
    fs::write(temp.path().join("outside.bin"), b"outside").unwrap();
    std::os::unix::fs::symlink(
        temp.path().join("outside.bin"),
        temp.path().join("demo/evals/link.bin"),
    )
    .unwrap();
    update_manifest(&temp, |manifest| {
        manifest.evals = vec!["evals/link.bin".into()]
    });
    let result = templiqx_local::compose_core(temp.path())
        .unwrap()
        .validate_package("demo");
    assert!(!result.ok);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TQX_PATH_INVALID")
    );
}

#[cfg(unix)]
#[test]
fn discovery_and_validation_reject_symlinked_package_manifests() {
    let temp = tempfile::tempdir().unwrap();
    templiqx_local::create_package(temp.path(), "demo", "0.1.0").unwrap();
    let manifest = temp.path().join("demo/templiqx.yaml");
    let outside = temp.path().join("outside.yaml");
    fs::rename(&manifest, &outside).unwrap();
    std::os::unix::fs::symlink(&outside, &manifest).unwrap();

    let service = templiqx_local::compose_core(temp.path()).unwrap();
    let discovered = service.discover_packages();
    assert!(!discovered.ok);
    assert_eq!(discovered.diagnostics[0].code, "TQX_PATH_INVALID");

    let validated = service.validate_package("demo");
    assert!(!validated.ok);
    assert_eq!(validated.diagnostics[0].code, "TQX_PATH_INVALID");
}
