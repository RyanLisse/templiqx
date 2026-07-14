use std::{fs, path::PathBuf};

const FORBIDDEN_DEPENDENCY_MARKERS: &[&str] = &[
    "templiqx-mock",
    "templiqx-runtime-http-mock",
    "templiqx-mock-gateway",
];

#[test]
fn production_http_crate_does_not_depend_on_conformance_mocks() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("Cargo.toml");
    for marker in FORBIDDEN_DEPENDENCY_MARKERS {
        assert!(
            !manifest.contains(marker),
            "templiqx-http Cargo.toml must not reference {marker}"
        );
    }

    let lib_rs = fs::read_to_string(manifest_dir.join("src/lib.rs")).expect("lib.rs");
    for marker in FORBIDDEN_DEPENDENCY_MARKERS {
        assert!(
            !lib_rs.contains(marker),
            "templiqx-http source must not reference {marker}"
        );
    }
}
