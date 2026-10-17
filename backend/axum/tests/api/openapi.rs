//! `src/api.rs`: the OpenAPI document and the root redirect.

use std::{fs, path::Path};

use reqwest::{Client, StatusCode};
use serde_json::Value;

use crate::spawn_app;

#[tokio::test]
async fn openapi_document_is_served() {
    let base_url = spawn_app().await;
    let client = Client::new();

    let document: Value = client
        .get(format!("{base_url}/api/openapi.json"))
        .send()
        .await
        .unwrap()
        .json()
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
    let ui = client
        .get(format!("{base_url}/swagger-ui/"))
        .send()
        .await
        .unwrap();
    assert_eq!(ui.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn root_leads_to_the_openapi_document() {
    let base_url = spawn_app().await;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let root = client.get(&base_url).send().await.unwrap();
    assert_eq!(root.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(root.headers()["location"], "/api/openapi.json");
}

/// The document is the contract other implementations are written against, so
/// `cargo test` keeps a copy of what this backend serves in `contract/`.
/// Commit it when it changes.
#[tokio::test]
async fn openapi_document_is_written_to_the_contract() {
    let base_url = spawn_app().await;

    let document: Value = Client::new()
        .get(format!("{base_url}/api/openapi.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut json = serde_json::to_string_pretty(&document).unwrap();
    json.push('\n');

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contract/openapi.json");
    if fs::read_to_string(&path).ok().as_deref() != Some(json.as_str()) {
        fs::write(&path, json).unwrap();
    }
}
