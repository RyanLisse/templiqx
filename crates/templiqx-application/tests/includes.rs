//! U4 (plan 001): `include` nodes splice package-relative partials (optionally
//! cross-package) at composition time. Cycles and path traversal fail closed;
//! the portable core never performs file IO.

use templiqx_contracts::Node;

fn service(temp: &tempfile::TempDir) -> templiqx_local::LocalService {
    let ws = temp.path().join(".ws");
    std::fs::create_dir_all(&ws).expect("workspace dir");
    templiqx_local::compose_with_workspace(temp.path(), ws).expect("service")
}

fn write(temp: &tempfile::TempDir, rel: &str, body: &str) {
    let path = temp.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).expect("dirs");
    std::fs::write(path, body).expect("write");
}

fn base_manifest(pkg: &str, deps: &str) -> String {
    format!(
        "api_version: templiqx/v1alpha1\npackage: {pkg}\nversion: 0.1.0\ndescription: include fixture\ncontracts:\n  - main\ncomponents: []\nevals: []\nmigrations: []\ntemplates: []\nprovenance:\n  owner: templiqx\n{deps}"
    )
}

/// A contract whose only message content is one include node.
fn contract_with_include(include: &str) -> String {
    format!(
        "api_version: templiqx/v1alpha1\nid: main\nversion: 0.1.0\nmessages:\n  - role: user\n    content:\n{include}\noutput_schema: {{ type: object }}\n"
    )
}

fn has_code(diags: &[templiqx_contracts::Diagnostic], code: &str) -> bool {
    diags.iter().any(|d| d.code == code)
}

#[test]
fn include_splices_partial_content() {
    let temp = tempfile::tempdir().expect("tempdir");
    write(&temp, "app/templiqx.yaml", &base_manifest("app", ""));
    write(
        &temp,
        "app/partials/greeting.yaml",
        "- kind: text\n  value: Hello from the partial\n",
    );
    write(
        &temp,
        "app/contracts/main.yaml",
        &contract_with_include("      - kind: include\n        path: partials/greeting.yaml\n"),
    );

    let env = service(&temp).inspect_contract("app", "main");
    assert!(env.ok, "{:?}", env.diagnostics);
    let contract = env.result.expect("contract");
    // The include node is gone; the partial's text is spliced in its place.
    match &contract.messages[0].content[0] {
        Node::Text { value } => assert_eq!(value, "Hello from the partial"),
        other => panic!("expected spliced Text node, got {other:?}"),
    }
    assert!(
        !contract.messages[0]
            .content
            .iter()
            .any(|n| matches!(n, Node::Include { .. })),
        "no Include nodes should remain after expansion"
    );
}

#[test]
fn cyclic_include_fails() {
    let temp = tempfile::tempdir().expect("tempdir");
    write(&temp, "app/templiqx.yaml", &base_manifest("app", ""));
    // A partial that includes itself → cycle.
    write(
        &temp,
        "app/partials/loop.yaml",
        "- kind: include\n  path: partials/loop.yaml\n",
    );
    write(
        &temp,
        "app/contracts/main.yaml",
        &contract_with_include("      - kind: include\n        path: partials/loop.yaml\n"),
    );

    let env = service(&temp).inspect_contract("app", "main");
    assert!(!env.ok);
    assert!(
        has_code(&env.diagnostics, "TQX_INCLUDE_CYCLE"),
        "{:?}",
        env.diagnostics
    );
}

#[test]
fn path_traversal_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");
    write(&temp, "app/templiqx.yaml", &base_manifest("app", ""));
    write(
        &temp,
        "app/contracts/main.yaml",
        &contract_with_include("      - kind: include\n        path: ../../etc/passwd\n"),
    );

    let env = service(&temp).inspect_contract("app", "main");
    assert!(!env.ok);
    assert!(
        has_code(&env.diagnostics, "TQX_INCLUDE_UNRESOLVED"),
        "{:?}",
        env.diagnostics
    );
}

#[test]
fn cross_package_include_from_dependency() {
    let temp = tempfile::tempdir().expect("tempdir");
    // Dependency package provides the partial.
    write(&temp, "lib/templiqx.yaml", &base_manifest("lib", ""));
    write(
        &temp,
        "lib/partials/shared.yaml",
        "- kind: text\n  value: shared from lib\n",
    );
    // App includes it via from_dependency.
    write(&temp, "app/templiqx.yaml", &base_manifest("app", ""));
    write(
        &temp,
        "app/contracts/main.yaml",
        &contract_with_include(
            "      - kind: include\n        path: partials/shared.yaml\n        from_dependency: lib\n",
        ),
    );

    let env = service(&temp).inspect_contract("app", "main");
    assert!(env.ok, "{:?}", env.diagnostics);
    let contract = env.result.expect("contract");
    match &contract.messages[0].content[0] {
        Node::Text { value } => assert_eq!(value, "shared from lib"),
        other => panic!("expected cross-package spliced Text, got {other:?}"),
    }
}
