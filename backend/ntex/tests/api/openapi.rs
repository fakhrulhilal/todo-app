//! `src/api.rs`: the OpenAPI document and the root redirect.

use std::{fs, path::Path};

use ntex::http::StatusCode;
use serde_json::Value;

use crate::{read_json, spawn_app};

/// `contract/openapi.json` is the contract, written by the axum backend's
/// tests. This backend must serve the same document.
#[ntex::test]
async fn openapi_document_is_the_contract() {
    let server = spawn_app().await;

    let served: Value = read_json(server.get("/api/openapi.json").send().await.unwrap()).await;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contract/openapi.json");
    let contract: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(served, contract);

    // No documentation UI: that is left to OpenAPI viewers.
    let ui = server.get("/swagger-ui/").send().await.unwrap();
    assert_eq!(ui.status(), StatusCode::NOT_FOUND);
}

#[ntex::test]
async fn root_leads_to_the_openapi_document() {
    let server = spawn_app().await;

    let root = server.get("/").send().await.unwrap();
    assert_eq!(root.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(root.header("location").unwrap(), "/api/openapi.json");
}
