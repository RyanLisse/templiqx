//! U9/R13: bounded Markdown interpolation and safe deterministic output.

use anyhow::{Result, ensure};
use templiqx_markdown::{ENVIRONMENT_ID, MarkdownAdapter, RENDERER_ID, RENDERER_VERSION};
use templiqx_ports::{DocumentRenderRequest, DocumentRenderer};

const MEMO_TEMPLATE: &str = concat!(
    "# Matter memo\n\n",
    "Client: **{{ client }}**\n\n",
    "{{ summary }}\n\n",
    "Owner: {{ owner }}\n",
);

const HTML_GOLDEN: &str = concat!(
    "<h1>Matter memo</h1>\n",
    "<p>Client: <strong>Acme &amp; Partners</strong></p>\n",
    "<p>Status is approved.</p>\n",
    "<p>Owner: Ryan</p>\n",
);

const PLAIN_GOLDEN: &str = concat!(
    "Matter memo\n",
    "\n",
    "Client: Acme &amp; Partners\n",
    "\n",
    "Status is approved.\n",
    "\n",
    "Owner: Ryan\n",
);

fn memo_data() -> serde_json::Value {
    serde_json::json!({
        "client": "Acme & Partners",
        "summary": "Status is approved.",
        "owner": "Ryan"
    })
}

#[test]
fn memo_markdown_matches_safe_html_and_plain_goldens() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let template = workspace.path().join("memo.md");
    std::fs::write(&template, MEMO_TEMPLATE)?;

    let first_output = workspace.path().join("first.html");
    let second_output = workspace.path().join("second.html");
    let first = MarkdownAdapter::html().render_document(&DocumentRenderRequest {
        template: template.clone(),
        data: memo_data(),
        output: first_output.clone(),
    })?;
    let second = MarkdownAdapter::html().render_document(&DocumentRenderRequest {
        template: template.clone(),
        data: memo_data(),
        output: second_output.clone(),
    })?;
    let first_bytes = std::fs::read(&first_output)?;
    let second_bytes = std::fs::read(&second_output)?;
    ensure!(first_bytes == HTML_GOLDEN.as_bytes());
    ensure!(first_bytes == second_bytes, "safe HTML render drifted");
    ensure!(first.report["renderer_id"] == RENDERER_ID);
    ensure!(first.report["renderer_version"] == RENDERER_VERSION);
    ensure!(first.report["environment_id"] == ENVIRONMENT_ID);
    ensure!(first.report["artifact_fingerprint"] == second.report["artifact_fingerprint"]);
    ensure!(first.report["artifact_fingerprint"] == first.report["output_hash"]);
    ensure!(first.report["status"] == "ok");

    let plain_output = workspace.path().join("memo.txt");
    MarkdownAdapter::plain_text().render_document(&DocumentRenderRequest {
        template,
        data: memo_data(),
        output: plain_output.clone(),
    })?;
    ensure!(std::fs::read(plain_output)? == PLAIN_GOLDEN.as_bytes());
    Ok(())
}

#[test]
fn raw_html_in_merge_data_is_neutralized_and_stable() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let template = workspace.path().join("unsafe-memo.md");
    std::fs::write(&template, "# Memo\n\n{{ body }}\n")?;
    let data = serde_json::json!({
        "body": "<script>alert(\"owned\")</script><b>raw HTML</b>"
    });
    let first_output = workspace.path().join("unsafe-first.html");
    let second_output = workspace.path().join("unsafe-second.html");
    MarkdownAdapter::html().render_document(&DocumentRenderRequest {
        template: template.clone(),
        data: data.clone(),
        output: first_output.clone(),
    })?;
    MarkdownAdapter::html().render_document(&DocumentRenderRequest {
        template,
        data,
        output: second_output.clone(),
    })?;
    let first = std::fs::read_to_string(first_output)?;
    let second = std::fs::read_to_string(second_output)?;

    ensure!(first == second, "unsafe-value render drifted");
    ensure!(!first.contains("<script>"));
    ensure!(!first.contains("<b>raw HTML</b>"));
    ensure!(!first.contains("<script>alert(\"owned\")</script>"));
    ensure!(first.contains("&lt;script&gt;"));
    ensure!(first.contains("&lt;b&gt;raw HTML&lt;/b&gt;"));
    Ok(())
}
