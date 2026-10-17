//! The health probes, over real HTTP. They answer as the axum backend's do.

use reqwest::{Client, StatusCode, header::CONTENT_TYPE};
use serde_json::{Value, json};
use todo_fullstack::{config::DatabaseConfig, health, store};
use tokio::net::TcpListener;

async fn spawn_probes() -> String {
    let store = store::connect(&DatabaseConfig {
        url: "sqlite::memory:".to_owned(),
        max_connections: 1,
        migration: true,
    })
    .await
    .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let app = health::router(store);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base_url
}

#[tokio::test]
async fn probes_answer_with_the_status_alone() {
    let base_url = spawn_probes().await;
    let client = Client::new();

    for path in ["/health/live", "/health/ready"] {
        let response = client
            .get(format!("{base_url}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(response.headers()["cache-control"], "no-store, no-cache");
        let content_type = response.headers()[CONTENT_TYPE].to_str().unwrap();
        assert!(content_type.starts_with("text/plain"), "{content_type}");
        assert_eq!(response.text().await.unwrap(), "Healthy", "{path}");
    }
}

#[tokio::test]
async fn format_json_adds_the_result_of_each_check() {
    let base_url = spawn_probes().await;
    let client = Client::new();
    let detail = |path: &'static str| {
        let request = client.get(format!("{base_url}{path}?format=json"));
        async move {
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            response.json::<Value>().await.unwrap()
        }
    };

    // Liveness runs no checks.
    assert_eq!(
        detail("/health/live").await,
        json!({ "status": "Healthy", "results": {} })
    );
    assert_eq!(
        detail("/health/ready").await,
        json!({
            "status": "Healthy",
            "results": {
                "database": {
                    "status": "Healthy",
                    "description": "The database answered.",
                    "data": {}
                }
            }
        })
    );
}

#[tokio::test]
async fn an_unknown_format_is_a_bad_request() {
    let base_url = spawn_probes().await;

    let response = reqwest::get(format!("{base_url}/health/ready?format=xml"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
