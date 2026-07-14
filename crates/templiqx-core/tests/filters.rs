//! U5 (template-engine parity plan 001): bounded locale-aware format filters.
//! Filters extend formatting without arbitrary code and read `context.locale`.

use std::collections::BTreeMap;

use serde_json::Value;
use templiqx_contracts::{RenderRequest, Severity};
use templiqx_core::{compile, parse_contract};

fn contract(filter: &str) -> templiqx_contracts::Contract {
    let source = format!(
        r#"
api_version: templiqx/v1alpha1
id: filter-fixture
version: 0.1.0
inputs:
  amount:
    schema: {{ type: number }}
  day:
    schema: {{ type: string }}
context:
  locale:
    schema: {{ type: string }}
messages:
  - role: user
    content:
      - kind: interpolate
        expression: {{ kind: ref, path: inputs.{field} }}
        filters: [{filter}]
output_schema: {{ type: object }}
"#,
        field = if filter == "format_date" {
            "day"
        } else {
            "amount"
        },
        filter = filter,
    );
    parse_contract(&source, None).expect("valid fixture parses")
}

fn render(
    filter: &str,
    locale: &str,
    value: serde_json::Value,
) -> Result<String, Vec<templiqx_contracts::Diagnostic>> {
    let c = contract(filter);
    let field = if filter == "format_date" {
        "day"
    } else {
        "amount"
    };
    let mut inputs = BTreeMap::new();
    inputs.insert(field.to_owned(), value);
    let mut context = BTreeMap::new();
    context.insert("locale".to_owned(), serde_json::json!(locale));
    let request = RenderRequest { inputs, context };
    compile(&c, &request, &[]).map(|i| i.messages[0].content.clone())
}

#[test]
fn format_date_matches_nl_nl_fixture() {
    assert_eq!(
        render("format_date", "nl-NL", serde_json::json!("2026-07-12")).unwrap(),
        "12-07-2026"
    );
}

#[test]
fn format_date_families_differ() {
    let d = serde_json::json!("2026-07-12");
    assert_eq!(
        render("format_date", "de-DE", d.clone()).unwrap(),
        "12.07.2026"
    );
    assert_eq!(
        render("format_date", "en-US", d.clone()).unwrap(),
        "07/12/2026"
    );
    assert_eq!(render("format_date", "", d).unwrap(), "2026-07-12");
}

#[test]
fn format_number_groups_per_locale() {
    let n = serde_json::json!(1234567.5);
    assert_eq!(
        render("format_number", "nl-NL", n.clone()).unwrap(),
        "1.234.567,50"
    );
    assert_eq!(render("format_number", "en-US", n).unwrap(), "1,234,567.50");
}

#[test]
fn invalid_date_input_fails_with_stable_diagnostic() {
    let err = render("format_date", "nl-NL", serde_json::json!("not-a-date")).unwrap_err();
    assert!(
        err.iter()
            .any(|d| d.code == "TQX_FILTER_INPUT" && d.severity == Severity::Error)
    );
}

#[test]
fn format_number_rejects_non_numeric_input() {
    // Typed inputs (schema: number) reject a string at value validation, before
    // the filter runs — defense in depth. Either layer failing is acceptable.
    let err = render("format_number", "en-US", serde_json::json!("abc")).unwrap_err();
    assert!(err.iter().any(|d| d.severity == Severity::Error));
}

#[test]
fn format_currency_prefixes_locale_symbol() {
    assert_eq!(
        render("format_currency", "nl-NL", serde_json::json!(42.5)).unwrap(),
        "€42,50"
    );
    assert_eq!(
        render("format_currency", "en-US", serde_json::json!(42.5)).unwrap(),
        "$42.50"
    );
}

fn render_with_context(
    filter: &str,
    context: BTreeMap<String, Value>,
    value: serde_json::Value,
) -> Result<String, Vec<templiqx_contracts::Diagnostic>> {
    let field = "key";
    let source = format!(
        r#"
api_version: templiqx/v1alpha1
id: translate-fixture
version: 0.1.0
inputs:
  key:
    schema: {{ type: string }}
context:
  locale:
    schema: {{ type: string }}
messages:
  - role: user
    content:
      - kind: interpolate
        expression: {{ kind: ref, path: inputs.key }}
        filters: [{filter}]
output_schema: {{ type: object }}
"#
    );
    let c = parse_contract(&source, None).expect("valid fixture parses");
    let mut inputs = BTreeMap::new();
    inputs.insert(field.to_owned(), value);
    let request = RenderRequest { inputs, context };
    compile(&c, &request, &[]).map(|i| i.messages[0].content.clone())
}

#[test]
fn translate_resolves_locale_with_fallback() {
    let mut context = BTreeMap::new();
    context.insert("locale".into(), serde_json::json!("nl"));
    context.insert(
        "_templiqx_translations".into(),
        serde_json::json!({
            "en": {"greeting": "Hello"},
            "nl": {"greeting": "Hallo"}
        }),
    );
    assert_eq!(
        render_with_context("translate", context, serde_json::json!("greeting")).unwrap(),
        "Hallo"
    );
}

#[test]
fn translate_missing_key_fails_closed() {
    let mut context = BTreeMap::new();
    context.insert("locale".into(), serde_json::json!("en"));
    context.insert(
        "_templiqx_translations".into(),
        serde_json::json!({"en": {"greeting": "Hello"}}),
    );
    let err = render_with_context("translate", context, serde_json::json!("missing")).unwrap_err();
    assert!(
        err.iter()
            .any(|d| d.code == "TQX_TRANSLATION_KEY" && d.severity == Severity::Error)
    );
}
