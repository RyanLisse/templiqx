//! Optional deterministic Typst-markup renderer.
//!
//! This adapter emits portable `.typ` source. It does not compile PDF and does
//! not depend on the Typst compiler. A host may pass the byte-stable artifact
//! through the separately constructed document-conversion seam.
//!
//! Supported syntax is deliberately bounded:
//!
//! - `{{ dotted.field }}` interpolates a scalar value.
//! - `{{ value | number:nl-NL }}` and `{{ value | date:nl-NL }}` apply pure,
//!   deterministic formatting.
//! - `{{#each rows}} ... {{/each}}` expands one array level.
//! - `{{#chart rows locale=nl-NL}}` emits a native Typst table from tabular
//!   JSON data.
//!
//! Unknown, null, or non-scalar fields become empty content and are reported
//! as fail-closed diagnostics. There is no expression evaluation, conditionals,
//! nested iteration, file access, or code execution.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;

use serde_json::{Number, Value};
use templiqx_ports::{DocumentRenderRequest, DocumentRenderResult, DocumentRenderer, PortError};

pub const RENDERER_ID: &str = "templiqx-typst";
pub const RENDERER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const ENVIRONMENT_ID: &str = "portable-typst-markup-v1";

#[derive(Debug, Default, Clone, Copy)]
pub struct TypstReportAdapter;

impl DocumentRenderer for TypstReportAdapter {
    fn render_document(
        &self,
        request: &DocumentRenderRequest,
    ) -> Result<DocumentRenderResult, PortError> {
        let template = fs::read_to_string(&request.template)
            .map_err(|error| PortError::Io(format!("read Typst template: {error}")))?;
        let mut unresolved = BTreeSet::new();
        let rendered = render(&template, &request.data, &mut unresolved);

        if let Some(parent) = request.output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| PortError::Io(format!("create output dir: {error}")))?;
        }
        fs::write(&request.output, rendered.as_bytes())
            .map_err(|error| PortError::Io(format!("write Typst markup: {error}")))?;

        let artifact_fingerprint = sha256_hex(rendered.as_bytes());
        let artifact_bytes = u64::try_from(rendered.len())
            .map_err(|error| PortError::Io(format!("Typst artifact size overflow: {error}")))?;
        let unresolved_fields: Vec<String> = unresolved.into_iter().collect();
        let diagnostics: Vec<Value> = unresolved_fields
            .iter()
            .map(|field| {
                serde_json::json!({
                    "code": "typst.unresolved_field",
                    "field": field,
                    "message": format!("field `{field}` was not available to the renderer"),
                    "severity": "error",
                })
            })
            .collect();
        let status = if unresolved_fields.is_empty() {
            "ok"
        } else {
            "failed_closed"
        };
        let report = serde_json::json!({
            "adapter": RENDERER_ID,
            "renderer_id": RENDERER_ID,
            "renderer_version": RENDERER_VERSION,
            "environment_id": ENVIRONMENT_ID,
            "artifact_fingerprint": artifact_fingerprint,
            "artifact_bytes": artifact_bytes,
            "output_hash": artifact_fingerprint,
            "status": status,
            "unresolved_fields": unresolved_fields,
            "diagnostics": diagnostics,
        });

        Ok(DocumentRenderResult {
            artifact: request.output.clone(),
            report,
        })
    }
}

/// Format a JSON number with two decimal places and locale-specific grouping.
///
/// Uses the exact JSON decimal representation, avoiding binary floating-point
/// conversion and its precision loss for financial values.
#[must_use]
pub fn format_number(value: &Value, locale: &str) -> Option<String> {
    let (negative, integer, fraction) = decimal_parts(value.as_number()?)?;
    let (group_separator, decimal_separator) = match locale {
        "nl" | "nl-NL" => ('.', ','),
        _ => (',', '.'),
    };
    let grouped = group_digits(&integer, group_separator);
    Some(format!(
        "{}{grouped}{decimal_separator}{fraction}",
        if negative { "-" } else { "" }
    ))
}

fn decimal_parts(number: &Number) -> Option<(bool, String, String)> {
    let raw = number.to_string();
    let (negative, unsigned) = raw
        .strip_prefix('-')
        .map_or((false, raw.as_str()), |value| (true, value));
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or(Some((unsigned, 0_i32)), |(value, exponent)| {
            Some((value, exponent.parse().ok()?))
        })?;
    let (integer, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if integer.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    let digits = format!("{integer}{fraction}");
    let decimal_position = i64::try_from(integer.len()).ok()? + i64::from(exponent);
    let integer_width = usize::try_from(decimal_position.max(1)).ok()?;
    let mut whole = String::with_capacity(integer_width);
    if decimal_position <= 0 {
        whole.push('0');
    } else {
        for index in 0..integer_width {
            whole.push(decimal_digit(&digits, i64::try_from(index).ok()?));
        }
    }
    let trimmed = whole.trim_start_matches('0');
    let whole = if trimmed.is_empty() { "0" } else { trimmed };

    let mut fixed = String::with_capacity(whole.len() + 2);
    fixed.push_str(whole);
    fixed.push(decimal_digit(&digits, decimal_position));
    fixed.push(decimal_digit(&digits, decimal_position + 1));
    if decimal_digit(&digits, decimal_position + 2) >= '5' {
        round_decimal_digits(&mut fixed);
    }
    let split = fixed.len().checked_sub(2)?;
    let fraction = fixed.split_off(split);
    let negative = negative
        && fixed
            .bytes()
            .chain(fraction.bytes())
            .any(|byte| byte != b'0');
    Some((negative, fixed, fraction))
}

fn decimal_digit(digits: &str, index: i64) -> char {
    usize::try_from(index)
        .ok()
        .and_then(|index| digits.as_bytes().get(index).copied())
        .map_or('0', char::from)
}

fn round_decimal_digits(digits: &mut String) {
    let mut bytes = digits.as_bytes().to_vec();
    for digit in bytes.iter_mut().rev() {
        if *digit == b'9' {
            *digit = b'0';
        } else {
            *digit += 1;
            *digits = String::from_utf8(bytes).expect("decimal digits are ASCII");
            return;
        }
    }
    bytes.insert(0, b'1');
    *digits = String::from_utf8(bytes).expect("decimal digits are ASCII");
}

/// Format an ISO `YYYY-MM-DD` date without timezone or host-locale access.
///
/// Returns `None` when the input is not a structurally valid calendar date.
#[must_use]
pub fn format_date(value: &str, locale: &str) -> Option<String> {
    if value.len() != 10 {
        return None;
    }
    let date = value;
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return None;
    }
    let year = &date[..4];
    let month = &date[5..7];
    let day = &date[8..10];
    let month_number: u8 = month.parse().ok()?;
    let day_number: u8 = day.parse().ok()?;
    if !(1..=12).contains(&month_number)
        || !(1..=days_in_month(year, month_number)?).contains(&day_number)
    {
        return None;
    }
    Some(match locale {
        "nl" | "nl-NL" => format!("{day}-{month}-{year}"),
        "en-US" => format!("{month}/{day}/{year}"),
        _ => date.to_owned(),
    })
}

fn days_in_month(year: &str, month: u8) -> Option<u8> {
    let year: u32 = year.parse().ok()?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    Some(match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    })
}

fn group_digits(integer: &str, separator: char) -> String {
    let mut grouped = String::with_capacity(integer.len() + integer.len() / 3);
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(3) {
            grouped.push(separator);
        }
        grouped.push(digit);
    }
    grouped
}

fn render(template: &str, data: &Value, unresolved: &mut BTreeSet<String>) -> String {
    let with_charts = expand_charts(template, data, unresolved);
    let with_rows = expand_each(&with_charts, data, unresolved);
    replace_fields(&with_rows, data, unresolved)
}

fn expand_each(template: &str, data: &Value, unresolved: &mut BTreeSet<String>) -> String {
    let mut output = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{#each ") {
        output.push_str(&rest[..start]);
        let after_open = &rest[start + "{{#each ".len()..];
        let Some(name_end) = after_open.find("}}") else {
            output.push_str(&rest[start..]);
            return output;
        };
        let name = after_open[..name_end].trim();
        let body_start = &after_open[name_end + 2..];
        let Some(close) = body_start.find("{{/each}}") else {
            output.push_str(&rest[start..]);
            return output;
        };
        let body = &body_start[..close];
        match lookup(data, name) {
            Some(Value::Array(items)) => {
                for item in items {
                    output.push_str(&replace_fields(body, item, unresolved));
                }
            }
            _ => {
                unresolved.insert(name.to_owned());
            }
        }
        rest = &body_start[close + "{{/each}}".len()..];
    }
    output.push_str(rest);
    output
}

fn expand_charts(template: &str, data: &Value, unresolved: &mut BTreeSet<String>) -> String {
    let mut output = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{#chart ") {
        output.push_str(&rest[..start]);
        let after_open = &rest[start + "{{#chart ".len()..];
        let Some(end) = after_open.find("}}") else {
            output.push_str(&rest[start..]);
            return output;
        };
        let directive = after_open[..end].trim();
        let mut tokens = directive.split_ascii_whitespace();
        let name = tokens.next().unwrap_or_default();
        let locale = tokens
            .find_map(|token| token.strip_prefix("locale="))
            .unwrap_or("en-US");
        match lookup(data, name) {
            Some(Value::Array(rows)) if !rows.is_empty() && rows.iter().all(Value::is_object) => {
                output.push_str(&render_chart(name, rows, locale, unresolved));
            }
            _ => {
                unresolved.insert(name.to_owned());
            }
        }
        rest = &after_open[end + 2..];
    }
    output.push_str(rest);
    output
}

fn render_chart(
    name: &str,
    rows: &[Value],
    locale: &str,
    unresolved: &mut BTreeSet<String>,
) -> String {
    let columns: BTreeSet<&str> = rows
        .iter()
        .filter_map(Value::as_object)
        .flat_map(|row| row.keys().map(String::as_str))
        .collect();
    if columns.is_empty() {
        unresolved.insert(name.to_owned());
        return String::new();
    }
    let mut output = format!(
        "// templiqx-native-chart: {}\n#table(\n  columns: ({}),\n",
        name,
        "1fr, ".repeat(columns.len())
    );
    for column in &columns {
        writeln!(output, "  [*{}*],", typst_text(column)).expect("writing to a String cannot fail");
    }
    for (row_index, row) in rows.iter().filter_map(Value::as_object).enumerate() {
        for column in &columns {
            let cell = row.get(*column).and_then(|value| {
                if value.is_number() {
                    format_number(value, locale)
                } else {
                    scalar(value)
                }
            });
            let cell = cell.unwrap_or_else(|| {
                unresolved.insert(format!("{name}[{row_index}].{column}"));
                String::new()
            });
            writeln!(output, "  [{}],", typst_text(&cell))
                .expect("writing to a String cannot fail");
        }
    }
    output.push_str(")\n");
    output
}

fn replace_fields(template: &str, context: &Value, unresolved: &mut BTreeSet<String>) -> String {
    let mut output = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        if rest[start..].starts_with("{{#") || rest[start..].starts_with("{{/") {
            output.push_str(&rest[..start + 2]);
            rest = &rest[start + 2..];
            continue;
        }
        output.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        let Some(end) = after_open.find("}}") else {
            output.push_str(&rest[start..]);
            return output;
        };
        let expression = after_open[..end].trim();
        let (field, formatter) = expression
            .split_once('|')
            .map_or((expression, None), |(field, formatter)| {
                (field.trim(), Some(formatter.trim()))
            });
        match lookup(context, field).and_then(|value| format_value(value, formatter)) {
            Some(value) => output.push_str(&typst_text(&value)),
            None => {
                unresolved.insert(field.to_owned());
            }
        }
        rest = &after_open[end + 2..];
    }
    output.push_str(rest);
    output
}

fn lookup<'a>(context: &'a Value, field: &str) -> Option<&'a Value> {
    if matches!(field, "this" | ".") {
        return Some(context);
    }
    field
        .split('.')
        .try_fold(context, |value, segment| value.get(segment))
}

fn format_value(value: &Value, formatter: Option<&str>) -> Option<String> {
    if value.is_null() {
        return None;
    }
    match formatter {
        None => scalar(value),
        Some(formatter) => {
            let (kind, locale) = formatter.split_once(':')?;
            match kind.trim() {
                "number" => format_number(value, locale.trim()),
                "date" => format_date(value.as_str()?, locale.trim()),
                _ => None,
            }
        }
    }
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(number_text(value)),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn number_text(value: &Number) -> String {
    value.to_string()
}

fn typst_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            _ => output.push(character),
        }
    }
    format!("#text(\"{output}\")")
}

const SHA256_INITIAL: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

const SHA256_ROUND: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

fn sha256_hex(input: &[u8]) -> String {
    let bit_length = u64::try_from(input.len())
        .unwrap_or(u64::MAX)
        .wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_length.to_be_bytes());

    let mut hash = SHA256_INITIAL;
    for chunk in padded.chunks_exact(64) {
        sha256_compress(&mut hash, chunk);
    }
    let mut output = String::with_capacity(64);
    for word in hash {
        write!(output, "{word:08x}").expect("writing to a String cannot fail");
    }
    output
}

#[allow(clippy::many_single_char_names)]
fn sha256_compress(hash: &mut [u32; 8], chunk: &[u8]) {
    let mut words = [0_u32; 64];
    for (index, bytes) in chunk.chunks_exact(4).enumerate() {
        words[index] = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    for index in 16..64 {
        let s0 = words[index - 15].rotate_right(7)
            ^ words[index - 15].rotate_right(18)
            ^ (words[index - 15] >> 3);
        let s1 = words[index - 2].rotate_right(17)
            ^ words[index - 2].rotate_right(19)
            ^ (words[index - 2] >> 10);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *hash;
    for index in 0..64 {
        let sum1 = h
            .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
            .wrapping_add((e & f) ^ (!e & g))
            .wrapping_add(SHA256_ROUND[index])
            .wrapping_add(words[index]);
        let sum0 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
            .wrapping_add((a & b) ^ (a & c) ^ (b & c));
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(sum1);
        d = c;
        c = b;
        b = a;
        a = sum1.wrapping_add(sum0);
    }
    for (slot, value) in hash.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn formats_dutch_numbers_and_dates() {
        assert_eq!(
            format_number(&json!(45_750), "nl-NL").as_deref(),
            Some("45.750,00")
        );
        assert_eq!(
            format_date("2026-07-16", "nl-NL").as_deref(),
            Some("16-07-2026")
        );
        assert!(format_date("2026-02-30", "nl-NL").is_none());
        assert!(format_date("2026-07-16T12:00:00Z", "nl-NL").is_none());
        assert_eq!(
            format_number(&json!(9_007_199_254_740_993_u64), "nl-NL").as_deref(),
            Some("9.007.199.254.740.993,00")
        );
        assert_eq!(
            format_number(&json!(1.005), "en-US").as_deref(),
            Some("1.01")
        );
        let small: Value = serde_json::from_str("1e-7").expect("valid JSON number");
        assert_eq!(format_number(&small, "en-US").as_deref(), Some("0.00"));
    }

    #[test]
    fn interpolated_values_cannot_activate_typst_markup() {
        let mut unresolved = BTreeSet::new();
        let output = render(
            "Value: [{{ value }}]",
            &json!({"value": "#evil[*x*] \"quoted\"\\path\n= heading"}),
            &mut unresolved,
        );
        assert_eq!(
            output,
            "Value: [#text(\"#evil[*x*] \\\"quoted\\\"\\\\path\\n= heading\")]"
        );
        assert!(unresolved.is_empty());
    }

    #[test]
    fn sha256_matches_standard_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
