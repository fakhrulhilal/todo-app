//! `src/api/problem.rs`: unreadable requests and the errors actix raises
//! itself are problem details too.

use reqwest::{Client, StatusCode, header::CONTENT_TYPE};
use serde_json::json;

use crate::{create, problem, spawn_app};

#[actix_web::test]
async fn unreadable_requests_are_problems() {
    let base_url = spawn_app().await;
    let client = Client::new();
    let todos = format!("{base_url}/api/v1/todos");

    // Malformed JSON
    let response = client
        .post(&todos)
        .header(CONTENT_TYPE, "application/json")
        .body("{bad")
        .send()
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 400);

    // JSON of the wrong shape
    let response = client
        .post(&todos)
        .json(&json!({ "name": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 422);

    // Not JSON at all
    let response = client
        .post(&todos)
        .header(CONTENT_TYPE, "text/plain")
        .body("Buy milk")
        .send()
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 415);

    // Larger than the limit
    let response = client
        .post(&todos)
        .header(CONTENT_TYPE, "application/json")
        .body(format!("{{\"title\":\"{}\"}}", "x".repeat(3 * 1024 * 1024)))
        .send()
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 413);

    // A path value of the wrong type
    let response = client
        .get(format!("{todos}/not-a-uuid"))
        .send()
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 400);

    // A query value of the wrong type
    let response = client
        .get(format!("{todos}?status=sometimes"))
        .send()
        .await
        .unwrap();
    let body = problem(response).await;
    assert_eq!(body.status, 400);
    // The path only: the query string is not part of `instance`.
    assert_eq!(body.instance.as_deref(), Some("/api/v1/todos"));
}

#[actix_web::test]
async fn router_errors_are_problems() {
    let base_url = spawn_app().await;
    let client = Client::new();

    let response = client
        .put(format!("{base_url}/api/v1/todos"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["allow"], "GET,HEAD,POST");
    let body = problem(response).await;
    assert_eq!(body.status, 405);
    assert_eq!(body.instance.as_deref(), Some("/api/v1/todos"));

    let response = client
        .delete(format!("{base_url}/health/live"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["allow"], "GET,HEAD");
    assert_eq!(problem(response).await.status, 405);

    for path in [
        "/api/v2/nothing",
        "/api/v1/nothing",
        "/todos",
        "/health/live/more",
        // As in axum, a path matches as it is written: no trailing slash and
        // no empty segment is dropped.
        "/api/v1/todos/",
        "/health/live/",
        "//api/v1/todos",
        "/api//v1/todos",
        "/api/v1/todos//",
    ] {
        let response = client
            .get(format!("{base_url}{path}"))
            .send()
            .await
            .unwrap();
        let body = problem(response).await;
        assert_eq!(body.status, 404);
        assert_eq!(body.detail.as_deref(), Some("No such endpoint."));
        assert_eq!(body.instance.as_deref(), Some(path));
    }
}

#[actix_web::test]
async fn head_is_answered_like_get_without_a_body() {
    let base_url = spawn_app().await;
    let client = Client::new();
    let milk = create(&client, &base_url, "Buy milk").await;

    for path in [
        format!("/api/v1/todos/{}", milk.id),
        "/health/ready".to_owned(),
    ] {
        let response = client
            .head(format!("{base_url}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert!(response.bytes().await.unwrap().is_empty(), "{path}");
    }
}
