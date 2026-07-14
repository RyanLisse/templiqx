use serde_yaml_ng::Value;
use std::collections::BTreeSet;

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
    let mut routed = registered_routes(source);
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

fn registered_routes(source: &str) -> BTreeSet<(String, String)> {
    let mut routes = BTreeSet::new();
    let mut cursor = 0;
    while let Some(relative_start) = source[cursor..].find(".route(") {
        let start = cursor + relative_start + ".route(".len();
        let end = matching_call_end(source, start);
        let call = &source[start..end];
        let path = first_string_literal(call).replace("{*artifact}", "{artifact}");
        for method in HTTP_METHODS {
            let direct = format!("{method}(");
            let chained = format!(".{method}(");
            if call.contains(&direct) || call.contains(&chained) {
                routes.insert((method.to_uppercase(), path.clone()));
            }
        }
        cursor = end;
    }
    routes
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
