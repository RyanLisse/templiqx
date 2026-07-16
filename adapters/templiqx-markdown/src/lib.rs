//! Bounded Markdown text adapter (`markdown-rs`).
//!
//! Deterministic, host-constructed renderer for memo/report/email-style output.
//! Flow: definition/template + approved merge data → Markdown → safe HTML or
//! plain text. Raw HTML in merge values is escaped before compilation;
//! `allow_dangerous_html` stays false. No MDX / embedded code.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;

use markdown::{CompileOptions, Options, to_html_with_options};
use serde_json::Value;
use sha2::{Digest, Sha256};
use templiqx_ports::{DocumentRenderRequest, DocumentRenderResult, DocumentRenderer, PortError};

pub const RENDERER_ID: &str = "templiqx-markdown";
pub const HTML_RENDERER_ID: &str = "templiqx-markdown-html";
pub const PLAIN_RENDERER_ID: &str = "templiqx-markdown-plain";
pub const RENDERER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ENVIRONMENT_ID: &str = "markdown-rs-1.0-safe-html-v1";

/// Bounded Markdown renderer. Construct via [`MarkdownAdapter::html`] or
/// [`MarkdownAdapter::plain_text`].
#[derive(Debug, Clone, Copy)]
pub struct MarkdownAdapter {
    kind: OutputKind,
}

#[derive(Debug, Clone, Copy)]
enum OutputKind {
    Html,
    Plain,
}

impl MarkdownAdapter {
    #[must_use]
    pub const fn html() -> Self {
        Self {
            kind: OutputKind::Html,
        }
    }

    #[must_use]
    pub const fn plain_text() -> Self {
        Self {
            kind: OutputKind::Plain,
        }
    }
}

impl DocumentRenderer for MarkdownAdapter {
    fn render_document(
        &self,
        request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError> {
        let template = fs::read_to_string(&request.template)
            .map_err(|error| PortError::Io(format!("read markdown template: {error}")))?;
        let mut unresolved = BTreeSet::new();
        let markdown = render_markdown(&template, &request.data, &mut unresolved);
        let body = match self.kind {
            OutputKind::Html => compile_safe_html(&markdown)?,
            OutputKind::Plain => markdown_to_plain(&markdown),
        };

        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| PortError::Io(format!("create markdown output dir: {error}")))?;
        }
        fs::write(&request.output, body.as_bytes())
            .map_err(|error| PortError::Io(format!("write markdown output: {error}")))?;

        let renderer_id = match self.kind {
            OutputKind::Html => RENDERER_ID,
            OutputKind::Plain => PLAIN_RENDERER_ID,
        };
        let fingerprint = sha256_hex(body.as_bytes());
        let artifact_bytes = u64::try_from(body.len())
            .map_err(|error| PortError::Io(format!("markdown artifact size overflow: {error}")))?;

        Ok(DocumentRenderResult {
            artifact: request.output.clone(),
            report: serde_json::json!({
                "adapter": renderer_id,
                "renderer_id": renderer_id,
                "renderer_version": RENDERER_VERSION,
                "environment_id": ENVIRONMENT_ID,
                "artifact_fingerprint": fingerprint,
                "artifact_bytes": artifact_bytes,
                "output_hash": fingerprint,
                "status": "ok",
                "format": match self.kind {
                    OutputKind::Html => "html",
                    OutputKind::Plain => "plain",
                },
                "unresolved_fields": unresolved.into_iter().collect::<Vec<_>>(),
            }),
        })
    }
}

fn compile_safe_html(markdown: &str) -> Result<String, PortError> {
    let options = Options {
        compile: CompileOptions {
            allow_dangerous_html: false,
            allow_dangerous_protocol: false,
            ..CompileOptions::default()
        },
        ..Options::default()
    };
    to_html_with_options(markdown, &options)
        .map_err(|error| PortError::InvalidData(format!("markdown compile failed: {error}")))
}

/// Strip a tiny subset of Markdown markers for plain output while keeping
/// HTML-escaped merge values intact (fail-closed against raw HTML injection).
fn markdown_to_plain(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    for line in markdown.lines() {
        let mut line = line;
        while let Some(rest) = line.strip_prefix('#') {
            line = rest.trim_start_matches(' ');
        }
        let mut plain = String::with_capacity(line.len());
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '*' && chars.peek() == Some(&'*') {
                chars.next();
                continue;
            }
            plain.push(ch);
        }
        out.push_str(&plain);
        out.push('\n');
    }
    out
}

fn render_markdown(template: &str, data: &Value, unresolved: &mut BTreeSet<String>) -> String {
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
            Some(v) => out.push_str(&escape_merge_value(&scalar(&v))),
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

/// Escape HTML-special characters so merge values cannot inject raw HTML/script
/// into the Markdown that later compiles with dangerous HTML disabled.
fn escape_merge_value(input: &str) -> String {
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

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}
