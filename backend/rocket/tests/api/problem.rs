//! `src/api/problem.rs` and `src/api/extract.rs`: unreadable requests and
//! the errors Rocket raises itself are problem details too.

use rocket::http::{ContentType, Status};
use serde_json::json;

use crate::{client, create, problem};

#[rocket::async_test]
async fn unreadable_requests_are_problems() {
    let client = client().await;
    let todos = "/api/v1/todos";

    // Malformed JSON
    let response = client
        .post(todos)
        .header(ContentType::JSON)
        .body("{bad")
        .dispatch()
        .await;
    let body = problem(response).await;
    assert_eq!(body.status, 400);
    assert!(
        body.detail
            .unwrap()
            .starts_with("Failed to parse the request body as JSON")
    );

    // JSON of the wrong shape
    let response = client
        .post(todos)
        .json(&json!({ "name": "x" }))
        .dispatch()
        .await;
    assert_eq!(problem(response).await.status, 422);

    // Not JSON at all
    let response = client
        .post(todos)
        .header(ContentType::Plain)
        .body("Buy milk")
        .dispatch()
        .await;
    assert_eq!(problem(response).await.status, 415);

    // Larger than the limit
    let response = client
        .post(todos)
        .header(ContentType::JSON)
        .body(format!("{{\"title\":\"{}\"}}", "x".repeat(3 * 1024 * 1024)))
        .dispatch()
        .await;
    assert_eq!(problem(response).await.status, 413);

    // A path value of the wrong type
    let response = client.get(format!("{todos}/not-a-uuid")).dispatch().await;
    assert_eq!(problem(response).await.status, 400);

    // A query value of the wrong type
    let response = client
        .get(format!("{todos}?status=sometimes"))
        .dispatch()
        .await;
    let body = problem(response).await;
    assert_eq!(body.status, 400);
    // The path only: the query string is not part of `instance`.
    assert_eq!(body.instance.as_deref(), Some("/api/v1/todos"));
}

#[rocket::async_test]
async fn router_errors_are_problems() {
    let client = client().await;

    let response = client.put("/api/v1/todos").dispatch().await;
    assert_eq!(response.headers().get_one("allow"), Some("GET,HEAD,POST"));
    let body = problem(response).await;
    assert_eq!(body.status, 405);
    assert_eq!(body.instance.as_deref(), Some("/api/v1/todos"));

    let response = client.delete("/health/live").dispatch().await;
    assert_eq!(response.headers().get_one("allow"), Some("GET,HEAD"));
    assert_eq!(problem(response).await.status, 405);

    for path in [
        "/api/v2/nothing",
        "/api/v1/nothing",
        "/todos",
        "/health/live/more",
    ] {
        let response = client.get(path).dispatch().await;
        let body = problem(response).await;
        assert_eq!(body.status, 404, "{path}");
        assert_eq!(body.detail.as_deref(), Some("No such endpoint."), "{path}");
        assert_eq!(body.instance.as_deref(), Some(path));
    }
}

/// Rocket matches a path by its segments, leaving out empty ones: a trailing
/// slash or a doubled one leads to the same route (axum answers 404).
#[rocket::async_test]
async fn empty_segments_lead_to_the_same_route() {
    let client = client().await;

    for path in [
        "/api/v1/todos/",
        "//api/v1/todos",
        "/api//v1/todos",
        "/api/v1/todos//",
    ] {
        let response = client.get(path).dispatch().await;
        assert_eq!(response.status(), Status::Ok, "{path}");
        assert_eq!(
            response.into_string().await.as_deref(),
            Some("[]"),
            "{path}"
        );
    }
    let response = client.get("/health/live/").dispatch().await;
    assert_eq!(response.into_string().await.as_deref(), Some("Healthy"));
}

#[rocket::async_test]
async fn head_is_answered_like_get_without_a_body() {
    let client = client().await;
    let milk = create(&client, "Buy milk").await;

    for path in [
        format!("/api/v1/todos/{}", milk.id),
        "/health/ready".to_owned(),
    ] {
        let response = client.head(&path).dispatch().await;
        assert_eq!(response.status(), Status::Ok, "{path}");
        assert!(
            response.into_bytes().await.unwrap_or_default().is_empty(),
            "{path}"
        );
    }
}

#[rocket::async_test]
async fn a_known_path_with_another_method_is_405_for_every_route() {
    let client = client().await;

    let response = client.post("/health/ready").dispatch().await;
    assert_eq!(response.headers().get_one("allow"), Some("GET,HEAD"));
    assert_eq!(problem(response).await.status, 405);

    let response = client
        .put("/api/v1/todos/019a0000-0000-7000-8000-000000000000")
        .dispatch()
        .await;
    assert_eq!(
        response.headers().get_one("allow"),
        Some("GET,HEAD,PATCH,DELETE")
    );
    assert_eq!(problem(response).await.status, 405);

    let response = client.post("/").dispatch().await;
    assert_eq!(response.headers().get_one("allow"), Some("GET,HEAD"));
    assert_eq!(problem(response).await.status, 405);
}
