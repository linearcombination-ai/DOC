//! Response bodies, byte-compatible with DOC's FastAPI (checked by the parity run).

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use utoipa::ToSchema;

use crate::finding::{Finding, Mode};

/// `{"status":"ok"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct HealthDetail {
    pub status: String,
}

/// DOC's `health_status` returns a Python tuple, which FastAPI serialises as a JSON array:
/// `[{"status":"ok"},200]` (RAN against the oracle). Kept as is: a client may read index 1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HealthResponse(pub HealthDetail, pub u16);

/// `[code, "national (english)", is_gateway]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LanguageEntry(pub String, pub String, pub bool);

/// `[book code, name]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct BookEntry(pub String, pub String);

/// `[resource type code, resource type name]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct ResourceTypeEntry(pub String, pub String);

// utoipa's derive for a tuple struct with mixed element types emits `items: {type: object}`, which
// loses the types the frontend needs (RAN: the generated document, see the report). These impls
// write the OpenAPI 3.1 `prefixItems` form, so `openapi-typescript` produces `[string, string, boolean]`.
mod tuple_schemas {
    use super::*;
    use utoipa::openapi::schema::{ArrayBuilder, ArrayItems, ObjectBuilder, Schema, Type};
    use utoipa::openapi::RefOr;
    use utoipa::__dev::ComposeSchema;

    fn string() -> Schema {
        ObjectBuilder::new().schema_type(Type::String).build().into()
    }
    fn boolean() -> Schema {
        ObjectBuilder::new().schema_type(Type::Boolean).build().into()
    }
    fn status_code() -> Schema {
        ObjectBuilder::new().schema_type(Type::Integer).minimum(Some(100)).maximum(Some(599)).build().into()
    }
    fn health_detail() -> Schema {
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .property("status", string())
            .required("status")
            .build()
            .into()
    }
    fn tuple(items: Vec<Schema>, description: &str) -> RefOr<Schema> {
        let n = items.len();
        RefOr::T(
            ArrayBuilder::new()
                .prefix_items(items)
                .items(ArrayItems::False)
                .min_items(Some(n))
                .max_items(Some(n))
                .description(Some(description))
                .build()
                .into(),
        )
    }
    impl ComposeSchema for LanguageEntry {
        fn compose(_generics: Vec<RefOr<Schema>>) -> RefOr<Schema> {
            tuple(vec![string(), string(), boolean()], "`[language code, \"national (english)\", is_gateway]`")
        }
    }
    impl ComposeSchema for BookEntry {
        fn compose(_generics: Vec<RefOr<Schema>>) -> RefOr<Schema> {
            tuple(vec![string(), string()], "`[book code, book name]`")
        }
    }
    impl ComposeSchema for ResourceTypeEntry {
        fn compose(_generics: Vec<RefOr<Schema>>) -> RefOr<Schema> {
            tuple(vec![string(), string()], "`[resource type code, resource type name]`")
        }
    }
    impl ComposeSchema for HealthResponse {
        fn compose(_generics: Vec<RefOr<Schema>>) -> RefOr<Schema> {
            tuple(vec![health_detail(), status_code()], "`[{\"status\": \"ok\"}, 200]`: DOC returns a Python tuple, which FastAPI serialises as an array")
        }
    }
    impl ToSchema for LanguageEntry {}
    impl ToSchema for BookEntry {}
    impl ToSchema for ResourceTypeEntry {}
    impl ToSchema for HealthResponse {}
}

/// `{book: [1..N]}` in canonical book order (an object, but the key order is part of the
/// behaviour: DOC's dict keeps insertion order and the UI iterates it).
#[derive(Clone, Debug, PartialEq, Eq, ToSchema)]
#[schema(value_type = std::collections::BTreeMap<String, Vec<u32>>)]
pub struct ChaptersInBooks(pub Vec<(String, Vec<u32>)>);

impl Serialize for ChaptersInBooks {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (book, chapters) in &self.0 {
            map.serialize_entry(book, chapters)?;
        }
        map.end()
    }
}

/// A list endpoint's answer with the findings the fixed mode returns (the `/findings/...` routes).
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct WithFindings<T> {
    pub data: T,
    pub findings: Vec<Finding>,
    pub mode: Mode,
}
