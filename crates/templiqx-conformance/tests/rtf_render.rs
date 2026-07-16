//! U10: deterministic RTF render — control-char escaping and byte stability.

use anyhow::{Result, ensure};
use templiqx_ports::{DocumentRenderRequest, DocumentRenderer};
use templiqx_rtf::{ENVIRONMENT_ID, RENDERER_ID, RENDERER_VERSION, RtfAdapter};

#[test]
fn rtf_render_escapes_controls_and_is_byte_stable() -> Result<()> {
    let workspace = tempfile::tempdir()?;
    let template = workspace.path().join("memo.rtf.txt");
    std::fs::write(&template, "Client: {{ client }}\\par Note: {{ note }}\\par")?;
    let data = serde_json::json!({
        "client": "Acme {Legal} \\ Partners",
        "note": "Café 😀"
    });

    let first_output = workspace.path().join("first.rtf");
    let second_output = workspace.path().join("second.rtf");
    let first = RtfAdapter.render_document(&DocumentRenderRequest {
        template: template.clone(),
        data: data.clone(),
        output: first_output.clone(),
    })?;
    let second = RtfAdapter.render_document(&DocumentRenderRequest {
        template,
        data,
        output: second_output.clone(),
    })?;

    let first_bytes = std::fs::read(&first_output)?;
    let second_bytes = std::fs::read(&second_output)?;
    let text = String::from_utf8(first_bytes.clone())?;

    ensure!(text.starts_with("{\\rtf1"), "RTF must start with {{\\rtf1");
    ensure!(first_bytes == second_bytes, "RTF render drifted");
    ensure!(
        text.contains("Acme \\{Legal\\} \\\\ Partners"),
        "RTF control chars must be escaped: {text}"
    );
    ensure!(
        text.contains("Caf\\u233? \\u-10179?\\u-8704?"),
        "RTF non-ASCII text must use signed UTF-16 escapes: {text}"
    );
    ensure!(!text.contains("Acme {Legal}"), "raw braces must not appear");
    ensure!(first.report["renderer_id"] == RENDERER_ID);
    ensure!(first.report["renderer_version"] == RENDERER_VERSION);
    ensure!(first.report["environment_id"] == ENVIRONMENT_ID);
    ensure!(first.report["artifact_fingerprint"] == second.report["artifact_fingerprint"]);
    ensure!(first.report["artifact_fingerprint"] == first.report["output_hash"]);
    ensure!(first.report["status"] == "ok");
    Ok(())
}
