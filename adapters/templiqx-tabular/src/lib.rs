//! Thin deterministic CSV and XML renderers over frozen tabular bindings.
//!
//! Column order comes only from the definition. Values are scalar approved
//! merge data; CSV fields are RFC-style quoted and XML text/attributes are
//! entity escaped. No query, expression, or provider behavior is available.

use std::{fmt::Write as _, fs, path::Path};

use serde_json::Value;
use sha2::{Digest, Sha256};
use templiqx_ports::{DocumentRenderRequest, DocumentRenderResult, DocumentRenderer, PortError};
use thiserror::Error;

pub const CSV_RENDERER_ID: &str = "templiqx-tabular-csv";
pub const XML_RENDERER_ID: &str = "templiqx-tabular-xml";
pub const RENDERER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ENVIRONMENT_ID: &str = "portable-tabular-v1";

#[derive(Debug, Default, Clone, Copy)]
pub struct CsvAdapter;

#[derive(Debug, Default, Clone, Copy)]
pub struct XmlAdapter;

#[derive(Debug, Error)]
enum TabularError {
    #[error("definition is not valid JSON: {0}")]
    DefinitionJson(#[from] serde_json::Error),
    #[error("invalid tabular definition: {0}")]
    InvalidDefinition(String),
    #[error("invalid tabular data: {0}")]
    InvalidData(String),
}

#[derive(Debug)]
struct Column {
    key: String,
    header: String,
}

#[derive(Debug)]
struct TabularBinding {
    rows_path: String,
    columns: Vec<Column>,
}

#[derive(Debug, Clone, Copy)]
enum OutputFormat {
    Csv,
    Xml,
}

impl DocumentRenderer for CsvAdapter {
    fn render_document(
        &self,
        request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError> {
        render_document(request, OutputFormat::Csv)
    }
}

impl DocumentRenderer for XmlAdapter {
    fn render_document(
        &self,
        request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError> {
        render_document(request, OutputFormat::Xml)
    }
}

fn render_document(
    request: &DocumentRenderRequest,
    format: OutputFormat,
) -> Result<DocumentRenderResult, PortError> {
    let binding = read_binding(&request.template).map_err(invalid_data)?;
    let rows = resolve_rows(&request.data, &binding.rows_path).map_err(invalid_data)?;
    let rendered = match format {
        OutputFormat::Csv => render_csv(&binding.columns, rows).map_err(invalid_data)?,
        OutputFormat::Xml => render_xml(&binding.columns, rows).map_err(invalid_data)?,
    };

    if let Some(parent) = request.output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| PortError::Io(format!("create tabular output dir: {error}")))?;
    }
    fs::write(&request.output, rendered.as_bytes())
        .map_err(|error| PortError::Io(format!("write tabular output: {error}")))?;

    let renderer_id = match format {
        OutputFormat::Csv => CSV_RENDERER_ID,
        OutputFormat::Xml => XML_RENDERER_ID,
    };
    let format_name = match format {
        OutputFormat::Csv => "csv",
        OutputFormat::Xml => "xml",
    };
    let fingerprint = sha256_hex(rendered.as_bytes());
    let artifact_bytes = u64::try_from(rendered.len())
        .map_err(|error| PortError::Io(format!("tabular artifact size overflow: {error}")))?;

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
            "format": format_name,
            "rows": rows.len(),
            "columns": binding.columns.len(),
        }),
    })
}

fn invalid_data(error: TabularError) -> PortError {
    PortError::InvalidData(error.to_string())
}

fn read_binding(path: &Path) -> Result<TabularBinding, TabularError> {
    let source = fs::read_to_string(path).map_err(|error| {
        TabularError::InvalidDefinition(format!("read {}: {error}", path.display()))
    })?;
    let root: Value = serde_json::from_str(&source)?;
    let definition = root.get("definition").unwrap_or(&root);
    let binding = definition
        .get("tabular_binding")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            TabularError::InvalidDefinition("missing object `tabular_binding`".to_owned())
        })?;
    let rows_path = required_string(binding.get("rows"), "tabular_binding.rows")?;
    let raw_columns = binding
        .get("columns")
        .and_then(Value::as_array)
        .filter(|columns| !columns.is_empty())
        .ok_or_else(|| {
            TabularError::InvalidDefinition(
                "tabular_binding.columns must be a non-empty array".to_owned(),
            )
        })?;
    let mut columns = Vec::with_capacity(raw_columns.len());
    for (index, column) in raw_columns.iter().enumerate() {
        let object = column.as_object().ok_or_else(|| {
            TabularError::InvalidDefinition(format!("columns[{index}] must be an object"))
        })?;
        columns.push(Column {
            key: required_string(object.get("key"), &format!("columns[{index}].key"))?,
            header: required_string(object.get("header"), &format!("columns[{index}].header"))?,
        });
    }
    Ok(TabularBinding { rows_path, columns })
}

fn required_string(value: Option<&Value>, field: &str) -> Result<String, TabularError> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| TabularError::InvalidDefinition(format!("{field} must be non-empty")))
}

fn resolve_rows<'a>(data: &'a Value, path: &str) -> Result<&'a [Value], TabularError> {
    let mut current = data;
    for segment in path.split('.') {
        current = current.get(segment).ok_or_else(|| {
            TabularError::InvalidData(format!("row binding `{path}` was not supplied"))
        })?;
    }
    current
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| TabularError::InvalidData(format!("row binding `{path}` must be an array")))
}

fn render_csv(columns: &[Column], rows: &[Value]) -> Result<String, TabularError> {
    let mut output = String::new();
    write_csv_record(
        &mut output,
        columns.iter().map(|column| column.header.as_str()),
    );
    for (row_index, row) in rows.iter().enumerate() {
        let object = row.as_object().ok_or_else(|| {
            TabularError::InvalidData(format!("rows[{row_index}] must be an object"))
        })?;
        let values = columns
            .iter()
            .map(|column| {
                object
                    .get(&column.key)
                    .ok_or_else(|| {
                        TabularError::InvalidData(format!(
                            "rows[{row_index}] is missing column `{}`",
                            column.key
                        ))
                    })
                    .and_then(scalar)
            })
            .collect::<Result<Vec<_>, _>>()?;
        write_csv_record(&mut output, values.iter().map(String::as_str));
    }
    Ok(output)
}

fn write_csv_record<'a>(output: &mut String, values: impl Iterator<Item = &'a str>) {
    for (index, value) in values.enumerate() {
        if index > 0 {
            output.push(',');
        }
        write_csv_field(output, value);
    }
    output.push('\n');
}

fn write_csv_field(output: &mut String, value: &str) {
    let formula_like = value.starts_with(['=', '+', '@'])
        || value
            .strip_prefix('-')
            .is_some_and(|rest| rest.chars().next().is_some_and(|ch| !ch.is_ascii_digit()));
    let escaped = if formula_like {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if escaped.contains([',', '"', '\r', '\n']) {
        output.push('"');
        output.push_str(&escaped.replace('"', "\"\""));
        output.push('"');
    } else {
        output.push_str(&escaped);
    }
}

fn render_xml(columns: &[Column], rows: &[Value]) -> Result<String, TabularError> {
    let mut output =
        String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<table>\n  <columns>\n");
    for column in columns {
        writeln!(
            output,
            "    <column key=\"{}\">{}</column>",
            escape_xml(&column.key),
            escape_xml(&column.header)
        )
        .expect("writing to String cannot fail");
    }
    output.push_str("  </columns>\n  <rows>\n");
    for (row_index, row) in rows.iter().enumerate() {
        let object = row.as_object().ok_or_else(|| {
            TabularError::InvalidData(format!("rows[{row_index}] must be an object"))
        })?;
        output.push_str("    <row>\n");
        for column in columns {
            let value = object.get(&column.key).ok_or_else(|| {
                TabularError::InvalidData(format!(
                    "rows[{row_index}] is missing column `{}`",
                    column.key
                ))
            })?;
            writeln!(
                output,
                "      <cell column=\"{}\">{}</cell>",
                escape_xml(&column.key),
                escape_xml(&scalar(value)?)
            )
            .expect("writing to String cannot fail");
        }
        output.push_str("    </row>\n");
    }
    output.push_str("  </rows>\n</table>\n");
    Ok(output)
}

fn scalar(value: &Value) -> Result<String, TabularError> {
    match value {
        Value::Null => Ok(String::new()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => Ok(value.clone()),
        Value::Array(_) | Value::Object(_) => Err(TabularError::InvalidData(
            "tabular cell values must be scalar".to_owned(),
        )),
    }
}

fn escape_xml(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            other => output.push(other),
        }
    }
    output
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}
