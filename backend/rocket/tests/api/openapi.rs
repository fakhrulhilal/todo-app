//! `src/api.rs`: the OpenAPI document and the root redirect.

use std::{fs, path::Path};

use rocket::http::Status;
use serde_json::Value;

use crate::client;

#[rocket::async_test]
async fn openapi_document_is_served() {
    let client = client().await;

    let document: Value = client
        .get("/api/openapi.json")
        .dispatch()
        .await
        .into_json()
        .await
        .unwrap();
    for path in [
        "/api/v1/todos",
        "/api/v1/todos/{id}",
        "/health/live",
        "/health/ready",
    ] {
        assert!(document["paths"][path].is_object(), "{path} is documented");
    }

    // No documentation UI: that is left to OpenAPI viewers.
    let ui = client.get("/swagger-ui/").dispatch().await;
    assert_eq!(ui.status(), Status::NotFound);
}

#[rocket::async_test]
async fn root_leads_to_the_openapi_document() {
    let client = client().await;

    let root = client.get("/").dispatch().await;
    assert_eq!(root.status(), Status::TemporaryRedirect);
    assert_eq!(
        root.headers().get_one("location"),
        Some("/api/openapi.json")
    );
}

/// `contract/openapi.json` is the contract, written by the axum backend's
/// tests. This backend must serve the same document.
#[rocket::async_test]
async fn openapi_document_is_the_contract() {
    let client = client().await;

    let served: Value = client
        .get("/api/openapi.json")
        .dispatch()
        .await
        .into_json()
        .await
        .unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contract/openapi.json");
    let contract: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(served, contract);
}
