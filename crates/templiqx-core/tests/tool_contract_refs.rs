//! U2 (plan 001): compile-time resolution of shared tool-contract references.
//! References inline the shared schema only when the pinned fingerprint matches;
//! otherwise they fail closed with a stable diagnostic.

use std::collections::BTreeMap;

use serde_json::json;
use templiqx_contracts::{Contract, ExtensionSpec, ToolContractRef};
use templiqx_core::resolve_tool_contract_refs;

fn contract_with_ref(reference: serde_json::Value) -> Contract {
    let source = r#"
api_version: templiqx/v1alpha1
id: refs
version: 0.1.0
messages:
  - role: user
    content:
      - kind: text
        value: hi
output_schema: { type: object }
"#;
    let mut contract = templiqx_core::parse_contract(source, None).expect("parses");
    contract.extensions.insert(
        "vendor.search".to_owned(),
        ExtensionSpec {
            capability: "tools".to_owned(),
            schema: reference,
            value: json!({ "query": "x" }),
        },
    );
    contract
}

fn table() -> BTreeMap<String, ToolContractRef> {
    BTreeMap::from([(
        "search_customers".to_owned(),
        ToolContractRef {
            fingerprint: "sha256:abc".to_owned(),
            schema: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            }),
        },
    )])
}

#[test]
fn resolves_and_inlines_matching_reference() {
    let mut c = contract_with_ref(json!({
        "$ref": "tool_contract:search_customers",
        "fingerprint": "sha256:abc"
    }));
    let diags = resolve_tool_contract_refs(&mut c, &table());
    assert!(diags.is_empty(), "{diags:?}");
    // Schema is now the inlined tool-contract schema, not the $ref stub.
    assert_eq!(
        c.extensions["vendor.search"].schema,
        table()["search_customers"].schema
    );
}

#[test]
fn wrong_fingerprint_fails_closed() {
    let mut c = contract_with_ref(json!({
        "$ref": "tool_contract:search_customers",
        "fingerprint": "sha256:STALE"
    }));
    let diags = resolve_tool_contract_refs(&mut c, &table());
    assert!(
        diags
            .iter()
            .any(|d| d.code == "TQX_TOOL_CONTRACT_REF_UNRESOLVED")
    );
}

#[test]
fn unknown_name_fails_closed() {
    let mut c = contract_with_ref(json!({
        "$ref": "tool_contract:missing",
        "fingerprint": "sha256:abc"
    }));
    let diags = resolve_tool_contract_refs(&mut c, &table());
    assert!(
        diags
            .iter()
            .any(|d| d.code == "TQX_TOOL_CONTRACT_REF_UNRESOLVED")
    );
}

#[test]
fn missing_fingerprint_pin_fails_closed() {
    let mut c = contract_with_ref(json!({ "$ref": "tool_contract:search_customers" }));
    let diags = resolve_tool_contract_refs(&mut c, &table());
    assert!(
        diags
            .iter()
            .any(|d| d.code == "TQX_TOOL_CONTRACT_REF_UNRESOLVED")
    );
}

#[test]
fn ordinary_schema_untouched() {
    let plain = json!({ "type": "object" });
    let mut c = contract_with_ref(plain.clone());
    let diags = resolve_tool_contract_refs(&mut c, &table());
    assert!(diags.is_empty());
    assert_eq!(c.extensions["vendor.search"].schema, plain);
}
