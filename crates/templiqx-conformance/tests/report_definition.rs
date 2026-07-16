use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::PathBuf};

const EXPECTED_FINGERPRINT: &str =
    "2d327e7076b30e973a32eb0d3eea82f059aaeed08aca18a37f5190b5e85eb28d";

fn fixture_path() -> PathBuf {
    package_root().join("definitions/dunning-letter-v1.yaml")
}

fn package_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/packages/basenet-legal")
}

fn load_definition() -> Result<Value> {
    let yaml = fs::read_to_string(fixture_path()).context("read report definition fixture")?;
    serde_yaml_ng::from_str(&yaml).context("parse report definition fixture")
}

fn assert_v1alpha1_shape(definition: &Value) -> Result<()> {
    let object = definition
        .as_object()
        .context("report definition must be a mapping")?;
    let expected_keys = BTreeSet::from([
        "approval",
        "field_map",
        "id",
        "query_binding",
        "target_format",
        "template_ref",
        "version",
    ]);
    let actual_keys = object.keys().map(String::as_str).collect::<BTreeSet<_>>();
    ensure!(actual_keys == expected_keys, "unexpected v1alpha1 fields");

    for field in [
        "id",
        "version",
        "query_binding",
        "template_ref",
        "target_format",
    ] {
        ensure!(
            object
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            "{field} must be a non-empty string"
        );
    }

    let template_ref = object["template_ref"].as_str().unwrap();
    ensure!(
        !template_ref.starts_with('/') && !template_ref.split('/').any(|part| part == ".."),
        "template_ref must be package-relative and confined"
    );
    ensure!(
        package_root().join(template_ref).is_file(),
        "template_ref must resolve to a package file"
    );

    let field_map = object["field_map"]
        .as_object()
        .context("field_map must be a mapping")?;
    ensure!(!field_map.is_empty(), "field_map must not be empty");
    ensure!(
        field_map
            .iter()
            .all(|(key, value)| !key.is_empty()
                && value.as_str().is_some_and(|path| !path.is_empty())),
        "field_map entries must map non-empty names to non-empty source paths"
    );

    let review = object["approval"]
        .as_object()
        .context("approval must be a mapping")?;
    let expected_review_keys = BTreeSet::from(["approved_at", "approved_by", "status"]);
    let actual_review_keys = review.keys().map(String::as_str).collect::<BTreeSet<_>>();
    ensure!(
        actual_review_keys == expected_review_keys,
        "unexpected approval metadata fields"
    );
    ensure!(
        review
            .values()
            .all(|value| value.as_str().is_some_and(|text| !text.is_empty())),
        "approval metadata values must be non-empty strings"
    );

    Ok(())
}

#[test]
fn synthetic_report_definition_has_stable_fingerprint() -> Result<()> {
    let definition = load_definition()?;
    assert_v1alpha1_shape(&definition)?;

    let manifest: Value = serde_yaml_ng::from_str(
        &fs::read_to_string(package_root().join("templiqx.yaml"))
            .context("read basenet-legal manifest")?,
    )?;
    ensure!(
        manifest["definitions"].as_array().is_some_and(|paths| paths
            .iter()
            .any(|path| { path.as_str() == Some("definitions/dunning-letter-v1.yaml") })),
        "package manifest must list the report definition"
    );

    let fingerprint = templiqx_contracts::fingerprint(&definition)?;
    ensure!(
        fingerprint == EXPECTED_FINGERPRINT,
        "report definition fingerprint drift: expected={EXPECTED_FINGERPRINT}, actual={fingerprint}"
    );

    let reordered: Value = serde_yaml_ng::from_str(
        r#"
approval:
  approved_at: "2026-07-16T00:00:00Z"
  approved_by: synthetic-reviewer
  status: approved
target_format: docx
template_ref: templates/v5-legal-template.docx
field_map:
  handler_role: signature_slot.role
  claims: claims
  outstanding_total: financials.formatted_total
  recipient_name: client.name
query_binding: host-query://synthetic-basenet/dunning-letter/v1
version: 1.0.0
id: dunning-letter
"#,
    )?;
    ensure!(
        templiqx_contracts::fingerprint(&reordered)? == fingerprint,
        "canonical JSON fingerprint must ignore mapping-key order"
    );

    Ok(())
}
