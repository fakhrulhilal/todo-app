//! `src/api/problem.rs`: the errors ntex raises itself are problem details too.

use ntex::http::{StatusCode, header::CONTENT_TYPE};
use serde_json::json;

use crate::{create, problem, spawn_app};

#[ntex::test]
async fn unreadable_requests_are_problems() {
    let server = spawn_app().await;

    // Malformed JSON
    let response = server
        .post("/api/v1/todos")
        .header(CONTENT_TYPE, "application/json")
        .send_body("{bad")
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 400);

    // JSON of the wrong shape
    let response = server
        .post("/api/v1/todos")
        .send_json(&json!({ "name": "x" }))
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 422);

    // Not JSON at all
    let response = server
        .post("/api/v1/todos")
        .header(CONTENT_TYPE, "text/plain")
        .send_body("Buy milk")
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 415);

    // A path value of the wrong type
    let response = server.get("/api/v1/todos/not-a-uuid").send().await.unwrap();
    assert_eq!(problem(response).await.status, 400);

    // A query value of the wrong type
    let response = server
        .get("/api/v1/todos?status=sometimes")
        .send()
        .await
        .unwrap();
    let body = problem(response).await;
    assert_eq!(body.status, 400);
    // The path only: the query string is not part of `instance`.
    assert_eq!(body.instance.as_deref(), Some("/api/v1/todos"));
}

#[ntex::test]
async fn router_errors_are_problems() {
    let server = spawn_app().await;

    let response = server.put("/api/v1/todos").send().await.unwrap();
    assert_eq!(response.header("allow").unwrap(), "GET,HEAD,POST");
    let body = problem(response).await;
    assert_eq!(body.status, 405);
    assert_eq!(body.instance.as_deref(), Some("/api/v1/todos"));

    for path in ["/api/v2/nothing", "/api/v1/nothing", "/todos"] {
        let response = server.get(path).send().await.unwrap();
        let body = problem(response).await;
        assert_eq!(body.status, 404);
        assert_eq!(body.detail.as_deref(), Some("No such endpoint."));
        assert_eq!(body.instance.as_deref(), Some(path));
    }
}

#[ntex::test]
async fn head_is_answered_like_get_without_a_body() {
    let server = spawn_app().await;
    let milk = create(&server, "Buy milk").await;

    let response = server
        .head(format!("/api/v1/todos/{}", milk.id))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.body().await.unwrap().is_empty());
}
