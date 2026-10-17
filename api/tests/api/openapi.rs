//! `src/api.rs`: the OpenAPI document, Swagger UI and the root redirect.

use reqwest::{Client, StatusCode};
use serde_json::Value;

use crate::spawn_app;

#[tokio::test]
async fn openapi_document_and_swagger_ui() {
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

    let ui = client
        .get(format!("{base_url}/swagger-ui/"))
        .send()
        .await
        .unwrap();
    assert_eq!(ui.status(), StatusCode::OK);
}

#[tokio::test]
async fn root_leads_to_the_api_docs() {
    let base_url = spawn_app().await;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let root = client.get(&base_url).send().await.unwrap();
    assert_eq!(root.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(root.headers()["location"], "/swagger-ui");
}
