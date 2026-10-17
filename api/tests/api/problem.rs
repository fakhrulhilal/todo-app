//! `src/api/problem.rs` and `src/api/extract.rs`: the errors axum raises
//! itself are problem details too.

use reqwest::{Client, header::CONTENT_TYPE};

use crate::{problem, spawn_app};

#[tokio::test]
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

#[tokio::test]
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

    for path in ["/api/v2/nothing", "/todos"] {
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
