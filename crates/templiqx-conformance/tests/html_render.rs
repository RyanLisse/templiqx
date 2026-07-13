//! U7 (plan 001): the optional HTML/plain-text adapter renders CRM3-style draft
//! JSON to deterministic golden HTML. The adapter is host-constructed and never
//! part of the default CLI/MCP composition (proven by `boundaries` + the
//! core-only composition tests elsewhere).

use std::path::{Path, PathBuf};

use serde_json::json;
use templiqx_html_plain::HtmlPlainAdapter;
use templiqx_ports::{DocumentRenderRequest, DocumentRenderer};

fn template_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/crm3/templates/draft-email.html")
        .canonicalize()
        .expect("crm3 html template")
}

#[test]
fn crm3_draft_json_renders_to_golden_html() {
    let out = tempfile::tempdir().expect("tempdir");
    let output = out.path().join("draft.html");
    // Draft-shaped data aligned with the BLI-62 drafting step.
    let data = json!({
        "client_name": "Jansen & Co",
        "summary": "Uw dossier is <beoordeeld> conform de aangeleverde stukken.",
        "citations": ["fragment 1", "fragment 2"],
        "author": "Templiqx"
    });
    let result = HtmlPlainAdapter
        .render_document(&DocumentRenderRequest {
            template: template_path(),
            data,
            output: output.clone(),
        })
        .expect("render ok");

    let rendered = std::fs::read_to_string(&result.artifact).expect("read output");
    let golden = "<!doctype html>\n\
<p>Beste Jansen &amp; Co,</p>\n\
<p>Uw dossier is &lt;beoordeeld&gt; conform de aangeleverde stukken.</p>\n\
<ul><li>fragment 1</li><li>fragment 2</li></ul>\n\
<p>Met vriendelijke groet,<br>Templiqx</p>\n";
    assert_eq!(rendered, golden);

    // No unresolved fields for complete draft data.
    assert_eq!(
        result.report["unresolved_fields"],
        json!([]),
        "report: {}",
        result.report
    );
}

#[test]
fn missing_draft_fields_are_reported() {
    let out = tempfile::tempdir().expect("tempdir");
    let output = out.path().join("partial.html");
    let result = HtmlPlainAdapter
        .render_document(&DocumentRenderRequest {
            template: template_path(),
            data: json!({ "client_name": "X" }),
            output,
        })
        .expect("render ok");
    let unresolved = result.report["unresolved_fields"]
        .as_array()
        .expect("array");
    assert!(unresolved.iter().any(|v| v == "summary"));
    assert!(unresolved.iter().any(|v| v == "author"));
}
