//! `src/api/health.rs`

use ntex::http::{StatusCode, header::CONTENT_TYPE};
use serde_json::{Value, json};

use crate::{problem, read_json, spawn_app, text};

#[ntex::test]
async fn probes_answer_with_the_status_alone() {
    let server = spawn_app().await;

    for path in ["/health/live", "/health/ready"] {
        let response = server.get(path).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(
            response.header("cache-control").unwrap(),
            "no-store, no-cache"
        );
        let content_type = response.header(CONTENT_TYPE).unwrap().to_str().unwrap();
        assert!(content_type.starts_with("text/plain"), "{content_type}");
        assert_eq!(text(response).await, "Healthy", "{path}");
    }
}

#[ntex::test]
async fn format_json_adds_the_result_of_each_check() {
    let server = spawn_app().await;
    let detail = async |path: &str| {
        let response = server
            .get(format!("{path}?format=json"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        read_json::<Value>(response).await
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

#[ntex::test]
async fn an_unknown_format_is_a_problem() {
    let server = spawn_app().await;

    let response = server.get("/health/ready?format=xml").send().await.unwrap();
    let body = problem(response).await;
    assert_eq!(body.status, 400);
    assert_eq!(body.instance.as_deref(), Some("/health/ready"));
}
