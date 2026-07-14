//! Deterministic, provider-neutral contract validation, rendering and compilation.

use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use templiqx_contracts::{
    CompiledInteraction, CompiledMessage, Contract, Diagnostic, Expr, Filter, MessageTemplate,
    Node, RenderRequest, Severity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Any,
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

pub fn parse_contract(source: &str, file: Option<&str>) -> Result<Contract, Vec<Diagnostic>> {
    serde_yaml_ng::from_str(source).map_err(|e| {
        vec![Diagnostic {
            code: "TQX_PARSE_YAML".into(),
            severity: Severity::Error,
            message: e.to_string(),
            file: file.map(str::to_owned),
            json_pointer: None,
            span: e.location().map(|l| templiqx_contracts::SourceSpan {
                line: l.line() as u32,
                column: l.column() as u32,
                end_line: l.line() as u32,
                end_column: l.column() as u32,
            }),
            help: Some("Use the strict Templiqx YAML schema; unknown fields are rejected.".into()),
        }]
    })
}

/// U2 (plan 001): resolve `tool_contract:<name>` references in contract
/// extensions against a package's shared tool-contract table. Each reference
/// must name a known tool contract and pin its exact fingerprint; on success the
/// extension's `schema` is replaced with the resolved schema so downstream
/// validation and compilation see a fully-inlined, bounded schema. Fails closed
/// with `TQX_TOOL_CONTRACT_REF_UNRESOLVED` — the same posture as package signing.
#[must_use]
pub fn resolve_tool_contract_refs(
    contract: &mut Contract,
    tool_contracts: &std::collections::BTreeMap<String, templiqx_contracts::ToolContractRef>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (key, extension) in contract.extensions.iter_mut() {
        let Some(object) = extension.schema.as_object() else {
            continue;
        };
        let Some(reference) = object.get("$ref").and_then(Value::as_str) else {
            continue;
        };
        let Some(name) = reference.strip_prefix("tool_contract:") else {
            continue;
        };
        let pointer = format!("/extensions/{key}/schema");
        let pinned = object.get("fingerprint").and_then(Value::as_str);
        match tool_contracts.get(name) {
            None => diagnostics.push(Diagnostic::error(
                "TQX_TOOL_CONTRACT_REF_UNRESOLVED",
                format!("extension '{key}' references unknown tool_contract '{name}'"),
                pointer,
            )),
            Some(resolved) => match pinned {
                Some(fingerprint) if fingerprint == resolved.fingerprint => {
                    extension.schema = resolved.schema.clone();
                }
                Some(fingerprint) => diagnostics.push(Diagnostic::error(
                    "TQX_TOOL_CONTRACT_REF_UNRESOLVED",
                    format!(
                        "extension '{key}' pins fingerprint '{fingerprint}' but tool_contract '{name}' has '{}'",
                        resolved.fingerprint
                    ),
                    pointer,
                )),
                None => diagnostics.push(Diagnostic::error(
                    "TQX_TOOL_CONTRACT_REF_UNRESOLVED",
                    format!("extension '{key}' references tool_contract '{name}' without a fingerprint pin"),
                    pointer,
                )),
            },
        }
    }
    diagnostics
}

pub fn validate_contract(contract: &Contract) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if contract.api_version != templiqx_contracts::API_VERSION {
        out.push(Diagnostic::error(
            "TQX_API_VERSION",
            "unsupported api_version",
            "/api_version",
        ));
    }
    if !valid_id(&contract.id) {
        out.push(Diagnostic::error(
            "TQX_INVALID_ID",
            "id must contain only ASCII letters, digits, '.', '_' or '-'",
            "/id",
        ));
    }
    if semver::Version::parse(&contract.version).is_err() {
        out.push(Diagnostic::error(
            "TQX_VERSION_INVALID",
            "contract version must be semantic versioning",
            "/version",
        ));
    }
    if contract.messages.is_empty() {
        out.push(Diagnostic::error(
            "TQX_MESSAGES_EMPTY",
            "at least one message is required",
            "/messages",
        ));
    }
    if let Err(e) = jsonschema::validator_for(&contract.output_schema) {
        out.push(Diagnostic::error(
            "TQX_OUTPUT_SCHEMA_INVALID",
            e.to_string(),
            "/output_schema",
        ));
    }
    validate_bounded_schema(&contract.output_schema, "/output_schema", &mut out);
    if contract.output_schema == Value::Bool(false) {
        out.push(Diagnostic::error(
            "TQX_OUTPUT_SCHEMA_UNSATISFIABLE",
            "output schema rejects every possible value",
            "/output_schema",
        ));
    }
    for (scope, fields) in [("inputs", &contract.inputs), ("context", &contract.context)] {
        for (name, field) in fields {
            let pointer = format!("/{scope}/{name}/schema");
            if let Err(e) = jsonschema::validator_for(&field.schema) {
                out.push(Diagnostic::error(
                    "TQX_FIELD_SCHEMA_INVALID",
                    format!("invalid schema for '{name}': {e}"),
                    &pointer,
                ));
            }
            validate_bounded_schema(&field.schema, &pointer, &mut out);
        }
    }
    let roots: BTreeSet<String> = contract
        .inputs
        .keys()
        .map(|k| format!("inputs.{k}"))
        .chain(contract.context.keys().map(|k| format!("context.{k}")))
        .collect();
    for (i, message) in contract.messages.iter().enumerate() {
        validate_nodes(
            &message.content,
            contract,
            &roots,
            &mut BTreeMap::new(),
            &format!("/messages/{i}/content"),
            &mut out,
        );
    }
    for (name, component) in &contract.components {
        if let Some(parameters) = component.parameters() {
            for (parameter, field) in parameters {
                let pointer = format!("/components/{name}/parameters/{parameter}/schema");
                if let Err(error) = jsonschema::validator_for(&field.schema) {
                    out.push(Diagnostic::error(
                        "TQX_COMPONENT_SCHEMA_INVALID",
                        format!("invalid schema for component parameter '{parameter}': {error}"),
                        &pointer,
                    ));
                }
                validate_bounded_schema(&field.schema, &pointer, &mut out);
            }
        }
        let mut locals: BTreeMap<String, Value> = match component.parameters() {
            Some(parameters) => parameters
                .iter()
                .map(|(name, field)| (name.clone(), field.schema.clone()))
                .collect(),
            None => component_arguments(component.content())
                .into_iter()
                .map(|name| (name, serde_json::json!({"type": "string"})))
                .collect(),
        };
        validate_nodes(
            component.content(),
            contract,
            &roots,
            &mut locals,
            &format!("/components/{name}"),
            &mut out,
        );
    }
    validate_component_cycles(contract, &mut out);
    for (key, extension) in &contract.extensions {
        if !key.contains('.') {
            out.push(Diagnostic::error(
                "TQX_EXTENSION_NAMESPACE",
                format!("extension '{key}' must use a namespaced key such as vendor.option"),
                format!("/extensions/{key}"),
            ));
        }
        if extension.capability.is_empty()
            || !extension.capability.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':')
            })
        {
            out.push(Diagnostic::error(
                "TQX_EXTENSION_CAPABILITY",
                format!("extension '{key}' declares an invalid capability"),
                format!("/extensions/{key}/capability"),
            ));
        }
        let schema_pointer = format!("/extensions/{key}/schema");
        if let Err(error) = jsonschema::validator_for(&extension.schema) {
            out.push(Diagnostic::error(
                "TQX_EXTENSION_SCHEMA_INVALID",
                format!("invalid schema for extension '{key}': {error}"),
                &schema_pointer,
            ));
        }
        validate_bounded_schema(&extension.schema, &schema_pointer, &mut out);
        validate_instance(
            &extension.schema,
            &extension.value,
            &format!("/extensions/{key}/value"),
            "TQX_EXTENSION_VALUE",
            &mut out,
        );
    }
    out
}

fn validate_component_cycles(contract: &Contract, out: &mut Vec<Diagnostic>) {
    fn references(nodes: &[Node], found: &mut BTreeSet<String>) {
        for node in nodes {
            match node {
                Node::Component { name, .. } => {
                    found.insert(name.clone());
                }
                Node::When {
                    then, otherwise, ..
                } => {
                    references(then, found);
                    references(otherwise, found);
                }
                Node::ForEach { body, .. } => references(body, found),
                Node::Text { .. } | Node::Interpolate { .. } | Node::Include { .. } => {}
            }
        }
    }
    fn visit(
        name: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
    ) -> Option<String> {
        if visiting.contains(name) {
            return Some(name.to_owned());
        }
        if visited.contains(name) {
            return None;
        }
        visiting.insert(name.to_owned());
        if let Some(next) = graph.get(name) {
            for child in next {
                if let Some(cycle) = visit(child, graph, visiting, visited) {
                    return Some(cycle);
                }
            }
        }
        visiting.remove(name);
        visited.insert(name.to_owned());
        None
    }
    let graph: BTreeMap<_, _> = contract
        .components
        .iter()
        .map(|(name, component)| {
            let mut found = BTreeSet::new();
            references(component.content(), &mut found);
            (name.clone(), found)
        })
        .collect();
    let mut visited = BTreeSet::new();
    for name in graph.keys() {
        if let Some(cycle) = visit(name, &graph, &mut BTreeSet::new(), &mut visited) {
            out.push(Diagnostic::error(
                "TQX_COMPONENT_CYCLE",
                format!("component cycle includes '{cycle}'"),
                format!("/components/{cycle}"),
            ));
            break;
        }
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn validate_nodes(
    nodes: &[Node],
    contract: &Contract,
    _roots: &BTreeSet<String>,
    locals: &mut BTreeMap<String, Value>,
    ptr: &str,
    out: &mut Vec<Diagnostic>,
) {
    for (i, node) in nodes.iter().enumerate() {
        let p = format!("{ptr}/{i}");
        match node {
            Node::Text { .. } => {}
            Node::Include { .. } => out.push(Diagnostic::error(
                "TQX_INCLUDE_UNEXPANDED",
                "include node reached validation unexpanded (composition layer must expand includes)",
                &p,
            )),
            Node::Interpolate {
                expression,
                filters,
            } => {
                validate_expr(
                    expression,
                    contract,
                    locals,
                    &format!("{p}/expression"),
                    out,
                );
                let kind = infer_kind(expression, contract, locals);
                let has_json = filters.iter().any(|filter| matches!(filter, Filter::Json));
                if matches!(kind, Kind::Array | Kind::Object | Kind::Null) && !has_json {
                    out.push(Diagnostic::error(
                        "TQX_INTERPOLATION_TYPE",
                        "arrays, objects and null require the json filter",
                        format!("{p}/filters"),
                    ));
                }
                if matches!(kind, Kind::Any) {
                    out.push(Diagnostic::error(
                        "TQX_INTERPOLATION_TYPE",
                        "interpolation expression must have a statically known scalar type or use a typed json value",
                        format!("{p}/expression"),
                    ));
                }
                if filters
                    .iter()
                    .any(|filter| matches!(filter, Filter::Trim | Filter::Lower | Filter::Upper))
                    && !matches!(kind, Kind::String)
                {
                    out.push(Diagnostic::error(
                        "TQX_FILTER_TYPE",
                        "trim, lower and upper filters require a string expression",
                        format!("{p}/filters"),
                    ));
                }
            }
            Node::When {
                condition,
                then,
                otherwise,
            } => {
                validate_expr(condition, contract, locals, &format!("{p}/condition"), out);
                if infer_kind(condition, contract, locals) != Kind::Boolean {
                    out.push(Diagnostic::error(
                        "TQX_CONDITION_TYPE",
                        "when condition must be boolean",
                        format!("{p}/condition"),
                    ));
                }
                validate_nodes(then, contract, _roots, locals, &format!("{p}/then"), out);
                validate_nodes(
                    otherwise,
                    contract,
                    _roots,
                    locals,
                    &format!("{p}/else"),
                    out,
                );
            }
            Node::ForEach {
                collection,
                item,
                body,
                ..
            } => {
                validate_expr(
                    collection,
                    contract,
                    locals,
                    &format!("{p}/collection"),
                    out,
                );
                if infer_kind(collection, contract, locals) != Kind::Array {
                    out.push(Diagnostic::error(
                        "TQX_FOR_EACH_TYPE",
                        "for_each collection must be an array",
                        format!("{p}/collection"),
                    ));
                }
                if !valid_id(item) {
                    out.push(Diagnostic::error(
                        "TQX_LOCAL_INVALID",
                        "iteration item is not a valid identifier",
                        format!("{p}/item"),
                    ));
                } else {
                    let item_schema = infer_schema(collection, contract, locals)
                        .as_ref()
                        .and_then(|schema| schema.get("items"))
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));
                    locals.insert(item.clone(), item_schema);
                    validate_nodes(body, contract, _roots, locals, &format!("{p}/body"), out);
                    locals.remove(item);
                }
            }
            Node::Component { name, with } => {
                let Some(component) = contract.components.get(name) else {
                    out.push(Diagnostic::error(
                        "TQX_COMPONENT_MISSING",
                        format!("unknown component '{name}'"),
                        format!("{p}/name"),
                    ));
                    continue;
                };
                let declared: BTreeMap<String, (Kind, bool)> =
                    if let Some(parameters) = component.parameters() {
                        parameters
                            .iter()
                            .map(|(name, field)| {
                                (name.clone(), (schema_kind(&field.schema), field.required))
                            })
                            .collect()
                    } else {
                        component_arguments(component.content())
                            .into_iter()
                            .map(|name| (name, (Kind::String, true)))
                            .collect()
                    };
                for (parameter, (_, required)) in &declared {
                    if *required && !with.contains_key(parameter) {
                        out.push(Diagnostic::error(
                            "TQX_COMPONENT_ARGUMENT_MISSING",
                            format!("component '{name}' requires argument '{parameter}'"),
                            format!("{p}/with/{parameter}"),
                        ));
                    }
                }
                for (argument, expression) in with {
                    validate_expr(
                        expression,
                        contract,
                        locals,
                        &format!("{p}/with/{argument}"),
                        out,
                    );
                    match declared.get(argument) {
                        None => out.push(Diagnostic::error(
                            "TQX_COMPONENT_ARGUMENT_UNKNOWN",
                            format!("component '{name}' has no parameter '{argument}'"),
                            format!("{p}/with/{argument}"),
                        )),
                        Some((expected, _)) => {
                            let actual = infer_kind(expression, contract, locals);
                            if *expected != Kind::Any && actual != Kind::Any && *expected != actual
                            {
                                out.push(Diagnostic::error(
                                    "TQX_COMPONENT_ARGUMENT_TYPE",
                                    format!("component '{name}' parameter '{argument}' expects {expected:?}, found {actual:?}"),
                                    format!("{p}/with/{argument}"),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn component_arguments(nodes: &[Node]) -> BTreeSet<String> {
    fn walk(nodes: &[Node], locals: &mut BTreeSet<String>, found: &mut BTreeSet<String>) {
        for node in nodes {
            match node {
                Node::Interpolate { expression, .. } => expression_roots(expression, locals, found),
                Node::When {
                    condition,
                    then,
                    otherwise,
                } => {
                    expression_roots(condition, locals, found);
                    walk(then, locals, found);
                    walk(otherwise, locals, found);
                }
                Node::ForEach {
                    collection,
                    item,
                    body,
                    ..
                } => {
                    expression_roots(collection, locals, found);
                    locals.insert(item.clone());
                    walk(body, locals, found);
                    locals.remove(item);
                }
                Node::Component { with, .. } => {
                    for expression in with.values() {
                        expression_roots(expression, locals, found);
                    }
                }
                Node::Text { .. } | Node::Include { .. } => {}
            }
        }
    }
    fn expression_roots(
        expression: &Expr,
        locals: &BTreeSet<String>,
        found: &mut BTreeSet<String>,
    ) {
        match expression {
            Expr::Ref { path } => {
                let mut parts = path.split('.');
                let root = parts.next().unwrap_or_default();
                if !matches!(root, "inputs" | "context") && !locals.contains(root) {
                    found.insert(root.to_owned());
                }
            }
            Expr::Equals { left, right } => {
                expression_roots(left, locals, found);
                expression_roots(right, locals, found);
            }
            Expr::Not { value } => expression_roots(value, locals, found),
            Expr::And { values } | Expr::Or { values } => {
                for value in values {
                    expression_roots(value, locals, found);
                }
            }
            Expr::Literal { .. } => {}
        }
    }
    let mut found = BTreeSet::new();
    walk(nodes, &mut BTreeSet::new(), &mut found);
    found
}

fn validate_expr(
    expr: &Expr,
    contract: &Contract,
    locals: &BTreeMap<String, Value>,
    ptr: &str,
    out: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Ref { path } => {
            if infer_schema_for_path(path, contract, locals).is_err() {
                out.push(Diagnostic::error(
                    "TQX_REF_UNKNOWN",
                    format!("unknown or schema-incompatible reference path '{path}'"),
                    ptr,
                ));
            }
        }
        Expr::Literal { .. } => {}
        Expr::Equals { left, right } => {
            validate_expr(left, contract, locals, &format!("{ptr}/left"), out);
            validate_expr(right, contract, locals, &format!("{ptr}/right"), out);
        }
        Expr::Not { value } => {
            validate_expr(value, contract, locals, &format!("{ptr}/value"), out);
            require_boolean(value, contract, locals, &format!("{ptr}/value"), out);
        }
        Expr::And { values } | Expr::Or { values } => {
            if values.is_empty() {
                out.push(Diagnostic::error(
                    "TQX_BOOLEAN_EMPTY",
                    "and/or requires at least one boolean operand",
                    format!("{ptr}/values"),
                ));
            }
            for (index, value) in values.iter().enumerate() {
                validate_expr(
                    value,
                    contract,
                    locals,
                    &format!("{ptr}/values/{index}"),
                    out,
                );
                require_boolean(
                    value,
                    contract,
                    locals,
                    &format!("{ptr}/values/{index}"),
                    out,
                );
            }
        }
    }
}

fn require_boolean(
    expr: &Expr,
    contract: &Contract,
    locals: &BTreeMap<String, Value>,
    ptr: &str,
    out: &mut Vec<Diagnostic>,
) {
    if infer_kind(expr, contract, locals) != Kind::Boolean {
        out.push(Diagnostic::error(
            "TQX_BOOLEAN_TYPE",
            "boolean operator operands must be boolean",
            ptr,
        ));
    }
}

fn infer_kind(expr: &Expr, contract: &Contract, locals: &BTreeMap<String, Value>) -> Kind {
    match expr {
        Expr::Ref { path } => infer_schema_for_path(path, contract, locals)
            .map_or(Kind::Any, |schema| schema_kind(&schema)),
        Expr::Literal { value } => value_kind(value),
        Expr::Equals { .. } | Expr::Not { .. } | Expr::And { .. } | Expr::Or { .. } => {
            Kind::Boolean
        }
    }
}

fn infer_schema(
    expr: &Expr,
    contract: &Contract,
    locals: &BTreeMap<String, Value>,
) -> Option<Value> {
    match expr {
        Expr::Ref { path } => infer_schema_for_path(path, contract, locals).ok(),
        Expr::Literal { value } => Some(serde_json::json!({"type": kind_name(value_kind(value))})),
        Expr::Equals { .. } | Expr::Not { .. } | Expr::And { .. } | Expr::Or { .. } => {
            Some(serde_json::json!({"type": "boolean"}))
        }
    }
}

fn infer_schema_for_path(
    path: &str,
    contract: &Contract,
    locals: &BTreeMap<String, Value>,
) -> Result<Value, ()> {
    let parts: Vec<_> = path.split('.').collect();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return Err(());
    }
    let (mut schema, remaining) = match parts[0] {
        "inputs" | "context" => {
            let name = *parts.get(1).ok_or(())?;
            let fields = if parts[0] == "inputs" {
                &contract.inputs
            } else {
                &contract.context
            };
            (fields.get(name).ok_or(())?.schema.clone(), &parts[2..])
        }
        local => (locals.get(local).ok_or(())?.clone(), &parts[1..]),
    };
    for part in remaining {
        schema = match schema_kind(&schema) {
            Kind::Object => schema
                .get("properties")
                .and_then(Value::as_object)
                .and_then(|properties| properties.get(*part))
                .cloned()
                .ok_or(())?,
            Kind::Array if part.parse::<usize>().is_ok() => {
                schema.get("items").cloned().ok_or(())?
            }
            _ => return Err(()),
        };
    }
    Ok(schema)
}

fn schema_kind(schema: &Value) -> Kind {
    match schema.get("type").and_then(Value::as_str) {
        Some("null") => Kind::Null,
        Some("boolean") => Kind::Boolean,
        Some("integer" | "number") => Kind::Number,
        Some("string") => Kind::String,
        Some("array") => Kind::Array,
        Some("object") => Kind::Object,
        _ => Kind::Any,
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Null => "null",
        Kind::Boolean => "boolean",
        Kind::Number => "number",
        Kind::String => "string",
        Kind::Array => "array",
        Kind::Object => "object",
        Kind::Any => "object",
    }
}

fn value_kind(value: &Value) -> Kind {
    match value {
        Value::Null => Kind::Null,
        Value::Bool(_) => Kind::Boolean,
        Value::Number(_) => Kind::Number,
        Value::String(_) => Kind::String,
        Value::Array(_) => Kind::Array,
        Value::Object(_) => Kind::Object,
    }
}

fn validate_bounded_schema(schema: &Value, ptr: &str, out: &mut Vec<Diagnostic>) {
    let Some(object) = schema.as_object() else {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_BOUNDED_SUBSET",
            "schemas must be objects in the Templiqx bounded subset",
            ptr,
        ));
        return;
    };
    const ALLOWED: &[&str] = &[
        "type",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "enum",
        "const",
        "format",
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "minItems",
        "maxItems",
        "uniqueItems",
        "description",
        "title",
    ];
    for keyword in object.keys() {
        if !ALLOWED.contains(&keyword.as_str()) {
            out.push(Diagnostic::error(
                "TQX_SCHEMA_KEYWORD_UNSUPPORTED",
                format!("JSON Schema keyword '{keyword}' is outside the POC bounded subset"),
                format!("{ptr}/{keyword}"),
            ));
        }
    }
    let Some(schema_type) = object.get("type").and_then(Value::as_str) else {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_TYPE_REQUIRED",
            "bounded schemas require one explicit scalar type",
            format!("{ptr}/type"),
        ));
        return;
    };
    if !matches!(
        schema_type,
        "null" | "boolean" | "integer" | "number" | "string" | "array" | "object"
    ) {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_TYPE_UNSUPPORTED",
            format!("schema type '{schema_type}' is unsupported"),
            format!("{ptr}/type"),
        ));
    }
    if object
        .get("additionalProperties")
        .is_some_and(|value| !value.is_boolean())
    {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_KEYWORD_UNSUPPORTED",
            "additionalProperties must be a boolean in the bounded subset",
            format!("{ptr}/additionalProperties"),
        ));
    }
    if let Some(values) = object.get("enum").and_then(Value::as_array)
        && (values.is_empty()
            || !values
                .iter()
                .any(|value| value_matches_type(value, schema_type)))
    {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_ENUM_IMPOSSIBLE",
            "enum must contain at least one value matching the declared type",
            format!("{ptr}/enum"),
        ));
    }
    if object
        .get("const")
        .is_some_and(|value| !value_matches_type(value, schema_type))
    {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_CONST_IMPOSSIBLE",
            "const does not match the declared type",
            format!("{ptr}/const"),
        ));
    }
    if let (Some(constant), Some(values)) = (
        object.get("const"),
        object.get("enum").and_then(Value::as_array),
    ) && !values.contains(constant)
    {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_CONST_ENUM_CONTRADICTION",
            "const must be one of the values allowed by enum",
            ptr,
        ));
    }
    if let Some(constant) = object.get("const")
        && !schema_accepts_value(schema, constant)
    {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_CONST_CONTRADICTION",
            "const is rejected by another constraint in the same schema",
            ptr,
        ));
    }
    if let Some(values) = object.get("enum").and_then(Value::as_array)
        && !values
            .iter()
            .any(|candidate| schema_accepts_value(schema, candidate))
    {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_ENUM_IMPOSSIBLE",
            "every enum value is rejected by another constraint in the same schema",
            ptr,
        ));
    }
    if let Some(properties) = object.get("properties") {
        if schema_type != "object" {
            out.push(Diagnostic::error(
                "TQX_SCHEMA_KEYWORD_TYPE",
                "properties is only valid for object schemas",
                format!("{ptr}/properties"),
            ));
        }
        if let Some(properties) = properties.as_object() {
            for (name, property) in properties {
                validate_bounded_schema(property, &format!("{ptr}/properties/{name}"), out);
            }
            if let Some(required) = object.get("required").and_then(Value::as_array) {
                for name in required.iter().filter_map(Value::as_str) {
                    if !properties.contains_key(name) {
                        out.push(Diagnostic::error(
                            "TQX_SCHEMA_REQUIRED_UNKNOWN",
                            format!("required property '{name}' is not declared"),
                            format!("{ptr}/required"),
                        ));
                    }
                }
            }
        }
    }
    if let Some(items) = object.get("items") {
        if schema_type != "array" {
            out.push(Diagnostic::error(
                "TQX_SCHEMA_KEYWORD_TYPE",
                "items is only valid for array schemas",
                format!("{ptr}/items"),
            ));
        }
        validate_bounded_schema(items, &format!("{ptr}/items"), out);
    } else if schema_type == "array" {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_ITEMS_REQUIRED",
            "array schemas require a typed items schema",
            format!("{ptr}/items"),
        ));
    }
    if let Some(format) = object.get("format")
        && (schema_type != "string" || !matches!(format.as_str(), Some("date" | "date-time")))
    {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_FORMAT_UNSUPPORTED",
            "only date and date-time string formats are supported",
            format!("{ptr}/format"),
        ));
    }
    for (minimum, maximum) in [
        ("minLength", "maxLength"),
        ("minItems", "maxItems"),
        ("minimum", "maximum"),
    ] {
        if let (Some(min), Some(max)) = (
            object.get(minimum).and_then(Value::as_f64),
            object.get(maximum).and_then(Value::as_f64),
        ) && min > max
        {
            out.push(Diagnostic::error(
                "TQX_SCHEMA_IMPOSSIBLE_BOUNDS",
                format!("{minimum} cannot exceed {maximum}"),
                ptr,
            ));
        }
    }
    let lower = object.get("minimum").and_then(Value::as_f64);
    let exclusive_lower = object.get("exclusiveMinimum").and_then(Value::as_f64);
    let upper = object.get("maximum").and_then(Value::as_f64);
    let exclusive_upper = object.get("exclusiveMaximum").and_then(Value::as_f64);
    if lower
        .zip(exclusive_upper)
        .is_some_and(|(minimum, maximum)| minimum >= maximum)
        || exclusive_lower
            .zip(upper)
            .is_some_and(|(minimum, maximum)| minimum >= maximum)
        || exclusive_lower
            .zip(exclusive_upper)
            .is_some_and(|(minimum, maximum)| minimum >= maximum)
    {
        out.push(Diagnostic::error(
            "TQX_SCHEMA_IMPOSSIBLE_BOUNDS",
            "numeric lower bound leaves no value below the upper bound",
            ptr,
        ));
    }
    if schema_type == "integer" {
        let effective_lower = [
            lower.map(f64::ceil),
            exclusive_lower.map(|value| value.floor() + 1.0),
        ]
        .into_iter()
        .flatten()
        .reduce(f64::max);
        let effective_upper = [
            upper.map(f64::floor),
            exclusive_upper.map(|value| value.ceil() - 1.0),
        ]
        .into_iter()
        .flatten()
        .reduce(f64::min);
        if effective_lower
            .zip(effective_upper)
            .is_some_and(|(minimum, maximum)| minimum > maximum)
        {
            out.push(Diagnostic::error(
                "TQX_SCHEMA_IMPOSSIBLE_INTEGER_BOUNDS",
                "integer bounds contain no integer value",
                ptr,
            ));
        }
    }
}

fn value_matches_type(value: &Value, schema_type: &str) -> bool {
    match schema_type {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => false,
    }
}

fn schema_accepts_value(schema: &Value, value: &Value) -> bool {
    let Ok(validator) = jsonschema::validator_for(schema) else {
        return false;
    };
    if !validator.is_valid(value) {
        return false;
    }
    let mut diagnostics = Vec::new();
    validate_formats(schema, value, "", "TQX_SCHEMA_PROBE", &mut diagnostics);
    diagnostics.is_empty()
}

fn validate_instance(
    schema: &Value,
    value: &Value,
    ptr: &str,
    code: &str,
    out: &mut Vec<Diagnostic>,
) {
    if let Ok(validator) = jsonschema::validator_for(schema) {
        for error in validator.iter_errors(value) {
            out.push(Diagnostic::error(code, error.to_string(), ptr));
        }
    }
    validate_formats(schema, value, ptr, code, out);
}

fn validate_formats(
    schema: &Value,
    value: &Value,
    ptr: &str,
    code: &str,
    out: &mut Vec<Diagnostic>,
) {
    if let (Some(format), Some(text)) =
        (schema.get("format").and_then(Value::as_str), value.as_str())
    {
        let valid = match format {
            "date" => valid_date(text),
            "date-time" => valid_date_time(text),
            _ => true,
        };
        if !valid {
            out.push(Diagnostic::error(
                code,
                format!("value is not a valid {format}"),
                ptr,
            ));
        }
    }
    if let (Some(properties), Some(values)) = (
        schema.get("properties").and_then(Value::as_object),
        value.as_object(),
    ) {
        for (name, child_schema) in properties {
            if let Some(child) = values.get(name) {
                validate_formats(child_schema, child, &format!("{ptr}/{name}"), code, out);
            }
        }
    }
    if let (Some(item_schema), Some(items)) = (schema.get("items"), value.as_array()) {
        for (index, item) in items.iter().enumerate() {
            validate_formats(item_schema, item, &format!("{ptr}/{index}"), code, out);
        }
    }
}

fn valid_date(value: &str) -> bool {
    if !value.is_ascii() || value.len() != 10 || &value[4..5] != "-" || &value[7..8] != "-" {
        return false;
    }
    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

fn valid_date_time(value: &str) -> bool {
    if !value.is_ascii() || value.len() < 20 || !valid_date(&value[..10]) || &value[10..11] != "T" {
        return false;
    }
    let time = &value[11..];
    if time.len() < 9 || &time[2..3] != ":" || &time[5..6] != ":" {
        return false;
    }
    let (Ok(hour), Ok(minute), Ok(second)) = (
        time[0..2].parse::<u32>(),
        time[3..5].parse::<u32>(),
        time[6..8].parse::<u32>(),
    ) else {
        return false;
    };
    if hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    let suffix = &time[8..];
    if suffix == "Z" {
        return true;
    }
    let timezone = if let Some(fraction) = suffix.strip_prefix('.') {
        let split = fraction.find(['Z', '+', '-']);
        let Some(index) = split else { return false };
        if index == 0 || !fraction[..index].bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        &fraction[index..]
    } else {
        suffix
    };
    if timezone == "Z" {
        return true;
    }
    if timezone.len() != 6 || !matches!(&timezone[..1], "+" | "-") || &timezone[3..4] != ":" {
        return false;
    }
    matches!(
        (timezone[1..3].parse::<u32>(), timezone[4..6].parse::<u32>()),
        (Ok(0..=23), Ok(0..=59))
    )
}

pub fn validate_values(contract: &Contract, request: &RenderRequest) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (scope, specs, values) in [
        ("inputs", &contract.inputs, &request.inputs),
        ("context", &contract.context, &request.context),
    ] {
        for (name, spec) in specs {
            match values.get(name) {
                None if spec.required => out.push(Diagnostic::error(
                    "TQX_VALUE_REQUIRED",
                    format!("required {scope} value '{name}' is missing"),
                    format!("/{scope}/{name}"),
                )),
                Some(value) => match jsonschema::validator_for(&spec.schema) {
                    Ok(_) => validate_instance(
                        &spec.schema,
                        value,
                        &format!("/{scope}/{name}"),
                        "TQX_VALUE_SCHEMA",
                        &mut out,
                    ),
                    Err(error) => out.push(Diagnostic::error(
                        "TQX_FIELD_SCHEMA_INVALID",
                        error.to_string(),
                        format!("/{scope}/{name}"),
                    )),
                },
                None => {}
            }
        }
        for name in values.keys() {
            if scope == "context" && name.starts_with("_templiqx_") {
                continue;
            }
            if !specs.contains_key(name) {
                out.push(Diagnostic::error(
                    "TQX_VALUE_UNKNOWN",
                    format!("undeclared {scope} value '{name}'"),
                    format!("/{scope}/{name}"),
                ));
            }
        }
    }
    out
}

pub fn compile(
    contract: &Contract,
    request: &RenderRequest,
    target_capabilities: &[String],
) -> Result<CompiledInteraction, Vec<Diagnostic>> {
    let mut errors = validate_contract(contract);
    errors.extend(validate_values(contract, request));
    let target: BTreeSet<&str> = target_capabilities.iter().map(String::as_str).collect();
    for capability in &contract.capabilities {
        if !target.contains(capability.as_str()) {
            errors.push(Diagnostic::error(
                "TQX_CAPABILITY_UNSUPPORTED",
                format!("target lacks required capability '{capability}'"),
                "/capabilities",
            ));
        }
    }
    for (name, extension) in &contract.extensions {
        if !target.contains(extension.capability.as_str()) {
            errors.push(Diagnostic::error(
                "TQX_EXTENSION_UNSUPPORTED",
                format!(
                    "target lacks capability '{}' required by extension '{name}'",
                    extension.capability
                ),
                format!("/extensions/{name}"),
            ));
        }
    }
    if errors.iter().any(|d| d.severity == Severity::Error) {
        return Err(errors);
    }
    let root = root_value(request);
    let mut messages = Vec::new();
    for MessageTemplate { role, content } in &contract.messages {
        messages.push(CompiledMessage {
            role: role.clone(),
            content: render_nodes(content, contract, &root, &BTreeMap::new(), &mut Vec::new())?,
        });
    }
    let mut target_capabilities = target_capabilities.to_vec();
    target_capabilities.sort();
    target_capabilities.dedup();
    let mut required_capabilities = contract.capabilities.clone();
    required_capabilities.extend(
        contract
            .extensions
            .values()
            .map(|extension| extension.capability.clone()),
    );
    required_capabilities.sort();
    required_capabilities.dedup();
    Ok(CompiledInteraction {
        compiler: format!("templiqx-core/{}", env!("CARGO_PKG_VERSION")),
        contract_id: contract.id.clone(),
        contract_version: contract.version.clone(),
        messages,
        output_schema: contract.output_schema.clone(),
        required_capabilities,
        target_capabilities,
        runtime_policy: contract.runtime_policy.clone(),
        extensions: contract
            .extensions
            .iter()
            .map(|(name, extension)| (name.clone(), extension.value.clone()))
            .collect(),
    })
}

fn root_value(request: &RenderRequest) -> Value {
    let mut root = Map::new();
    root.insert(
        "inputs".into(),
        Value::Object(request.inputs.clone().into_iter().collect()),
    );
    root.insert(
        "context".into(),
        Value::Object(request.context.clone().into_iter().collect()),
    );
    Value::Object(root)
}

fn render_nodes(
    nodes: &[Node],
    contract: &Contract,
    root: &Value,
    locals: &BTreeMap<String, Value>,
    stack: &mut Vec<String>,
) -> Result<String, Vec<Diagnostic>> {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text { value } => out.push_str(value),
            Node::Include { .. } => {
                return Err(vec![Diagnostic::error(
                    "TQX_INCLUDE_UNEXPANDED",
                    "include node reached rendering unexpanded (composition layer must expand includes)",
                    "",
                )]);
            }
            Node::Interpolate {
                expression,
                filters,
            } => {
                let mut value = eval(expression, root, locals)?;
                let locale = root
                    .get("context")
                    .and_then(|c| c.get("locale"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                for f in filters {
                    value = apply_filter(value, f, locale, root)?;
                }
                out.push_str(&display_value(&value));
            }
            Node::When {
                condition,
                then,
                otherwise,
            } => {
                let branch = if boolean_value(eval(condition, root, locals)?)? {
                    then
                } else {
                    otherwise
                };
                out.push_str(&render_nodes(branch, contract, root, locals, stack)?);
            }
            Node::ForEach {
                collection,
                item,
                body,
                separator,
            } => {
                let value = eval(collection, root, locals)?;
                let Value::Array(items) = value else {
                    return Err(vec![Diagnostic::error(
                        "TQX_FOR_EACH_TYPE",
                        "for_each collection must resolve to an array",
                        "",
                    )]);
                };
                let mut rendered = Vec::new();
                for value in items {
                    let mut next = locals.clone();
                    next.insert(item.clone(), value);
                    rendered.push(render_nodes(body, contract, root, &next, stack)?);
                }
                out.push_str(&rendered.join(separator));
            }
            Node::Component { name, with } => {
                if stack.contains(name) {
                    return Err(vec![Diagnostic::error(
                        "TQX_COMPONENT_CYCLE",
                        format!("component cycle at '{name}'"),
                        "",
                    )]);
                }
                let Some(component) = contract.components.get(name) else {
                    return Err(vec![Diagnostic::error(
                        "TQX_COMPONENT_MISSING",
                        format!("unknown component '{name}'"),
                        "",
                    )]);
                };
                let mut next = locals.clone();
                for (key, expr) in with {
                    next.insert(key.clone(), eval(expr, root, locals)?);
                }
                stack.push(name.clone());
                out.push_str(&render_nodes(
                    component.content(),
                    contract,
                    root,
                    &next,
                    stack,
                )?);
                stack.pop();
            }
        }
    }
    Ok(out)
}

fn eval(
    expr: &Expr,
    root: &Value,
    locals: &BTreeMap<String, Value>,
) -> Result<Value, Vec<Diagnostic>> {
    Ok(match expr {
        Expr::Ref { path } => resolve(path, root, locals).cloned().ok_or_else(|| {
            vec![Diagnostic::error(
                "TQX_VALUE_MISSING",
                format!("reference '{path}' is missing"),
                "",
            )]
        })?,
        Expr::Literal { value } => value.clone(),
        Expr::Equals { left, right } => {
            Value::Bool(eval(left, root, locals)? == eval(right, root, locals)?)
        }
        Expr::Not { value } => Value::Bool(!boolean_value(eval(value, root, locals)?)?),
        Expr::And { values } => Value::Bool(values.iter().try_fold(true, |a, v| {
            Ok::<_, Vec<Diagnostic>>(a && boolean_value(eval(v, root, locals)?)?)
        })?),
        Expr::Or { values } => Value::Bool(values.iter().try_fold(false, |a, v| {
            Ok::<_, Vec<Diagnostic>>(a || boolean_value(eval(v, root, locals)?)?)
        })?),
    })
}

fn resolve<'a>(
    path: &str,
    root: &'a Value,
    locals: &'a BTreeMap<String, Value>,
) -> Option<&'a Value> {
    let mut parts = path.split('.');
    let first = parts.next()?;
    let mut value = locals.get(first).or_else(|| root.get(first))?;
    for part in parts {
        value = match value {
            Value::Object(map) => map.get(part)?,
            Value::Array(xs) => xs.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}
fn boolean_value(value: Value) -> Result<bool, Vec<Diagnostic>> {
    value.as_bool().ok_or_else(|| {
        vec![Diagnostic::error(
            "TQX_BOOLEAN_TYPE",
            "boolean expression resolved to a non-boolean value",
            "",
        )]
    })
}
fn apply_filter(
    v: Value,
    f: &Filter,
    locale: &str,
    root: &Value,
) -> Result<Value, Vec<Diagnostic>> {
    Ok(match f {
        Filter::Trim => Value::String(display_value(&v).trim().to_owned()),
        Filter::Lower => Value::String(display_value(&v).to_lowercase()),
        Filter::Upper => Value::String(display_value(&v).to_uppercase()),
        Filter::Json => Value::String(serde_json::to_string(&v).unwrap_or_default()),
        Filter::FormatDate => Value::String(format_date(&v, locale)?),
        Filter::FormatNumber => Value::String(format_number(&v, locale)?),
        Filter::FormatCurrency => Value::String(format_currency(&v, locale)?),
        Filter::Translate => Value::String(translate_key(&v, locale, root)?),
    })
}

fn primary_language_subtag(locale: &str) -> &str {
    locale.split(['-', '_']).next().unwrap_or(locale)
}

fn locale_lookup_candidates(locale: &str, fallback: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut push = |value: &str| {
        if value.is_empty() {
            return;
        }
        if !candidates.iter().any(|candidate| candidate == value) {
            candidates.push(value.to_owned());
        }
        let primary = primary_language_subtag(value);
        if primary != value && !candidates.iter().any(|candidate| candidate == primary) {
            candidates.push(primary.to_owned());
        }
    };
    push(locale);
    push(fallback);
    push("en");
    candidates
}

fn translate_key(v: &Value, locale: &str, root: &Value) -> Result<String, Vec<Diagnostic>> {
    let key = match v {
        Value::String(s) => s.as_str(),
        _ => {
            return Err(vec![Diagnostic::error(
                "TQX_FILTER_INPUT",
                "translate expects a string translation key",
                "",
            )]);
        }
    };
    let bundles = root
        .get("context")
        .and_then(|c| c.get("_templiqx_translations"))
        .and_then(Value::as_object);
    let Some(bundles) = bundles else {
        return Err(vec![Diagnostic::error(
            "TQX_TRANSLATION_MISSING",
            "no translation bundles were loaded for this package",
            "",
        )]);
    };
    let fallback = root
        .get("context")
        .and_then(|c| c.get("fallback_locale"))
        .and_then(Value::as_str)
        .unwrap_or("");
    for candidate in locale_lookup_candidates(locale, fallback) {
        if let Some(bundle) = bundles.get(&candidate).and_then(Value::as_object)
            && let Some(value) = bundle.get(key).and_then(Value::as_str)
        {
            return Ok(value.to_owned());
        }
    }
    Err(vec![Diagnostic::error(
        "TQX_TRANSLATION_KEY",
        format!("missing translation key '{key}'"),
        "",
    )])
}

fn format_currency(v: &Value, locale: &str) -> Result<String, Vec<Diagnostic>> {
    let number = v
        .as_f64()
        .or_else(|| v.as_i64().map(|n| n as f64))
        .ok_or_else(|| {
            vec![Diagnostic::error(
                "TQX_FILTER_INPUT",
                "format_currency expects a numeric value",
                "",
            )]
        })?;
    let formatted = format_number(&Value::from(number), locale)?;
    let symbol = match currency_symbol(locale) {
        "eur" => "€",
        _ => "$",
    };
    Ok(format!("{symbol}{formatted}"))
}

fn currency_symbol(locale: &str) -> &'static str {
    let lower = locale.to_ascii_lowercase();
    if lower.starts_with("nl")
        || lower.starts_with("de")
        || lower.starts_with("fr")
        || lower.starts_with("es")
        || lower.starts_with("it")
        || lower.starts_with("pt")
    {
        "eur"
    } else {
        "usd"
    }
}

/// Locale families sharing a grouping/date convention. Kept intentionally small
/// and explicit — unknown locales fall back to ISO/plain so rendering never fails
/// on locale alone.
fn locale_family(locale: &str) -> &'static str {
    let lower = locale.to_ascii_lowercase();
    if lower.starts_with("nl") {
        "nl"
    } else if lower.starts_with("de") {
        "de"
    } else if lower.starts_with("en-us") || lower.starts_with("en_us") {
        "en-us"
    } else {
        "iso"
    }
}

fn format_date(v: &Value, locale: &str) -> Result<String, Vec<Diagnostic>> {
    let Value::String(raw) = v else {
        return Err(vec![Diagnostic::error(
            "TQX_FILTER_INPUT",
            "format_date expects an ISO 'YYYY-MM-DD' string",
            "",
        )]);
    };
    let parts: Vec<&str> = raw.split('-').collect();
    let valid = parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()));
    if !valid {
        return Err(vec![Diagnostic::error(
            "TQX_FILTER_INPUT",
            format!("format_date could not parse '{raw}' as 'YYYY-MM-DD'"),
            "",
        )]);
    }
    let (y, m, d) = (parts[0], parts[1], parts[2]);
    Ok(match locale_family(locale) {
        "nl" => format!("{d}-{m}-{y}"),
        "de" => format!("{d}.{m}.{y}"),
        "en-us" => format!("{m}/{d}/{y}"),
        _ => format!("{y}-{m}-{d}"),
    })
}

fn format_number(v: &Value, locale: &str) -> Result<String, Vec<Diagnostic>> {
    let number = v.as_f64().ok_or_else(|| {
        vec![Diagnostic::error(
            "TQX_FILTER_INPUT",
            "format_number expects a numeric value",
            "",
        )]
    })?;
    let (group, decimal) = match locale_family(locale) {
        "nl" | "de" => ('.', ','),
        _ => (',', '.'),
    };
    let negative = number.is_sign_negative() && number != 0.0;
    let text = format!("{:.2}", number.abs());
    let (int_part, frac_part) = text.split_once('.').unwrap_or((&text, "00"));
    let mut grouped = String::new();
    let bytes = int_part.as_bytes();
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            grouped.push(group);
        }
        grouped.push(*byte as char);
    }
    Ok(format!(
        "{}{grouped}{decimal}{frac_part}",
        if negative { "-" } else { "" }
    ))
}
fn display_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

pub fn validate_output(schema: &Value, output: &Value) -> Vec<Diagnostic> {
    match jsonschema::validator_for(schema) {
        Ok(_) => {
            let mut diagnostics = Vec::new();
            validate_instance(
                schema,
                output,
                "/output",
                "TQX_OUTPUT_SCHEMA",
                &mut diagnostics,
            );
            diagnostics
        }
        Err(e) => vec![Diagnostic::error(
            "TQX_OUTPUT_SCHEMA_INVALID",
            e.to_string(),
            "/output_schema",
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"
api_version: templiqx/v1alpha1
id: typed
version: 1.0.0
inputs:
  people:
    schema: {type: array, items: {type: string}}
    required: true
context:
  enabled:
    schema: {type: boolean}
    required: true
capabilities: [structured_output]
messages:
  - role: user
    content:
      - kind: when
        condition: {kind: ref, path: context.enabled}
        then:
          - kind: for_each
            collection: {kind: ref, path: inputs.people}
            item: person
            separator: ", "
            body:
              - kind: interpolate
                expression: {kind: ref, path: person}
                filters: [upper]
        else: [{kind: text, value: disabled}]
output_schema: {type: object, required: [ok], properties: {ok: {type: boolean}}}
"#;

    #[test]
    fn strict_yaml_rejects_unknown_fields() {
        let source = SOURCE.replace("id: typed", "id: typed\nunknown: true");
        let diagnostics = parse_contract(&source, Some("typed.yaml")).unwrap_err();
        assert_eq!(diagnostics[0].code, "TQX_PARSE_YAML");
        assert_eq!(diagnostics[0].file.as_deref(), Some("typed.yaml"));
    }

    #[test]
    fn renders_bounded_nodes_deterministically() {
        let contract = parse_contract(SOURCE, None).unwrap();
        let request: RenderRequest = serde_json::from_value(serde_json::json!({
            "inputs":{"people":["ada", "ryan"]}, "context":{"enabled":true}
        }))
        .unwrap();
        let compiled = compile(&contract, &request, &["structured_output".into()]).unwrap();
        assert_eq!(compiled.messages[0].content, "ADA, RYAN");
        assert_eq!(
            templiqx_contracts::fingerprint(&compiled).unwrap(),
            templiqx_contracts::fingerprint(&compiled).unwrap()
        );
    }

    #[test]
    fn rejects_missing_values_and_capabilities_before_compile() {
        let contract = parse_contract(SOURCE, None).unwrap();
        let diagnostics = compile(
            &contract,
            &RenderRequest {
                inputs: BTreeMap::new(),
                context: BTreeMap::new(),
            },
            &[],
        )
        .unwrap_err();
        assert!(diagnostics.iter().any(|d| d.code == "TQX_VALUE_REQUIRED"));
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "TQX_CAPABILITY_UNSUPPORTED")
        );
    }

    #[test]
    fn rejects_wrong_scope_reference() {
        let source = SOURCE.replace("context.enabled", "inputs.enabled");
        let contract = parse_contract(&source, None).unwrap();
        assert!(
            validate_contract(&contract)
                .iter()
                .any(|d| d.code == "TQX_REF_UNKNOWN")
        );
    }

    #[test]
    fn rejects_incompatible_structured_node_type() {
        let source = SOURCE.replace(
            "schema: {type: array, items: {type: string}}",
            "schema: {type: string}",
        );
        let contract = parse_contract(&source, None).unwrap();
        assert!(
            validate_contract(&contract)
                .iter()
                .any(|d| d.code == "TQX_FOR_EACH_TYPE")
        );
    }

    #[test]
    fn validates_nested_schema_paths_including_for_each_items() {
        let source = SOURCE
            .replace(
                "schema: {type: array, items: {type: string}}",
                "schema: {type: array, items: {type: object, required: [name], properties: {name: {type: string}}}}",
            )
            .replace("path: person}", "path: person.name}");
        let contract = parse_contract(&source, None).unwrap();
        assert!(validate_contract(&contract).is_empty());

        let invalid =
            parse_contract(&source.replace("person.name", "person.missing"), None).unwrap();
        assert!(
            validate_contract(&invalid)
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_REF_UNKNOWN")
        );
    }

    #[test]
    fn rejects_non_boolean_operators_and_conditions() {
        let source = SOURCE.replace(
            "condition: {kind: ref, path: context.enabled}",
            "condition: {kind: not, value: {kind: literal, value: nope}}",
        );
        let contract = parse_contract(&source, None).unwrap();
        let diagnostics = validate_contract(&contract);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_BOOLEAN_TYPE")
        );
    }

    #[test]
    fn complex_interpolation_requires_json_filter() {
        let mut contract = parse_contract(SOURCE, None).unwrap();
        contract.messages[0].content = vec![Node::Interpolate {
            expression: Expr::Ref {
                path: "inputs.people".into(),
            },
            filters: vec![],
        }];
        assert!(
            validate_contract(&contract)
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_INTERPOLATION_TYPE")
        );
        contract.messages[0].content = vec![Node::Interpolate {
            expression: Expr::Ref {
                path: "inputs.people".into(),
            },
            filters: vec![Filter::Json],
        }];
        assert!(validate_contract(&contract).is_empty());
    }

    #[test]
    fn typed_components_reject_missing_unknown_and_incompatible_arguments() {
        let source = r#"
api_version: templiqx/v1alpha1
id: typed-component
version: 1.0.0
inputs:
  count: {schema: {type: integer}, required: true}
messages:
  - role: user
    content:
      - kind: component
        name: greeting
        with:
          recipient: {kind: ref, path: inputs.count}
          extra: {kind: literal, value: true}
output_schema: {type: string}
components:
  greeting:
    parameters:
      recipient: {schema: {type: string}, required: true}
      formal: {schema: {type: boolean}, required: true}
    content:
      - kind: interpolate
        expression: {kind: ref, path: recipient}
"#;
        let contract = parse_contract(source, None).unwrap();
        let diagnostics = validate_contract(&contract);
        for code in [
            "TQX_COMPONENT_ARGUMENT_MISSING",
            "TQX_COMPONENT_ARGUMENT_UNKNOWN",
            "TQX_COMPONENT_ARGUMENT_TYPE",
        ] {
            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.code == code),
                "missing {code}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_and_impossible_schema_constructs() {
        let mut contract = parse_contract(SOURCE, None).unwrap();
        contract.output_schema = serde_json::json!({
            "type": "object",
            "oneOf": [{"type": "object"}]
        });
        contract.inputs.get_mut("people").unwrap().schema = serde_json::json!({
            "type": "array",
            "items": {"type": "string"},
            "minItems": 3,
            "maxItems": 1
        });
        let diagnostics = validate_contract(&contract);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_SCHEMA_KEYWORD_UNSUPPORTED")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_SCHEMA_IMPOSSIBLE_BOUNDS")
        );
    }

    #[test]
    fn rejects_cross_keyword_schema_contradictions() {
        let mut contract = parse_contract(SOURCE, None).unwrap();
        contract.output_schema = serde_json::json!({
            "type": "string",
            "const": "a",
            "enum": ["b"]
        });
        let diagnostics = validate_contract(&contract);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "TQX_SCHEMA_CONST_ENUM_CONTRADICTION" })
        );

        contract.output_schema = serde_json::json!({
            "type": "number",
            "minimum": 2,
            "exclusiveMaximum": 2
        });
        assert!(
            validate_contract(&contract)
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_SCHEMA_IMPOSSIBLE_BOUNDS")
        );

        contract.output_schema = serde_json::json!({
            "type": "integer",
            "const": 1,
            "minimum": 2
        });
        assert!(
            validate_contract(&contract)
                .iter()
                .any(|diagnostic| { diagnostic.code == "TQX_SCHEMA_CONST_CONTRADICTION" })
        );

        contract.output_schema = serde_json::json!({
            "type": "integer",
            "exclusiveMinimum": 1,
            "exclusiveMaximum": 2
        });
        assert!(
            validate_contract(&contract)
                .iter()
                .any(|diagnostic| { diagnostic.code == "TQX_SCHEMA_IMPOSSIBLE_INTEGER_BOUNDS" })
        );

        contract.output_schema = serde_json::json!({
            "type": "string",
            "const": "a",
            "minLength": 2
        });
        assert!(
            validate_contract(&contract)
                .iter()
                .any(|diagnostic| { diagnostic.code == "TQX_SCHEMA_CONST_CONTRADICTION" })
        );
    }

    #[test]
    fn rejects_invalid_component_and_extension_schemas() {
        let mut contract = parse_contract(SOURCE, None).unwrap();
        contract.components.insert(
            "invalid".into(),
            templiqx_contracts::ComponentDefinition::Typed(templiqx_contracts::TypedComponent {
                parameters: BTreeMap::from([(
                    "value".into(),
                    templiqx_contracts::FieldSpec {
                        schema: serde_json::json!({
                            "type": "object",
                            "properties": []
                        }),
                        required: true,
                        description: String::new(),
                    },
                )]),
                content: vec![],
            }),
        );
        contract.extensions.insert(
            "vendor.invalid".into(),
            templiqx_contracts::ExtensionSpec {
                capability: "vendor.invalid".into(),
                schema: serde_json::json!({"type": "array", "items": []}),
                value: serde_json::json!([]),
            },
        );
        let diagnostics = validate_contract(&contract);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_COMPONENT_SCHEMA_INVALID")
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_EXTENSION_SCHEMA_INVALID")
        );
    }

    #[test]
    fn enforces_date_and_date_time_formats_at_runtime() {
        let mut contract = parse_contract(SOURCE, None).unwrap();
        contract.inputs.get_mut("people").unwrap().schema =
            serde_json::json!({"type": "string", "format": "date"});
        let request = RenderRequest {
            inputs: BTreeMap::from([("people".into(), Value::String("2026-02-30".into()))]),
            context: BTreeMap::from([("enabled".into(), Value::Bool(true))]),
        };
        assert!(
            validate_values(&contract, &request)
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_VALUE_SCHEMA")
        );
        let schema = serde_json::json!({
            "type": "object",
            "properties": {"at": {"type": "string", "format": "date-time"}},
            "required": ["at"]
        });
        assert!(
            !validate_output(&schema, &serde_json::json!({"at": "2026-07-11 10:00:00"})).is_empty()
        );
        assert!(
            validate_output(&schema, &serde_json::json!({"at": "2026-07-11T10:00:00Z"})).is_empty()
        );
    }

    #[test]
    fn typed_extension_validates_value_and_capability_gate() {
        let mut contract = parse_contract(SOURCE, None).unwrap();
        contract.extensions.insert(
            "vendor.reasoning".into(),
            templiqx_contracts::ExtensionSpec {
                capability: "vendor.reasoning".into(),
                schema: serde_json::json!({"type": "integer", "minimum": 1}),
                value: Value::String("high".into()),
            },
        );
        assert!(
            validate_contract(&contract)
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_EXTENSION_VALUE")
        );
        contract
            .extensions
            .get_mut("vendor.reasoning")
            .unwrap()
            .value = serde_json::json!(2);
        let request: RenderRequest = serde_json::from_value(serde_json::json!({
            "inputs":{"people":["ada"]}, "context":{"enabled":true}
        }))
        .unwrap();
        let missing = compile(&contract, &request, &["structured_output".into()]).unwrap_err();
        assert!(
            missing
                .iter()
                .any(|diagnostic| diagnostic.code == "TQX_EXTENSION_UNSUPPORTED")
        );
        let compiled = compile(
            &contract,
            &request,
            &["structured_output".into(), "vendor.reasoning".into()],
        )
        .unwrap();
        assert!(
            compiled
                .required_capabilities
                .contains(&"vendor.reasoning".into())
        );
    }
}
