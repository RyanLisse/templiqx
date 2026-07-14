use serde_yaml_ng::Value;
use std::collections::{BTreeMap, BTreeSet};

const HTTP_METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];
const META_ROUTES: [(&str, &str); 6] = [
    ("GET", "/healthz"),
    ("GET", "/readyz"),
    ("GET", "/operations/v1/health/live"),
    ("GET", "/operations/v1/health/ready"),
    ("GET", "/operations/v1/openapi.yaml"),
    ("GET", "/operations/v1/openapi.json"),
];

#[test]
fn registered_operation_routes_and_openapi_paths_do_not_drift() {
    let source = include_str!("../src/lib.rs");
    let registered = registered_operations(source);
    let mut routed = registered.keys().cloned().collect::<BTreeSet<_>>();
    for (method, path) in META_ROUTES {
        assert!(
            routed.remove(&(method.to_owned(), path.to_owned())),
            "allow-listed meta route {method} {path} is no longer registered"
        );
    }

    let document: Value =
        serde_yaml_ng::from_str(include_str!("../../../openapi/templiqx-operations-v1.yaml"))
            .expect("checked-in OpenAPI must parse as YAML");
    let mut documented = documented_routes(&document);
    for (method, path) in META_ROUTES {
        documented.remove(&(method.to_owned(), path.to_owned()));
    }

    let undocumented = routed.difference(&documented).cloned().collect::<Vec<_>>();
    let unrouted = documented.difference(&routed).cloned().collect::<Vec<_>>();
    assert!(
        undocumented.is_empty() && unrouted.is_empty(),
        "HTTP/OpenAPI drift detected. Routes missing from OpenAPI: {undocumented:?}. OpenAPI operations missing from router: {unrouted:?}. Update router(...) and openapi/templiqx-operations-v1.yaml together."
    );
    assert_eq!(
        routed.len(),
        26,
        "the catalog must expose exactly 26 operations"
    );

    let openapi_operations = documented_operation_ids(&document);
    let catalog_operations = templiqx_application::CAPABILITY_CATALOG
        .iter()
        .map(|operation| (*operation).to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        openapi_operations, catalog_operations,
        "application capability catalog and non-meta OpenAPI operationIds drifted; document each canonical operation exactly once"
    );

    for ((method, path), handler) in registered {
        if META_ROUTES.contains(&(method.as_str(), path.as_str())) {
            continue;
        }
        let documented_operation = documented_operation(&document, &method, &path);
        let operation_id = documented_operation
            .get("operationId")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{method} {path} must declare operationId"));
        assert_eq!(
            camel_to_snake(operation_id),
            handler,
            "OpenAPI operationId binding drift for {method} {path}: router dispatches to `{handler}`, but the document declares `{operation_id}`"
        );
    }
}

#[test]
fn json_request_dto_fields_and_openapi_schemas_do_not_drift() {
    let http_source = include_str!("../src/lib.rs");
    let application_source = include_str!("../../templiqx-application/src/lib.rs");
    let document: Value =
        serde_yaml_ng::from_str(include_str!("../../../openapi/templiqx-operations-v1.yaml"))
            .expect("checked-in OpenAPI must parse as YAML");

    let mut checked = 0usize;
    for ((method, path), handler) in registered_operations(http_source) {
        let Some(dto) = handler_json_request_dto(http_source, &handler) else {
            continue;
        };
        let operation = documented_operation(&document, &method, &path);
        let operation_id = operation
            .get("operationId")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{method} {path} must declare operationId"));
        let schema = operation
            .get("requestBody")
            .and_then(|body| body.get("content"))
            .and_then(|content| content.get("application/json"))
            .and_then(|media_type| media_type.get("schema"))
            .unwrap_or_else(|| {
                panic!("{method} {path} ({operation_id}) accepts Json<{dto}> but has no application/json request schema")
            });
        let schema = resolve_schema(&document, schema);
        let documented_properties = schema
            .get("properties")
            .and_then(Value::as_mapping)
            .map(|properties| {
                properties
                    .keys()
                    .map(|key| key.as_str().expect("schema property name").to_owned())
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let documented_required = schema
            .get("required")
            .and_then(Value::as_sequence)
            .map(|required| {
                required
                    .iter()
                    .map(|field| field.as_str().expect("required field name").to_owned())
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let (dto_properties, dto_required) = dto_fields(&dto, &[http_source, application_source]);

        assert_eq!(
            documented_properties, dto_properties,
            "OpenAPI request property drift for {method} {path} ({operation_id}) versus Json<{dto}>"
        );
        assert_eq!(
            documented_required, dto_required,
            "OpenAPI required-field drift for {method} {path} ({operation_id}) versus Json<{dto}>"
        );
        checked += 1;
    }

    assert_eq!(checked, 12, "every JSON request-body DTO must be checked");
}

fn documented_operation_ids(document: &Value) -> BTreeSet<String> {
    let paths = document
        .get("paths")
        .and_then(Value::as_mapping)
        .expect("OpenAPI paths mapping");
    let meta_paths = META_ROUTES
        .iter()
        .map(|(_, path)| *path)
        .collect::<BTreeSet<_>>();
    let mut operations = BTreeSet::new();
    for (path, item) in paths {
        let path = path.as_str().expect("OpenAPI path key");
        if meta_paths.contains(path) {
            continue;
        }
        let methods = item.as_mapping().expect("OpenAPI path item");
        for method in HTTP_METHODS {
            let Some(operation) = methods.get(Value::String(method.to_owned())) else {
                continue;
            };
            let operation_id = operation
                .get("operationId")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{method} {path} must declare operationId"));
            assert!(
                operations.insert(camel_to_snake(operation_id)),
                "duplicate canonical OpenAPI operationId: {operation_id}"
            );
        }
    }
    operations
}

fn camel_to_snake(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_uppercase() {
            if !result.is_empty() {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
        } else {
            result.push(character);
        }
    }
    result
}

fn documented_routes(document: &Value) -> BTreeSet<(String, String)> {
    let paths = document
        .get("paths")
        .and_then(Value::as_mapping)
        .expect("OpenAPI paths mapping");
    let mut routes = BTreeSet::new();
    for (path, item) in paths {
        let path = path.as_str().expect("OpenAPI path key");
        let methods = item.as_mapping().expect("OpenAPI path item");
        for method in HTTP_METHODS {
            if methods.contains_key(Value::String(method.to_owned())) {
                routes.insert((method.to_uppercase(), path.to_owned()));
            }
        }
    }
    routes
}

fn documented_operation<'a>(document: &'a Value, method: &str, path: &str) -> &'a Value {
    document
        .get("paths")
        .and_then(|paths| paths.get(path))
        .and_then(|item| item.get(method.to_ascii_lowercase()))
        .unwrap_or_else(|| panic!("OpenAPI operation {method} {path}"))
}

fn registered_operations(source: &str) -> BTreeMap<(String, String), String> {
    let mut routes = BTreeMap::new();
    let mut cursor = 0;
    while let Some(relative_start) = source[cursor..].find(".route(") {
        let start = cursor + relative_start + ".route(".len();
        let end = matching_call_end(source, start);
        let call = &source[start..end];
        let path = first_string_literal(call).replace("{*artifact}", "{artifact}");
        for method in HTTP_METHODS {
            if let Some(handler) = route_handler(call, method) {
                let route = (method.to_uppercase(), path.clone());
                assert!(
                    routes.insert(route.clone(), handler).is_none(),
                    "duplicate registered route: {} {}",
                    route.0,
                    route.1
                );
            }
        }
        cursor = end;
    }
    routes
}

fn route_handler(call: &str, method: &str) -> Option<String> {
    let direct = format!("{method}(");
    let chained = format!(".{method}(");
    let start = call
        .find(&chained)
        .map(|position| position + chained.len())
        .or_else(|| call.find(&direct).map(|position| position + direct.len()))?;
    let handler = call[start..]
        .trim_start()
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect::<String>();
    assert!(!handler.is_empty(), "handler for {method} route");
    Some(handler)
}

fn handler_json_request_dto(source: &str, handler: &str) -> Option<String> {
    let marker = format!("async fn {handler}(");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("handler `{handler}` must be defined in the HTTP crate"));
    let signature_end = matching_delimiter_end(source, start + marker.len(), '(', ')');
    let signature = &source[start..=signature_end];
    let json_start = signature.find("Json<")? + "Json<".len();
    let json_end = matching_delimiter_end(signature, json_start, '<', '>');
    Some(signature[json_start..json_end].trim().to_owned())
}

fn resolve_schema<'a>(document: &'a Value, schema: &'a Value) -> &'a Value {
    let Some(reference) = schema.get("$ref").and_then(Value::as_str) else {
        return schema;
    };
    let component = reference
        .strip_prefix("#/components/schemas/")
        .unwrap_or_else(|| panic!("unsupported request schema reference: {reference}"));
    document
        .get("components")
        .and_then(|components| components.get("schemas"))
        .and_then(|schemas| schemas.get(component))
        .unwrap_or_else(|| panic!("request schema component `{component}`"))
}

fn dto_fields(dto: &str, sources: &[&str]) -> (BTreeSet<String>, BTreeSet<String>) {
    let source = sources
        .iter()
        .find(|source| {
            source.contains(&format!("struct {dto} "))
                || source.contains(&format!("struct {dto}\n"))
        })
        .unwrap_or_else(|| panic!("request DTO `{dto}` must have a source definition"));
    let body = named_struct_body(source, dto);
    let mut properties = BTreeSet::new();
    let mut required = BTreeSet::new();
    let mut serde_attribute = String::new();

    for line in body.lines() {
        let line = line.trim();
        if line.starts_with("#[serde(") {
            serde_attribute = line.to_owned();
            continue;
        }
        if line.is_empty() || line.starts_with("///") || line.starts_with("#[") {
            continue;
        }
        let field = line.strip_prefix("pub ").unwrap_or(line);
        let Some((name, field_type)) = field.trim_end_matches(',').split_once(':') else {
            continue;
        };
        let name = name.trim();
        let field_type = field_type.trim();
        if serde_attribute.contains("flatten") {
            let (nested_properties, nested_required) = dto_fields(field_type, sources);
            properties.extend(nested_properties);
            required.extend(nested_required);
        } else if !serde_attribute.contains("skip_deserializing")
            && !serde_attribute.contains("serde(skip)")
        {
            let serialized_name = serde_rename(&serde_attribute).unwrap_or(name).to_owned();
            properties.insert(serialized_name.clone());
            if !serde_attribute.contains("default") && !field_type.starts_with("Option<") {
                required.insert(serialized_name);
            }
        }
        serde_attribute.clear();
    }

    (properties, required)
}

fn named_struct_body<'a>(source: &'a str, name: &str) -> &'a str {
    let marker = format!("struct {name}");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("struct `{name}` source definition"));
    let brace = source[start..]
        .find('{')
        .expect("named struct opening brace")
        + start;
    let end = matching_delimiter_end(source, brace + 1, '{', '}');
    &source[brace + 1..end]
}

fn serde_rename(attribute: &str) -> Option<&str> {
    let marker = "rename = \"";
    let start = attribute.find(marker)? + marker.len();
    let end = attribute[start..].find('"')? + start;
    Some(&attribute[start..end])
}

fn matching_delimiter_end(source: &str, start: usize, opening: char, closing: char) -> usize {
    let mut depth = 1usize;
    for (offset, character) in source[start..].char_indices() {
        if character == opening {
            depth += 1;
        } else if character == closing {
            depth -= 1;
            if depth == 0 {
                return start + offset;
            }
        }
    }
    panic!("unterminated delimiter `{opening}`")
}

fn matching_call_end(source: &str, start: usize) -> usize {
    let mut depth = 1usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, character) in source[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return start + offset;
                }
            }
            _ => {}
        }
    }
    panic!("unterminated Router::route call")
}

fn first_string_literal(call: &str) -> String {
    let start = call.find('"').expect("route path string") + 1;
    let end = call[start..].find('"').expect("route path closing quote") + start;
    call[start..end].to_owned()
}
