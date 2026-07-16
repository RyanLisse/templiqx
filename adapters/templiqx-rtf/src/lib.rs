//! Bounded deterministic RTF document renderer.
//!
//! Hand-rolled RTF emitter (no `scrivener-rtf`): RTF is plain text with a small
//! control vocabulary, and a minimal deterministic writer avoids an unneeded
//! dependency. Same field-interpolation model as `templiqx-html-plain` — no
//! code execution. Host-constructed only; never wired into default composition.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;

use serde_json::Value;
use sha2::{Digest, Sha256};
use templiqx_ports::{DocumentRenderRequest, DocumentRenderResult, DocumentRenderer, PortError};

pub const RENDERER_ID: &str = "templiqx-rtf";
pub const RENDERER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ENVIRONMENT_ID: &str = "handrolled-rtf-v1";

#[derive(Debug, Default, Clone, Copy)]
pub struct RtfAdapter;

impl DocumentRenderer for RtfAdapter {
    fn render_document(
        &self,
        request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError> {
        let template = fs::read_to_string(&request.template)
            .map_err(|error| PortError::Io(format!("read RTF template: {error}")))?;
        let mut unresolved = BTreeSet::new();
        let body = render(&template, &request.data, &mut unresolved);
        let document = wrap_rtf(&body);

        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| PortError::Io(format!("create RTF output dir: {error}")))?;
        }
        fs::write(&request.output, document.as_bytes())
            .map_err(|error| PortError::Io(format!("write RTF output: {error}")))?;

        let fingerprint = sha256_hex(document.as_bytes());
        let artifact_bytes = u64::try_from(document.len())
            .map_err(|error| PortError::Io(format!("RTF artifact size overflow: {error}")))?;

        Ok(DocumentRenderResult {
            artifact: request.output.clone(),
            report: serde_json::json!({
                "adapter": RENDERER_ID,
                "renderer_id": RENDERER_ID,
                "renderer_version": RENDERER_VERSION,
                "environment_id": ENVIRONMENT_ID,
                "artifact_fingerprint": fingerprint,
                "artifact_bytes": artifact_bytes,
                "output_hash": fingerprint,
                "status": "ok",
                "unresolved_fields": unresolved.into_iter().collect::<Vec<_>>(),
            }),
        })
    }
}

fn wrap_rtf(body: &str) -> String {
    format!("{{\\rtf1\\ansi\\deff0{{\\fonttbl{{\\f0 Times New Roman;}}}}\\f0\\fs24 {body}}}")
}

fn render(template: &str, data: &Value, unresolved: &mut BTreeSet<String>) -> String {
    let expanded = expand_each(template, data, unresolved);
    replace_fields(&expanded, data, unresolved)
}

fn expand_each(template: &str, data: &Value, unresolved: &mut BTreeSet<String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{#each ") {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + "{{#each ".len()..];
        let Some(name_end) = after_open.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = after_open[..name_end].trim().to_owned();
        let body_start = &after_open[name_end + 2..];
        let Some(close) = body_start.find("{{/each}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let body = &body_start[..close];
        match data.get(&name) {
            Some(Value::Array(items)) => {
                for item in items {
                    out.push_str(&replace_fields(body, item, unresolved));
                }
            }
            _ => {
                unresolved.insert(name);
            }
        }
        rest = &body_start[close + "{{/each}}".len()..];
    }
    out.push_str(rest);
    out
}

fn replace_fields(template: &str, context: &Value, unresolved: &mut BTreeSet<String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        if rest[start..].starts_with("{{#") || rest[start..].starts_with("{{/") {
            out.push_str(&rest[..start + 2]);
            rest = &rest[start + 2..];
            continue;
        }
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let key = after[..end].trim();
        let value = if key == "this" || key == "." {
            Some(context.clone())
        } else {
            context.get(key).cloned()
        };
        match value {
            Some(Value::Null) | None => {
                unresolved.insert(key.to_owned());
            }
            Some(v) => out.push_str(&escape_rtf(&scalar(&v))),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Escape RTF control characters and encode non-ASCII as `\\uN?`.
fn escape_rtf(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\par "),
            c if c.is_ascii() => out.push(c),
            c => {
                // RTF `\uN?` carries signed UTF-16 code units. Encoding every
                // unit also preserves non-BMP characters as surrogate pairs.
                let mut encoded = [0_u16; 2];
                for unit in c.encode_utf16(&mut encoded) {
                    let signed = i16::from_ne_bytes(unit.to_ne_bytes());
                    write!(out, "\\u{signed}?").expect("writing to String cannot fail");
                }
            }
        }
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}
