use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let from_path = entry.path();
        let to_path = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&from_path, &to_path)?;
        } else {
            fs::copy(&from_path, &to_path)?;
        }
    }
    Ok(())
}

fn copy_crm3_package(root: &Path) -> Result<()> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    copy_dir(&repo.join("examples/crm3"), &root.join("crm3"))
}

fn write_merge_data(root: &Path) -> Result<std::path::PathBuf> {
    let data = fs::read(root.join("crm3/evals/bli-62-output.json"))?;
    let merge_data = serde_json::from_slice::<Value>(&data)?["merge_data"].clone();
    let data_path = root.join("merge-data.json");
    fs::write(&data_path, serde_json::to_vec_pretty(&merge_data)?)?;
    Ok(data_path)
}

fn run_templiqx(args: &[&str]) -> Result<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_templiqx"))
        .args(args)
        .output()?;
    parse_output(output)
}

fn run_templiqx_in(args: &[&str], current_dir: &Path) -> Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_templiqx"))
        .args(args)
        .current_dir(current_dir)
        .output()
        .context("run templiqx")
}

fn parse_output(output: Output) -> Result<Value> {
    ensure!(
        output.status.success(),
        "templiqx failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).context("parse CLI JSON")
}

fn set_readonly(path: &Path, readonly: bool) -> Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(readonly);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[test]
fn catalog_does_not_compose_default_root() -> Result<()> {
    let readonly = tempfile::tempdir()?;
    set_readonly(readonly.path(), true)?;

    let catalog = run_templiqx_in(&["--json", "catalog"], readonly.path())?;
    ensure!(
        catalog.status.success(),
        "catalog failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&catalog.stdout),
        String::from_utf8_lossy(&catalog.stderr)
    );
    let catalog = parse_output(catalog)?;
    ensure!(
        catalog["ok"] == true,
        "unexpected catalog envelope: {catalog}"
    );

    set_readonly(readonly.path(), false)?;
    Ok(())
}

#[test]
fn old_render_command_defaults_remain_compatible() -> Result<()> {
    let packages = tempfile::tempdir()?;
    copy_crm3_package(packages.path())?;
    let data_path = write_merge_data(packages.path())?;

    let envelope = run_templiqx(&[
        "--root",
        packages.path().to_str().context("packages root")?,
        "--json",
        "render-document",
        "crm3",
        "templates/v5-contract-template.docx",
        data_path.to_str().context("merge data path")?,
        "default-rendered.docx",
    ])?;

    ensure!(envelope["ok"] == true, "unexpected envelope: {envelope}");
    ensure!(envelope["result"]["artifact"] == "default-rendered.docx");
    ensure!(
        packages
            .path()
            .join(".templiqx-workspace/crm3/default-rendered.docx")
            .exists()
    );
    ensure!(!packages.path().join("crm3/default-rendered.docx").exists());
    Ok(())
}

#[test]
fn explicit_workspace_matches_rust_artifact_contract() -> Result<()> {
    let packages = tempfile::tempdir()?;
    let workspace = tempfile::tempdir()?;
    copy_crm3_package(packages.path())?;
    let data_path = write_merge_data(packages.path())?;

    let envelope = run_templiqx(&[
        "--root",
        packages.path().to_str().context("packages root")?,
        "--json",
        "render-document",
        "crm3",
        "templates/v5-contract-template.docx",
        data_path.to_str().context("merge data path")?,
        "explicit/rendered.docx",
        "--workspace",
        workspace.path().to_str().context("workspace")?,
    ])?;

    ensure!(envelope["ok"] == true, "unexpected envelope: {envelope}");
    ensure!(envelope["result"]["artifact"] == "explicit/rendered.docx");
    ensure!(
        workspace
            .path()
            .join("crm3/explicit/rendered.docx")
            .exists()
    );
    ensure!(!packages.path().join("crm3/explicit/rendered.docx").exists());
    Ok(())
}

#[test]
fn package_lifecycle_commands_preserve_structured_cas_envelopes() -> Result<()> {
    let packages = tempfile::tempdir()?;
    let root = packages.path().to_str().context("packages root")?;
    let created = run_templiqx(&["--root", root, "--json", "create", "demo"])?;
    let expected = created["fingerprints"]["package"]
        .as_str()
        .context("created package fingerprint")?;
    let updated = run_templiqx(&[
        "--root",
        root,
        "--json",
        "update-package",
        "demo",
        "--version",
        "0.2.0",
        "--expected-fingerprint",
        expected,
    ])?;
    ensure!(updated["ok"] == true, "unexpected envelope: {updated}");
    let expected = updated["fingerprints"]["package"]
        .as_str()
        .context("updated package fingerprint")?;
    let deleted = run_templiqx(&[
        "--root",
        root,
        "--json",
        "delete-package",
        "demo",
        "--expected-fingerprint",
        expected,
    ])?;
    ensure!(deleted["ok"] == true, "unexpected envelope: {deleted}");
    ensure!(!packages.path().join("demo").exists());
    Ok(())
}

fn quality_request() -> Value {
    serde_json::json!({
        "package": "demo",
        "contract_id": "contract",
        "expected_package_fingerprint": "package-fingerprint",
        "expected_base_contract_fingerprint": "contract-fingerprint",
        "expected_fixture_set_fingerprint": "fixture-fingerprint",
        "policy": {
            "id": "policy",
            "replicates_per_fixture": 1,
            "minimum_semantic_cases": 1,
            "maximum_infrastructure_failure_ppm": 0,
            "claimed_evaluator_profile_fingerprint": "evaluator-fingerprint",
            "claimed_model_profile_fingerprint": "model-fingerprint",
            "binary_scorers": [],
            "objectives": [],
            "eligibility_rules": []
        },
        "candidates": []
    })
}

fn run_quality_request(root: &Path, request: &Path) -> Result<Output> {
    run_templiqx_in(
        &[
            "--root",
            root.to_str().context("packages root")?,
            "assess-quality-proposals",
            "--request",
            request.to_str().context("request path")?,
        ],
        root,
    )
}

#[test]
fn quality_assessment_command_rejects_private_invalid_json_without_reflection() -> Result<()> {
    let packages = tempfile::tempdir()?;
    let request = packages.path().join("quality-request.json");
    let email = "ryan.sensitive@example.invalid";
    let ssn = "123-45-6789";

    let mut nested_unknown = quality_request();
    nested_unknown["policy"][format!("customer_{email}_ssn_{ssn}")] = Value::Bool(true);
    let cases = [
        serde_json::to_vec(&serde_json::json!({
            format!("customer_{email}_ssn_{ssn}"): true
        }))?,
        serde_json::to_vec(&nested_unknown)?,
        format!(r#"{{"package":"demo","candidate_{email}_ssn_{ssn}":"unterminated"#).into_bytes(),
    ];

    for contents in cases {
        fs::write(&request, contents)?;
        let output = run_quality_request(packages.path(), &request)?;
        ensure!(!output.status.success(), "invalid JSON must fail closed");
        ensure!(
            output.stdout.is_empty(),
            "invalid quality request emitted stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        ensure!(
            output.stderr == b"templiqx: quality assessment request body is invalid\n",
            "quality rejection reflected private decode details: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        ensure!(
            !stderr.contains(email)
                && !stderr.contains(ssn)
                && !stderr.contains("unknown field")
                && !stderr.contains("EOF"),
            "quality rejection reflected a canary: {stderr}"
        );
    }
    Ok(())
}

#[test]
fn quality_assessment_command_redacts_request_file_read_failures() -> Result<()> {
    let packages = tempfile::tempdir()?;
    let missing = packages
        .path()
        .join("ryan.sensitive@example.invalid-123-45-6789.json");

    let output = run_quality_request(packages.path(), &missing)?;
    ensure!(!output.status.success(), "missing request must fail closed");
    ensure!(output.stdout.is_empty(), "missing request emitted stdout");
    ensure!(
        output.stderr == b"templiqx: quality assessment request file could not be read\n",
        "quality file rejection reflected private path details: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
