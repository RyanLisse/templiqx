//! Deterministic XLSX renderer for frozen tabular report definitions.
//!
//! The adapter reads an ordered `tabular_binding` from the JSON definition at
//! `request.template`, resolves its row array from approved `request.data`, and
//! emits typed cells plus one native Excel column chart. It performs no query,
//! expression, formula, or model execution.

use std::{fmt::Write as _, fs, path::Path};

use rust_xlsxwriter::{Chart, ChartType, DocProperties, ExcelDateTime, Workbook, Worksheet};
use serde_json::Value;
use sha2::{Digest, Sha256};
use templiqx_ports::{DocumentRenderRequest, DocumentRenderResult, DocumentRenderer, PortError};
use thiserror::Error;

pub const RENDERER_ID: &str = "templiqx-xlsx";
pub const RENDERER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ENVIRONMENT_ID: &str = "rust-xlsxwriter-0.96-fixed-metadata-v1";

const SHEET_NAME: &str = "Report";

#[derive(Debug, Default, Clone, Copy)]
pub struct XlsxAdapter;

#[derive(Debug, Error)]
enum XlsxAdapterError {
    #[error("definition is not valid JSON: {0}")]
    DefinitionJson(#[from] serde_json::Error),
    #[error("invalid tabular definition: {0}")]
    InvalidDefinition(String),
    #[error("invalid tabular data: {0}")]
    InvalidData(String),
    #[error("XLSX write failed: {0}")]
    Workbook(#[from] rust_xlsxwriter::XlsxError),
}

#[derive(Debug)]
struct Column {
    key: String,
    header: String,
}

#[derive(Debug)]
struct ChartBinding {
    title: String,
    category_index: u16,
    value_index: u16,
}

#[derive(Debug)]
struct TabularBinding {
    rows_path: String,
    columns: Vec<Column>,
    chart: ChartBinding,
}

impl DocumentRenderer for XlsxAdapter {
    fn render_document(
        &self,
        request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError> {
        let binding = read_binding(&request.template).map_err(invalid_data)?;
        let rows = resolve_rows(&request.data, &binding.rows_path).map_err(invalid_data)?;

        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| PortError::Io(format!("create XLSX output dir: {error}")))?;
        }

        write_workbook(&request.output, &binding, rows).map_err(invalid_data)?;
        let bytes = fs::read(&request.output)
            .map_err(|error| PortError::Io(format!("read rendered XLSX: {error}")))?;
        let fingerprint = sha256_hex(&bytes);
        let artifact_bytes = u64::try_from(bytes.len())
            .map_err(|error| PortError::Io(format!("XLSX artifact size overflow: {error}")))?;

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
                "rows": rows.len(),
                "columns": binding.columns.len(),
                "native_chart": "column",
            }),
        })
    }
}

fn invalid_data(error: XlsxAdapterError) -> PortError {
    PortError::InvalidData(error.to_string())
}

fn read_binding(path: &Path) -> Result<TabularBinding, XlsxAdapterError> {
    let source = fs::read_to_string(path).map_err(|error| {
        XlsxAdapterError::InvalidDefinition(format!("read {}: {error}", path.display()))
    })?;
    let root: Value = serde_json::from_str(&source)?;
    let definition = root.get("definition").unwrap_or(&root);
    let binding = definition
        .get("tabular_binding")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            XlsxAdapterError::InvalidDefinition("missing object `tabular_binding`".to_owned())
        })?;
    let rows_path = required_string(binding.get("rows"), "tabular_binding.rows")?;
    let raw_columns = binding
        .get("columns")
        .and_then(Value::as_array)
        .filter(|columns| !columns.is_empty())
        .ok_or_else(|| {
            XlsxAdapterError::InvalidDefinition(
                "tabular_binding.columns must be a non-empty array".to_owned(),
            )
        })?;
    let mut columns = Vec::with_capacity(raw_columns.len());
    for (index, column) in raw_columns.iter().enumerate() {
        let object = column.as_object().ok_or_else(|| {
            XlsxAdapterError::InvalidDefinition(format!("columns[{index}] must be an object"))
        })?;
        columns.push(Column {
            key: required_string(object.get("key"), &format!("columns[{index}].key"))?,
            header: required_string(object.get("header"), &format!("columns[{index}].header"))?,
        });
    }
    let chart = binding
        .get("chart")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            XlsxAdapterError::InvalidDefinition("missing object `tabular_binding.chart`".to_owned())
        })?;
    if required_string(chart.get("type"), "chart.type")? != "column" {
        return Err(XlsxAdapterError::InvalidDefinition(
            "only native column charts are supported".to_owned(),
        ));
    }
    let category = required_string(chart.get("category"), "chart.category")?;
    let value = required_string(chart.get("value"), "chart.value")?;
    let category_index = column_index(&columns, &category, "chart.category")?;
    let value_index = column_index(&columns, &value, "chart.value")?;

    Ok(TabularBinding {
        rows_path,
        columns,
        chart: ChartBinding {
            title: required_string(chart.get("title"), "chart.title")?,
            category_index,
            value_index,
        },
    })
}

fn required_string(value: Option<&Value>, field: &str) -> Result<String, XlsxAdapterError> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| XlsxAdapterError::InvalidDefinition(format!("{field} must be non-empty")))
}

fn column_index(columns: &[Column], key: &str, field: &str) -> Result<u16, XlsxAdapterError> {
    let index = columns
        .iter()
        .position(|column| column.key == key)
        .ok_or_else(|| {
            XlsxAdapterError::InvalidDefinition(format!(
                "{field} references unknown column `{key}`"
            ))
        })?;
    u16::try_from(index).map_err(|_| {
        XlsxAdapterError::InvalidDefinition("column count exceeds XLSX limits".to_owned())
    })
}

fn resolve_rows<'a>(data: &'a Value, path: &str) -> Result<&'a [Value], XlsxAdapterError> {
    let mut current = data;
    for segment in path.split('.') {
        current = current.get(segment).ok_or_else(|| {
            XlsxAdapterError::InvalidData(format!("row binding `{path}` was not supplied"))
        })?;
    }
    current
        .as_array()
        .filter(|rows| !rows.is_empty())
        .map(Vec::as_slice)
        .ok_or_else(|| {
            XlsxAdapterError::InvalidData(format!("row binding `{path}` must be a non-empty array"))
        })
}

fn write_workbook(
    output: &Path,
    binding: &TabularBinding,
    rows: &[Value],
) -> Result<(), XlsxAdapterError> {
    let mut workbook = Workbook::new();
    let fixed_date = ExcelDateTime::from_ymd(2000, 1, 1)?.and_hms(0, 0, 0)?;
    let properties = DocProperties::new()
        .set_creation_datetime(&fixed_date)
        .set_author("Templiqx")
        .set_title(&binding.chart.title);
    workbook.set_properties(&properties);

    let worksheet = workbook.add_worksheet();
    worksheet.set_name(SHEET_NAME)?;
    write_headers(worksheet, &binding.columns)?;
    write_rows(worksheet, &binding.columns, rows)?;

    let last_row = u32::try_from(rows.len())
        .map_err(|_| XlsxAdapterError::InvalidData("row count exceeds XLSX limits".to_owned()))?;
    let mut chart = Chart::new(ChartType::Column);
    chart.title().set_name(&binding.chart.title);
    chart
        .add_series()
        .set_categories((
            SHEET_NAME,
            1,
            binding.chart.category_index,
            last_row,
            binding.chart.category_index,
        ))
        .set_values((
            SHEET_NAME,
            1,
            binding.chart.value_index,
            last_row,
            binding.chart.value_index,
        ))
        .set_name(&binding.columns[usize::from(binding.chart.value_index)].header);
    let chart_column = u16::try_from(binding.columns.len() + 1).map_err(|_| {
        XlsxAdapterError::InvalidData("column count exceeds XLSX limits".to_owned())
    })?;
    worksheet.insert_chart(1, chart_column, &chart)?;
    workbook.save(output)?;
    Ok(())
}

fn write_headers(worksheet: &mut Worksheet, columns: &[Column]) -> Result<(), XlsxAdapterError> {
    for (index, column) in columns.iter().enumerate() {
        let column_index = u16::try_from(index).map_err(|_| {
            XlsxAdapterError::InvalidData("column count exceeds XLSX limits".to_owned())
        })?;
        worksheet.write_string(0, column_index, &column.header)?;
    }
    Ok(())
}

fn write_rows(
    worksheet: &mut Worksheet,
    columns: &[Column],
    rows: &[Value],
) -> Result<(), XlsxAdapterError> {
    for (row_index, row) in rows.iter().enumerate() {
        let object = row.as_object().ok_or_else(|| {
            XlsxAdapterError::InvalidData(format!("rows[{row_index}] must be an object"))
        })?;
        let sheet_row = u32::try_from(row_index + 1).map_err(|_| {
            XlsxAdapterError::InvalidData("row count exceeds XLSX limits".to_owned())
        })?;
        for (column_index, column) in columns.iter().enumerate() {
            let sheet_column = u16::try_from(column_index).map_err(|_| {
                XlsxAdapterError::InvalidData("column count exceeds XLSX limits".to_owned())
            })?;
            let value = object.get(&column.key).ok_or_else(|| {
                XlsxAdapterError::InvalidData(format!(
                    "rows[{row_index}] is missing column `{}`",
                    column.key
                ))
            })?;
            match value {
                Value::Null => {}
                Value::Bool(value) => {
                    worksheet.write_boolean(sheet_row, sheet_column, *value)?;
                }
                Value::Number(value) => {
                    let value = value.as_f64().ok_or_else(|| {
                        XlsxAdapterError::InvalidData(format!(
                            "rows[{row_index}].{} is not an XLSX number",
                            column.key
                        ))
                    })?;
                    worksheet.write_number(sheet_row, sheet_column, value)?;
                }
                Value::String(value) => {
                    worksheet.write_string(sheet_row, sheet_column, value)?;
                }
                Value::Array(_) | Value::Object(_) => {
                    return Err(XlsxAdapterError::InvalidData(format!(
                        "rows[{row_index}].{} must be scalar",
                        column.key
                    )));
                }
            }
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to String cannot fail");
            output
        })
}
