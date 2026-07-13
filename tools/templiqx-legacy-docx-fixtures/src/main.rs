//! Deterministic synthetic DOCX corpus generator.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>
"#;
const EMPTY_DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>
"#;
const HEADER_FOOTER_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/>
</Relationships>
"#;

fn main() -> Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_output);
    generate(&output)
}

fn default_output() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/legacy-corpus/fixtures")
}

fn generate(root: &Path) -> Result<()> {
    fs::create_dir_all(root)?;

    fixture(
        root,
        "v1-beanshell-detected",
        &[(
            "word/document.xml",
            document(
                r#"<w:p><w:r><w:t>Bean</w:t></w:r><w:r><w:t>Shell bsh.eval(&quot;never&quot;)</w:t></w:r></w:p>"#,
                "",
            ),
        )],
        json!({}),
        report(
            json!([finding(
                "unsafe",
                "word/document.xml",
                "v1_beanshell",
                None,
                "legacy executable content is never executed"
            )]),
            0,
            0,
            0,
            1,
            0,
        ),
        None,
        None,
    )?;
    fixture(
        root,
        "v2-marker-detected",
        &[(
            "word/document.xml",
            document(
                r#"<w:p><w:r><w:t>${v2:legacy.customer}</w:t></w:r></w:p>"#,
                "",
            ),
        )],
        json!({}),
        report(
            json!([finding(
                "unsupported",
                "word/document.xml",
                "v2",
                None,
                "V2 is detected but not migrated by this adapter"
            )]),
            0,
            0,
            1,
            0,
            0,
        ),
        None,
        None,
    )?;

    let nested_source = document(
        r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>$data.case.number</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:fldSimple w:instr=" MERGEFIELD case.owner "><w:r><w:t>owner</w:t></w:r></w:fldSimple></w:p></w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>"#,
        "",
    );
    let nested_baseline = document(
        r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>C-1042</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:fldSimple w:instr=" MERGEFIELD case.owner "><w:r><w:t>Ryan</w:t></w:r></w:fldSimple></w:p></w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>"#,
        "",
    );
    fixture(
        root,
        "v5-nested-table",
        &[("word/document.xml", nested_source)],
        json!({}),
        report(
            json!([
                finding(
                    "migrated",
                    "word/document.xml",
                    "v5_reference",
                    Some("case.number"),
                    "supported V5 reference"
                ),
                finding(
                    "migrated",
                    "word/document.xml",
                    "mergefield",
                    Some("case.owner"),
                    "ordinary Word MERGEFIELD"
                )
            ]),
            2,
            0,
            0,
            0,
            0,
        ),
        Some(json!({"case":{"number":"C-1042","owner":"Ryan"}})),
        Some(&[("word/document.xml", nested_baseline)]),
    )?;

    let hf_document = document(
        r#"<w:p><w:r><w:t>Body $data.case.number</w:t></w:r></w:p>"#,
        r#"<w:sectPr><w:headerReference w:type="default" r:id="rId1"/><w:footerReference w:type="default" r:id="rId2"/></w:sectPr>"#,
    );
    let hf_header = story(
        "hdr",
        r#"<w:p><w:r><w:t>${organisation.name}</w:t></w:r></w:p>"#,
    );
    let hf_footer = story(
        "ftr",
        r#"<w:p><w:fldSimple w:instr=" MERGEFIELD page.label "><w:r><w:t>page</w:t></w:r></w:fldSimple></w:p>"#,
    );
    let hf_baseline_document = document(
        r#"<w:p><w:r><w:t>Body C-1042</w:t></w:r></w:p>"#,
        r#"<w:sectPr><w:headerReference w:type="default" r:id="rId1"/><w:footerReference w:type="default" r:id="rId2"/></w:sectPr>"#,
    );
    let hf_baseline_header = story("hdr", r#"<w:p><w:r><w:t>Blinqx</w:t></w:r></w:p>"#);
    let hf_baseline_footer = story(
        "ftr",
        r#"<w:p><w:fldSimple w:instr=" MERGEFIELD page.label "><w:r><w:t>Page 1</w:t></w:r></w:fldSimple></w:p>"#,
    );
    fixture(
        root,
        "v5-header-footer",
        &[
            ("word/document.xml", hf_document),
            ("word/header1.xml", hf_header),
            ("word/footer1.xml", hf_footer),
        ],
        json!({}),
        report(
            json!([
                finding(
                    "migrated",
                    "word/document.xml",
                    "v5_reference",
                    Some("case.number"),
                    "supported V5 reference"
                ),
                finding(
                    "migrated",
                    "word/footer1.xml",
                    "mergefield",
                    Some("page.label"),
                    "ordinary Word MERGEFIELD"
                ),
                finding(
                    "migrated",
                    "word/header1.xml",
                    "v5_reference",
                    Some("organisation.name"),
                    "supported V5 reference"
                )
            ]),
            3,
            0,
            0,
            0,
            0,
        ),
        Some(
            json!({"case":{"number":"C-1042"},"organisation":{"name":"Blinqx"},"page":{"label":"Page 1"}}),
        ),
        Some(&[
            ("word/document.xml", hf_baseline_document),
            ("word/header1.xml", hf_baseline_header),
            ("word/footer1.xml", hf_baseline_footer),
        ]),
    )?;

    let alias_source = document(
        r#"<w:p><w:r><w:t>$data.client.old_name / ${customer.former_name}</w:t></w:r></w:p><w:p><w:fldSimple w:instr=" MERGEFIELD missing.value "><w:r><w:t>missing</w:t></w:r></w:fldSimple></w:p>"#,
        "",
    );
    let alias_baseline = document(
        r#"<w:p><w:r><w:t>Ada / Ada</w:t></w:r></w:p><w:p><w:fldSimple w:instr=" MERGEFIELD missing.value "><w:r><w:t>missing</w:t></w:r></w:fldSimple></w:p>"#,
        "",
    );
    fixture(
        root,
        "v5-alias-collision-missing",
        &[("word/document.xml", alias_source)],
        json!({"client.old_name":"person.name","customer.former_name":"person.name"}),
        report(
            json!([
                finding(
                    "migrated",
                    "word/document.xml",
                    "v5_reference",
                    Some("person.name"),
                    "alias `client.old_name` normalized"
                ),
                finding(
                    "migrated",
                    "word/document.xml",
                    "v5_reference",
                    Some("person.name"),
                    "alias `customer.former_name` normalized"
                ),
                finding(
                    "migrated",
                    "word/document.xml",
                    "mergefield",
                    Some("missing.value"),
                    "ordinary Word MERGEFIELD"
                )
            ]),
            3,
            0,
            0,
            0,
            0,
        ),
        Some(json!({"person":{"name":"Ada"}})),
        Some(&[("word/document.xml", alias_baseline)]),
    )?;

    hostile_fixtures(root)?;
    Ok(())
}

fn fixture(
    root: &Path,
    id: &str,
    parts: &[(&str, String)],
    aliases: Value,
    expected_report: Value,
    render_data: Option<Value>,
    baseline_parts: Option<&[(&str, String)]>,
) -> Result<()> {
    let directory = root.join(id);
    fs::create_dir_all(&directory)?;
    write_docx(&directory.join("source.docx"), parts, false)?;
    write_json(&directory.join("aliases.json"), &aliases)?;
    write_json(&directory.join("expected-report.json"), &expected_report)?;
    if let Some(data) = render_data {
        write_json(&directory.join("render-data.json"), &data)?;
    }
    if let Some(parts) = baseline_parts {
        write_docx(&directory.join("expected-render.docx"), parts, false)?;
    }
    Ok(())
}

fn hostile_fixtures(root: &Path) -> Result<()> {
    let corrupt = root.join("invalid-corrupt");
    fs::create_dir_all(&corrupt)?;
    fs::write(corrupt.join("source.docx"), b"not-a-zip\n")?;
    write_json(
        &corrupt.join("expected-error.json"),
        &json!({"kind":"io","contains":"invalid DOCX ZIP"}),
    )?;

    let oversized = root.join("invalid-oversized-entry");
    fs::create_dir_all(&oversized)?;
    let large = format!(
        r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:body></w:document>"#,
        "A".repeat(2048)
    );
    write_docx(
        &oversized.join("source.docx"),
        &[("word/document.xml", large)],
        false,
    )?;
    write_json(
        &oversized.join("expected-error.json"),
        &json!({"kind":"invalid_data","contains":"exceeds per-entry limit","max_entry_bytes":1024}),
    )?;

    let traversal = root.join("invalid-traversal");
    fs::create_dir_all(&traversal)?;
    write_docx(
        &traversal.join("source.docx"),
        &[("word/document.xml", document("<w:p/>", ""))],
        true,
    )?;
    write_json(
        &traversal.join("expected-error.json"),
        &json!({"kind":"invalid_data","contains":"unsafe ZIP member name"}),
    )?;
    Ok(())
}

fn write_docx(path: &Path, parts: &[(&str, String)], traversal: bool) -> Result<()> {
    let content_types = content_types(parts);
    let mut entries = BTreeMap::from([
        ("[Content_Types].xml".to_owned(), content_types.into_bytes()),
        ("_rels/.rels".to_owned(), ROOT_RELS.as_bytes().to_vec()),
    ]);
    let has_header_footer = parts
        .iter()
        .any(|(name, _)| name.starts_with("word/header") || name.starts_with("word/footer"));
    entries.insert(
        "word/_rels/document.xml.rels".to_owned(),
        if has_header_footer {
            HEADER_FOOTER_RELS
        } else {
            EMPTY_DOCUMENT_RELS
        }
        .as_bytes()
        .to_vec(),
    );
    for (name, value) in parts {
        entries.insert((*name).to_owned(), value.as_bytes().to_vec());
    }
    if traversal {
        entries.insert("../escape.txt".to_owned(), b"must never be read".to_vec());
    }
    let file = File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);
    for (name, bytes) in entries {
        zip.start_file(name, options)?;
        zip.write_all(&bytes)?;
    }
    zip.finish()?;
    Ok(())
}

fn content_types(parts: &[(&str, String)]) -> String {
    let mut overrides = vec![
        r#"  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#,
    ];
    if parts.iter().any(|(name, _)| name == &"word/header1.xml") {
        overrides.push(r#"  <Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>"#);
    }
    if parts.iter().any(|(name, _)| name == &"word/footer1.xml") {
        overrides.push(r#"  <Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>"#);
    }
    format!(
        "{}{}{}",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
"#,
        overrides.join("\n"),
        "\n</Types>\n"
    )
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn document(body: &str, section: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}{section}</w:body></w:document>
"#
    )
}

fn story(kind: &str, content: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:{kind} xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{content}</w:{kind}>
"#
    )
}

fn finding(
    category: &str,
    part: &str,
    construct: &str,
    reference: Option<&str>,
    detail: &str,
) -> Value {
    let mut finding =
        json!({"category":category,"part":part,"construct":construct,"detail":detail});
    if let Some(reference) = reference {
        finding["reference"] = json!(reference);
    }
    finding
}

fn report(
    findings: Value,
    migrated: usize,
    approximated: usize,
    unsupported: usize,
    unsafe_constructs: usize,
    unresolved: usize,
) -> Value {
    json!({
        "dialect":"v5",
        "findings":findings,
        "migrated":migrated,
        "approximated":approximated,
        "unsupported":unsupported,
        "unsafe_constructs":unsafe_constructs,
        "unresolved":unresolved
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_corpus_is_byte_for_byte_reproducible() {
        let temporary = tempfile::tempdir().unwrap();
        generate(temporary.path()).unwrap();
        let checked_in = default_output();
        let generated = files(temporary.path());
        let expected = files(&checked_in);
        assert_eq!(
            generated.keys().collect::<Vec<_>>(),
            expected.keys().collect::<Vec<_>>()
        );
        for (path, bytes) in generated {
            assert_eq!(
                bytes,
                expected[&path],
                "fixture differs after regeneration: {}",
                path.display()
            );
        }
    }

    fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut pending = vec![root.to_owned()];
        let mut result = BTreeMap::new();
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(entry.path());
                } else {
                    result.insert(
                        entry.path().strip_prefix(root).unwrap().to_owned(),
                        fs::read(entry.path()).unwrap(),
                    );
                }
            }
        }
        result
    }
}
