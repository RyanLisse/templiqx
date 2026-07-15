//! U2 (cross-opco plan 001): bounded DOCX repeat and conditional render proof.

use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde_json::json;
use templiqx_docx_v5::DocxV5Adapter;
use templiqx_ports::{DocumentRenderRequest, DocumentRenderer};
use zip::ZipArchive;

fn corpus_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/legacy-corpus/fixtures")
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("parse json")
}

fn story_xml(path: &Path) -> String {
    let file = File::open(path).expect("open docx");
    let mut archive = ZipArchive::new(file).expect("read docx zip");
    let mut entry = archive.by_name("word/document.xml").expect("document part");
    let mut xml = String::new();
    entry.read_to_string(&mut xml).expect("read document xml");
    xml
}

#[test]
fn repeat_renders_three_rows_with_normalized_ooxml_parity() {
    let fixture = corpus_root().join("v5-legal-repeat-rendered");
    let temporary = tempfile::tempdir().expect("tempdir");
    let output = temporary.path().join("rendered.docx");
    let adapter = DocxV5Adapter::default();
    let rendered = adapter
        .render_document(&DocumentRenderRequest {
            template: fixture.join("source.docx"),
            data: read_json(&fixture.join("render-data.json")),
            output: output.clone(),
        })
        .expect("render repeat fixture");

    let report: templiqx_docx_v5::RenderReport =
        serde_json::from_value(rendered.report).expect("render report");
    assert_eq!(report.replacements, 6, "two fields across three rows");

    let document = story_xml(&output);
    assert_eq!(document.matches("<w:tr>").count(), 3, "expected three rows");
    assert!(document.contains("Retainer review"));
    assert!(document.contains("Court filing"));
    assert!(document.contains("Client meeting"));

    let parity = adapter
        .compare_normalized(&output, &fixture.join("expected-render.docx"))
        .expect("parity");
    assert!(parity.equal, "repeat parity mismatch: {parity:#?}");
}

#[test]
fn conditional_includes_paragraph_when_truthy() {
    let fixture = corpus_root().join("v5-legal-conditional-rendered");
    let temporary = tempfile::tempdir().expect("tempdir");
    let output = temporary.path().join("included.docx");
    let adapter = DocxV5Adapter::default();
    adapter
        .render_document(&DocumentRenderRequest {
            template: fixture.join("source.docx"),
            data: json!({"include_notice": true, "notice_text": "Binding notice text"}),
            output: output.clone(),
        })
        .expect("render conditional fixture");

    let document = story_xml(&output);
    assert!(document.contains("Binding notice text"));
    assert!(document.contains("Always visible"));
    assert!(!document.contains("${?include_notice}"));
    assert!(!document.contains("${/include_notice}"));

    let parity = adapter
        .compare_normalized(&output, &fixture.join("expected-render.docx"))
        .expect("parity");
    assert!(
        parity.equal,
        "conditional include parity mismatch: {parity:#?}"
    );
}

#[test]
fn conditional_excludes_paragraph_when_falsy() {
    let fixture = corpus_root().join("v5-legal-conditional-rendered");
    let temporary = tempfile::tempdir().expect("tempdir");
    let output = temporary.path().join("excluded.docx");
    let adapter = DocxV5Adapter::default();
    adapter
        .render_document(&DocumentRenderRequest {
            template: fixture.join("source.docx"),
            data: json!({"include_notice": false, "notice_text": "Binding notice text"}),
            output: output.clone(),
        })
        .expect("render conditional fixture");

    let document = story_xml(&output);
    assert!(!document.contains("Binding notice text"));
    assert!(!document.contains("Notice:"));
    assert!(document.contains("Always visible"));
}
