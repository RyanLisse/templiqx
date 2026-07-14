use fs2::FileExt;
use std::fs;
use std::path::Path;
use templiqx_application::{
    DeletePackageRequest, DeleteWorkspaceArtifactRequest, RenderDocumentRequest,
    UpdatePackageRequest,
};
use templiqx_contracts::RenderRequest;
use templiqx_ports::ArtifactWorkspace;

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

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let from_path = entry.path();
        let to_path = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&from_path, &to_path);
        } else {
            fs::copy(&from_path, &to_path).unwrap();
        }
    }
}

fn copy_crm3_package(root: &Path) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    copy_dir(&repo.join("examples/crm3"), &root.join("crm3"));
}

fn set_readonly_recursive(path: &Path, readonly: bool) {
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let child = entry.path();
        if entry.file_type().unwrap().is_dir() {
            set_readonly_recursive(&child, readonly);
        }
        let mut permissions = fs::metadata(&child).unwrap().permissions();
        permissions.set_readonly(readonly);
        fs::set_permissions(&child, permissions).unwrap();
    }
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(readonly);
    fs::set_permissions(path, permissions).unwrap();
}

fn merge_data(root: &Path) -> serde_json::Value {
    let data = fs::read(root.join("crm3/evals/bli-62-output.json")).unwrap();
    serde_json::from_slice::<serde_json::Value>(&data).unwrap()["merge_data"].clone()
}

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

#[test]
fn package_lifecycle_is_cas_safe_and_invalidates_signatures() {
    let temp = tempfile::tempdir().unwrap();
    templiqx_local::create_package(temp.path(), "demo", "0.1.0").unwrap();
    update_manifest(&temp, |manifest| {
        manifest
            .signatures
            .push(templiqx_contracts::PackageSignature {
                key_id: "test".into(),
                algorithm: "sha256-keyed".into(),
                value: "signed".into(),
            });
    });
    let service = templiqx_local::compose(temp.path()).unwrap();
    let manifest: templiqx_contracts::PackageManifest = serde_yaml_ng::from_str(
        &fs::read_to_string(temp.path().join("demo/templiqx.yaml")).unwrap(),
    )
    .unwrap();
    let expected = templiqx_contracts::fingerprint(&manifest).unwrap();
    let stale = service.update_package(&UpdatePackageRequest {
        package: "demo".into(),
        version: Some("0.2.0".into()),
        description: None,
        expected_fingerprint: "stale".into(),
    });
    assert!(!stale.ok);
    assert_eq!(stale.diagnostics[0].code, "TQX_CAS_CONFLICT");

    let updated = service.update_package(&UpdatePackageRequest {
        package: "demo".into(),
        version: Some("0.2.0".into()),
        description: Some("production candidate".into()),
        expected_fingerprint: expected,
    });
    assert!(updated.ok, "{:?}", updated.diagnostics);
    let manifest = updated.result.unwrap();
    assert_eq!(manifest.version, "0.2.0");
    assert!(manifest.signatures.is_empty());
    let expected = templiqx_contracts::fingerprint(&manifest).unwrap();
    let deleted = service.delete_package(&DeletePackageRequest {
        package: "demo".into(),
        expected_fingerprint: expected,
    });
    assert!(deleted.ok, "{:?}", deleted.diagnostics);
    assert!(!temp.path().join("demo").exists());
}

#[test]
fn package_delete_blocks_untracked_content_and_dependents() {
    let temp = tempfile::tempdir().unwrap();
    templiqx_local::create_package(temp.path(), "demo", "0.1.0").unwrap();
    fs::write(temp.path().join("demo/untracked.txt"), "keep").unwrap();
    let service = templiqx_local::compose(temp.path()).unwrap();
    let manifest = service.discover_packages().result.unwrap().remove(0);
    let expected = templiqx_contracts::fingerprint(&manifest).unwrap();
    let blocked = service.delete_package(&DeletePackageRequest {
        package: "demo".into(),
        expected_fingerprint: expected,
    });
    assert!(!blocked.ok);
    assert!(blocked.diagnostics[0].message.contains("untracked"));

    let dependent_root = tempfile::tempdir().unwrap();
    templiqx_local::create_package(dependent_root.path(), "dep", "0.1.0").unwrap();
    templiqx_local::create_package(dependent_root.path(), "app", "0.1.0").unwrap();
    let app_manifest_path = dependent_root.path().join("app/templiqx.yaml");
    let mut app_manifest: templiqx_contracts::PackageManifest =
        serde_yaml_ng::from_str(&fs::read_to_string(&app_manifest_path).unwrap()).unwrap();
    app_manifest
        .dependencies
        .insert("dep".into(), "pinned".into());
    fs::write(
        app_manifest_path,
        serde_yaml_ng::to_string(&app_manifest).unwrap(),
    )
    .unwrap();
    let service = templiqx_local::compose(dependent_root.path()).unwrap();
    let dep = service
        .discover_packages()
        .result
        .unwrap()
        .into_iter()
        .find(|manifest| manifest.package == "dep")
        .unwrap();
    let blocked = service.delete_package(&DeletePackageRequest {
        package: "dep".into(),
        expected_fingerprint: templiqx_contracts::fingerprint(&dep).unwrap(),
    });
    assert!(!blocked.ok);
    assert!(blocked.diagnostics[0].message.contains("depends on"));
}

#[test]
fn workspace_artifact_delete_reuses_confinement_and_byte_cas() {
    let packages = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    templiqx_local::create_package(packages.path(), "demo", "0.1.0").unwrap();
    fs::create_dir_all(workspace.path().join("demo/out")).unwrap();
    fs::write(workspace.path().join("demo/out/result.txt"), "result").unwrap();
    let service =
        templiqx_local::compose_with_workspace(packages.path(), workspace.path()).unwrap();
    let expected = templiqx_contracts::fingerprint_bytes(b"result");
    let stale = service.delete_workspace_artifact(&DeleteWorkspaceArtifactRequest {
        package: "demo".into(),
        path: "out/result.txt".into(),
        workspace: None,
        expected_fingerprint: "stale".into(),
    });
    assert!(!stale.ok);
    let deleted = service.delete_workspace_artifact(&DeleteWorkspaceArtifactRequest {
        package: "demo".into(),
        path: "out/result.txt".into(),
        workspace: None,
        expected_fingerprint: expected,
    });
    assert!(deleted.ok, "{:?}", deleted.diagnostics);
    assert!(!workspace.path().join("demo/out/result.txt").exists());
    let traversal = service.delete_workspace_artifact(&DeleteWorkspaceArtifactRequest {
        package: "demo".into(),
        path: "../outside".into(),
        workspace: None,
        expected_fingerprint: "x".into(),
    });
    assert!(!traversal.ok);
    assert_eq!(traversal.diagnostics[0].code, "TQX_PATH_INVALID");
}

#[test]
fn workspace_delete_waits_for_service_writer_lease() {
    let workspace = tempfile::tempdir().unwrap();
    let adapter = templiqx_local::FilesystemArtifactWorkspace::new(workspace.path()).unwrap();
    let lease = adapter
        .lease_output_path("demo", "out/result.txt", None)
        .unwrap();
    fs::write(lease.path(), "result").unwrap();
    let expected = templiqx_contracts::fingerprint_bytes(b"result");
    let deleting = adapter.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        tx.send(deleting.delete_artifact("demo", "out/result.txt", None, &expected))
            .unwrap();
    });

    assert!(matches!(
        rx.recv_timeout(std::time::Duration::from_millis(100)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    drop(lease);
    assert!(
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
            .is_ok()
    );
    handle.join().unwrap();
    assert!(!workspace.path().join("demo/out/result.txt").exists());
}

#[test]
fn workspace_read_waits_for_service_writer_lease() {
    let workspace = tempfile::tempdir().unwrap();
    let adapter = templiqx_local::FilesystemArtifactWorkspace::new(workspace.path()).unwrap();
    let lease = adapter
        .lease_output_path("demo", "out/result.txt", None)
        .unwrap();
    fs::write(lease.path(), "partial").unwrap();
    let reading = adapter.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        tx.send(reading.read_artifact("demo", "out/result.txt", None))
            .unwrap();
    });

    assert!(matches!(
        rx.recv_timeout(std::time::Duration::from_millis(100)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    fs::write(lease.path(), "complete").unwrap();
    drop(lease);
    assert_eq!(
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
            .unwrap(),
        b"complete"
    );
    handle.join().unwrap();
}

#[test]
fn workspace_list_waits_for_service_writer_lease() {
    let workspace = tempfile::tempdir().unwrap();
    let adapter = templiqx_local::FilesystemArtifactWorkspace::new(workspace.path()).unwrap();
    let lease = adapter
        .lease_output_path("demo", "out/result.txt", None)
        .unwrap();
    fs::write(lease.path(), "partial").unwrap();
    let listing = adapter.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        tx.send(listing.list_artifacts("demo", None, Some("out")))
            .unwrap();
    });

    assert!(matches!(
        rx.recv_timeout(std::time::Duration::from_millis(100)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    fs::write(lease.path(), "complete").unwrap();
    drop(lease);
    let artifacts = rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].1, b"complete".len() as u64);
    handle.join().unwrap();
}

#[test]
fn persistent_package_lock_serializes_mutation_delete_and_recreate() {
    let temp = tempfile::tempdir().unwrap();
    templiqx_local::create_package(temp.path(), "demo", "0.1.0").unwrap();
    let lock_path = temp.path().join(".templiqx-package-locks/demo.lock");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    lock.lock_exclusive().unwrap();

    let root = temp.path().to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let service = templiqx_local::compose(&root).unwrap();
        let manifest = service.discover_packages().result.unwrap().remove(0);
        tx.send(service.update_package(&UpdatePackageRequest {
            package: "demo".into(),
            version: Some("0.2.0".into()),
            description: None,
            expected_fingerprint: templiqx_contracts::fingerprint(&manifest).unwrap(),
        }))
        .unwrap();
    });
    assert!(matches!(
        rx.recv_timeout(std::time::Duration::from_millis(100)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    FileExt::unlock(&lock).unwrap();
    assert!(
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
            .ok
    );
    handle.join().unwrap();

    let service = templiqx_local::compose(temp.path()).unwrap();
    let manifest = service.discover_packages().result.unwrap().remove(0);
    assert!(
        service
            .delete_package(&DeletePackageRequest {
                package: "demo".into(),
                expected_fingerprint: templiqx_contracts::fingerprint(&manifest).unwrap(),
            })
            .ok
    );
    assert!(lock_path.exists(), "delete must retain the lock inode");

    lock.lock_exclusive().unwrap();
    let root = temp.path().to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        tx.send(templiqx_local::create_package(&root, "demo", "0.3.0"))
            .unwrap();
    });
    assert!(matches!(
        rx.recv_timeout(std::time::Duration::from_millis(100)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    FileExt::unlock(&lock).unwrap();
    assert!(
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .unwrap()
            .is_ok()
    );
    handle.join().unwrap();
}

#[test]
fn package_root_can_be_read_only_when_workspace_is_writable() {
    let packages = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    copy_crm3_package(packages.path());
    set_readonly_recursive(&packages.path().join("crm3"), true);

    let service =
        templiqx_local::compose_with_workspace(packages.path(), workspace.path()).unwrap();
    let rendered = service.render_document(&RenderDocumentRequest {
        package: "crm3".into(),
        template: "templates/v5-contract-template.docx".into(),
        data: merge_data(packages.path()),
        output: "artifacts/rendered.docx".into(),
        workspace: None,
    });

    set_readonly_recursive(&packages.path().join("crm3"), false);
    assert!(rendered.ok, "{:?}", rendered.diagnostics);
    assert!(
        workspace
            .path()
            .join("crm3/artifacts/rendered.docx")
            .exists()
    );
    assert!(
        !packages
            .path()
            .join("crm3/artifacts/rendered.docx")
            .exists()
    );
}

#[test]
fn old_local_defaults_still_render_with_safe_workspace() {
    let packages = tempfile::tempdir().unwrap();
    copy_crm3_package(packages.path());
    let service = templiqx_local::compose(packages.path()).unwrap();

    let rendered = service.render_document(&RenderDocumentRequest {
        package: "crm3".into(),
        template: "templates/v5-contract-template.docx".into(),
        data: merge_data(packages.path()),
        output: "default-rendered.docx".into(),
        workspace: None,
    });

    assert!(rendered.ok, "{:?}", rendered.diagnostics);
    assert!(
        packages
            .path()
            .join(".templiqx-workspace/crm3/default-rendered.docx")
            .exists()
    );
    assert!(!packages.path().join("crm3/default-rendered.docx").exists());
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
