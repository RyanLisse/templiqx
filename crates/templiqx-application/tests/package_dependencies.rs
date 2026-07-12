//! U3 (plan 001): package dependency declarations verified against templiqx.lock.
//! Content-addressed, no registry, no network fetch. Lock verifies manifest↔lock
//! agreement and dependency-root presence; unsigned/dep-free packages unchanged.

use std::path::Path;

fn packages_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/packages")
        .canonicalize()
        .expect("examples/packages root")
}

/// Build a temp workspace: a leaf `dep` package plus an `app` package whose
/// manifest declares `dep` and whose lock pins it. Returns the temp dir.
fn workspace_with(app_manifest: &str, lock: Option<&str>) -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    // Leaf dependency package (copies the demo greeting contract).
    std::fs::create_dir_all(temp.path().join("dep/contracts")).expect("dep dirs");
    std::fs::write(
        temp.path().join("dep/templiqx.yaml"),
        "api_version: templiqx/v1alpha1\npackage: dep\nversion: 0.1.0\ndescription: leaf dependency\ncontracts:\n  - greeting\ncomponents: []\nevals: []\nmigrations: []\ntemplates: []\nprovenance:\n  owner: templiqx\n",
    )
    .expect("dep manifest");
    std::fs::copy(
        packages_root().join("demo/contracts/greeting.yaml"),
        temp.path().join("dep/contracts/greeting.yaml"),
    )
    .expect("dep contract");

    // Dependent package.
    std::fs::create_dir_all(temp.path().join("app/contracts")).expect("app dirs");
    std::fs::write(temp.path().join("app/templiqx.yaml"), app_manifest).expect("app manifest");
    std::fs::copy(
        packages_root().join("demo/contracts/greeting.yaml"),
        temp.path().join("app/contracts/greeting.yaml"),
    )
    .expect("app contract");
    if let Some(lock) = lock {
        std::fs::write(temp.path().join("app/templiqx.lock"), lock).expect("lock");
    }
    temp
}

const APP_WITH_DEP: &str = "api_version: templiqx/v1alpha1\npackage: app\nversion: 0.1.0\ndescription: dependent package\ncontracts:\n  - greeting\ncomponents: []\nevals: []\nmigrations: []\ntemplates: []\nprovenance:\n  owner: templiqx\ndependencies:\n  dep: sha256:aaa\n";

fn service(temp: &tempfile::TempDir) -> templiqx_local::LocalService {
    let ws = temp.path().join(".ws");
    std::fs::create_dir_all(&ws).expect("workspace dir");
    templiqx_local::compose_with_workspace(temp.path(), ws).expect("service")
}

fn has_code(
    env: &templiqx_contracts::OperationEnvelope<Vec<templiqx_contracts::ContractSummary>>,
    code: &str,
) -> bool {
    env.diagnostics.iter().any(|d| d.code == code)
}

#[test]
fn matching_lock_and_present_root_validates() {
    let lock = "dependencies:\n  dep:\n    path: ../dep\n    fingerprint: sha256:aaa\n";
    let temp = workspace_with(APP_WITH_DEP, Some(lock));
    let env = service(&temp).validate_package("app");
    assert!(env.ok, "{:?}", env.diagnostics);
}

#[test]
fn lock_fingerprint_drift_fails() {
    let lock = "dependencies:\n  dep:\n    path: ../dep\n    fingerprint: sha256:WRONG\n";
    let temp = workspace_with(APP_WITH_DEP, Some(lock));
    let env = service(&temp).validate_package("app");
    assert!(!env.ok);
    assert!(has_code(&env, "TQX_LOCK_DRIFT"), "{:?}", env.diagnostics);
}

#[test]
fn declared_dependency_without_lock_fails() {
    let temp = workspace_with(APP_WITH_DEP, None);
    let env = service(&temp).validate_package("app");
    assert!(!env.ok);
    assert!(has_code(&env, "TQX_LOCK_MISSING"), "{:?}", env.diagnostics);
}

#[test]
fn missing_dependency_root_fails() {
    let manifest = "api_version: templiqx/v1alpha1\npackage: app\nversion: 0.1.0\ndescription: dependent package\ncontracts:\n  - greeting\ncomponents: []\nevals: []\nmigrations: []\ntemplates: []\nprovenance:\n  owner: templiqx\ndependencies:\n  ghost: sha256:aaa\n";
    let lock = "dependencies:\n  ghost:\n    path: ../ghost\n    fingerprint: sha256:aaa\n";
    let temp = workspace_with(manifest, Some(lock));
    let env = service(&temp).validate_package("app");
    assert!(!env.ok);
    assert!(
        has_code(&env, "TQX_DEPENDENCY_ROOT_MISSING"),
        "{:?}",
        env.diagnostics
    );
}

#[test]
fn lock_without_manifest_declaration_drifts() {
    let manifest = "api_version: templiqx/v1alpha1\npackage: app\nversion: 0.1.0\ndescription: dependent package\ncontracts:\n  - greeting\ncomponents: []\nevals: []\nmigrations: []\ntemplates: []\nprovenance:\n  owner: templiqx\n";
    let lock = "dependencies:\n  dep:\n    path: ../dep\n    fingerprint: sha256:aaa\n";
    let temp = workspace_with(manifest, Some(lock));
    let env = service(&temp).validate_package("app");
    assert!(!env.ok);
    assert!(has_code(&env, "TQX_LOCK_DRIFT"), "{:?}", env.diagnostics);
}

#[test]
fn package_without_dependencies_unchanged() {
    let temp = tempfile::tempdir().expect("tempdir");
    let ws = tempfile::tempdir().expect("workspace");
    let service =
        templiqx_local::compose_with_workspace(packages_root(), ws.path()).expect("service");
    let env = service.validate_package("demo");
    assert!(env.ok, "{:?}", env.diagnostics);
    let _ = temp;
}
