use serde_json::{Map, Value};
use templiqx_ports::{AuthorizedQueryPort, AuthorizedQueryRequest, DataIntrospectPort, PortError};

#[derive(Debug, Clone)]
pub struct FakeDataAccess {
    fixture: Value,
}

impl FakeDataAccess {
    pub fn from_fixture_json(source: &str) -> Result<Self, PortError> {
        let fixture: Value = serde_json::from_str(source)
            .map_err(|error| PortError::InvalidData(format!("data fixture: {error}")))?;
        let object = fixture
            .as_object()
            .ok_or_else(|| PortError::InvalidData("data fixture must be an object".into()))?;

        for key in ["schema", "authorized_scopes", "rows"] {
            if !object.contains_key(key) {
                return Err(PortError::InvalidData(format!(
                    "data fixture is missing '{key}'"
                )));
            }
        }

        Ok(Self { fixture })
    }

    fn has_scope(&self, actor: &str, entity: &str) -> Result<bool, PortError> {
        let scopes = self
            .fixture
            .get("authorized_scopes")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                PortError::InvalidData("data fixture 'authorized_scopes' must be an array".into())
            })?;

        for scope in scopes {
            let scope = scope.as_object().ok_or_else(|| {
                PortError::InvalidData("data fixture scope must be an object".into())
            })?;
            let scope_actor = scope.get("actor").and_then(Value::as_str).ok_or_else(|| {
                PortError::InvalidData("data fixture scope actor must be a string".into())
            })?;
            let entities = scope
                .get("entities")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    PortError::InvalidData("data fixture scope entities must be an array".into())
                })?;

            if scope_actor == actor
                && entities
                    .iter()
                    .any(|allowed| allowed.as_str() == Some(entity))
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn entity_rows(&self, entity: &str) -> Result<&[Value], PortError> {
        self.fixture
            .get("rows")
            .and_then(Value::as_object)
            .ok_or_else(|| PortError::InvalidData("data fixture 'rows' must be an object".into()))?
            .get(entity)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                PortError::InvalidData(format!(
                    "data fixture has no row array for entity '{entity}'"
                ))
            })
    }
}

impl DataIntrospectPort for FakeDataAccess {
    fn describe_schema(&self, _actor: &str) -> Result<Value, PortError> {
        self.fixture
            .get("schema")
            .cloned()
            .ok_or_else(|| PortError::InvalidData("data fixture is missing 'schema'".into()))
    }
}

impl AuthorizedQueryPort for FakeDataAccess {
    fn query(&self, request: &AuthorizedQueryRequest) -> Result<Value, PortError> {
        if !self.has_scope(&request.actor, &request.entity)? {
            return Err(PortError::Unsupported(format!(
                "actor '{}' has no query scope for entity '{}'",
                request.actor, request.entity
            )));
        }

        let filter = request.filter.as_object().ok_or_else(|| {
            PortError::InvalidData("authorized query filter must be an object".into())
        })?;
        let mut rows = Vec::new();

        for row in self.entity_rows(&request.entity)? {
            let row = row.as_object().ok_or_else(|| {
                PortError::InvalidData(format!(
                    "data fixture row for entity '{}' must be an object",
                    request.entity
                ))
            })?;
            if !filter
                .iter()
                .all(|(field, expected)| row.get(field) == Some(expected))
            {
                continue;
            }

            let mut selected = Map::new();
            for field in &request.fields {
                let value = row.get(field).ok_or_else(|| {
                    PortError::InvalidData(format!(
                        "data fixture row for entity '{}' has no field '{field}'",
                        request.entity
                    ))
                })?;
                selected.insert(field.clone(), value.clone());
            }
            rows.push(Value::Object(selected));
        }

        let mut response = Map::new();
        if let Some(context) = self.fixture.get("@odata.context") {
            response.insert("@odata.context".into(), context.clone());
        }
        response.insert("value".into(), Value::Array(rows));
        Ok(Value::Object(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FIXTURE: &str = include_str!(
        "../../../examples/packages/basenet-legal/fixtures/authorized-query-response.json"
    );

    fn fake() -> FakeDataAccess {
        FakeDataAccess::from_fixture_json(FIXTURE).expect("synthetic fixture should parse")
    }

    #[test]
    fn introspect_returns_shape_without_entity_rows() {
        let schema = fake()
            .describe_schema("synthetic-legal-author")
            .expect("schema should be available");

        assert_eq!(
            schema,
            json!({
                "entities": [{
                    "name": "legal_matters",
                    "fields": [
                        {"name": "matter_id", "type": "Edm.String"},
                        {"name": "title", "type": "Edm.String"},
                        {"name": "status", "type": "Edm.String"}
                    ]
                }]
            })
        );
        assert!(schema.get("rows").is_none());
        assert!(!schema.to_string().contains("SYN-MATTER"));
    }

    #[test]
    fn authorized_query_returns_deterministic_rows() {
        let fake = fake();
        let request = AuthorizedQueryRequest {
            actor: "synthetic-legal-author".into(),
            entity: "legal_matters".into(),
            fields: vec!["matter_id".into(), "title".into()],
            filter: json!({}),
        };

        let first = fake.query(&request).expect("query should succeed");
        let second = fake.query(&request).expect("query should repeat");

        assert_eq!(first, second);
        assert_eq!(
            first,
            json!({
                "@odata.context": "https://synthetic.invalid/odata/$metadata#legal_matters",
                "value": [
                    {
                        "matter_id": "SYN-MATTER-001",
                        "title": "Synthetic Lease Review"
                    },
                    {
                        "matter_id": "SYN-MATTER-002",
                        "title": "Synthetic Filing Example"
                    }
                ]
            })
        );
    }

    #[test]
    fn query_without_matching_scope_fails_closed() {
        let error = fake()
            .query(&AuthorizedQueryRequest {
                actor: "synthetic-unscoped-actor".into(),
                entity: "legal_matters".into(),
                fields: vec!["matter_id".into()],
                filter: json!({}),
            })
            .expect_err("missing scope must reject the query");

        assert!(matches!(error, PortError::Unsupported(_)));
        assert!(error.to_string().contains("no query scope"));
    }
}
