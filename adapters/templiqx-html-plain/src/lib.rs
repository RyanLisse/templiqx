//! U7 (plan 001): a minimal, optional HTML / plain-text document renderer.
//!
//! Deterministic, host-constructed adapter for email and web snippets. It is a
//! bounded field-interpolation renderer — NOT a full contract-AST or template
//! language. Supported template syntax:
//!
//! - `{{ field }}` — HTML-escaped scalar lookup in the merge data object.
//! - `{{#each list}} … {{/each}}` — one level of iteration over a JSON array.
//!   Inside a block, `{{ this }}` is the current scalar item and `{{ field }}`
//!   is a field of the current object item.
//!
//! Unknown fields render as an empty string and are reported. There is no code
//! execution, no nested `each`, no conditionals — those stay out of scope by
//! design (KTD5). The adapter is never wired into the default CLI/MCP
//! composition; a host constructs it explicitly, like the runtime adapters.

use std::collections::BTreeSet;
use std::fs;

use serde_json::Value;
use templiqx_ports::{
    DocumentRenderRequest, DocumentRenderResult, DocumentRenderer, PortError,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct HtmlPlainAdapter;

impl DocumentRenderer for HtmlPlainAdapter {
    fn render_document(
        &self,
        request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError> {
        let template = fs::read_to_string(&request.template)
            .map_err(|e| PortError::Io(format!("read template: {e}")))?;
        let mut unresolved = BTreeSet::new();
        let rendered = render(&template, &request.data, &mut unresolved);
        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent).map_err(|e| PortError::Io(format!("create output dir: {e}")))?;
        }
        fs::write(&request.output, rendered.as_bytes())
            .map_err(|e| PortError::Io(format!("write output: {e}")))?;
        let report = serde_json::json!({
            "adapter": "templiqx-html-plain",
            "unresolved_fields": unresolved.into_iter().collect::<Vec<_>>(),
            "bytes": rendered.len(),
        });
        Ok(DocumentRenderResult {
            artifact: request.output.clone(),
            report,
        })
    }
}

/// Expand `{{#each}}` blocks first, then remaining `{{ field }}` placeholders.
fn render(template: &str, data: &Value, unresolved: &mut BTreeSet<String>) -> String {
    let expanded = expand_each(template, data, unresolved);
    replace_fields(&expanded, data, unresolved)
}

/// Expand a single level of `{{#each name}} BODY {{/each}}`. Nested `each` is
/// intentionally unsupported; an inner `{{#each}}` inside a body is treated as
/// literal text (rendered fields inside still resolve against the item).
fn expand_each(template: &str, data: &Value, unresolved: &mut BTreeSet<String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{#each ") {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + "{{#each ".len()..];
        let Some(name_end) = after_open.find("}}") else {
            // Malformed open tag: emit the remainder verbatim and stop.
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

/// Replace `{{ field }}` (and `{{ this }}`) placeholders with escaped values.
fn replace_fields(template: &str, context: &Value, unresolved: &mut BTreeSet<String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        // Leave `{{#each` / `{{/each}}` markers for the each pass; they should
        // already be gone, but guard so control tokens are never mis-parsed.
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
            Some(v) => out.push_str(&escape(&scalar(&v))),
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

fn escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn go(tpl: &str, data: Value) -> (String, Vec<String>) {
        let mut unresolved = BTreeSet::new();
        let out = render(tpl, &data, &mut unresolved);
        (out, unresolved.into_iter().collect())
    }

    #[test]
    fn fields_are_escaped() {
        let (out, unresolved) = go("<p>{{ name }}</p>", json!({ "name": "<b> & \"x\"" }));
        assert_eq!(out, "<p>&lt;b&gt; &amp; &quot;x&quot;</p>");
        assert!(unresolved.is_empty());
    }

    #[test]
    fn each_over_objects() {
        let (out, _) = go(
            "<ul>{{#each items}}<li>{{ label }}</li>{{/each}}</ul>",
            json!({ "items": [{ "label": "a" }, { "label": "b" }] }),
        );
        assert_eq!(out, "<ul><li>a</li><li>b</li></ul>");
    }

    #[test]
    fn each_over_scalars_uses_this() {
        let (out, _) = go(
            "{{#each xs}}[{{ this }}]{{/each}}",
            json!({ "xs": ["p", "q"] }),
        );
        assert_eq!(out, "[p][q]");
    }

    #[test]
    fn missing_field_is_reported_and_empty() {
        let (out, unresolved) = go("<p>{{ ghost }}</p>", json!({}));
        assert_eq!(out, "<p></p>");
        assert_eq!(unresolved, vec!["ghost".to_owned()]);
    }
}
