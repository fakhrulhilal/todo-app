//! `src/api/todos.rs`

use ntex::http::StatusCode;
use serde_json::json;
use todo_backend_ntex::domain::Todo;

use crate::{create, problem, read_json, spawn_app};

#[ntex::test]
async fn todo_lifecycle() {
    let server = spawn_app().await;

    let milk = create(&server, "  Buy milk  ").await;
    assert_eq!(milk.title, "Buy milk");
    assert!(!milk.completed);
    let dog = create(&server, "Walk the dog").await;

    let updated: Todo = read_json(
        server
            .patch(format!("/api/v1/todos/{}", milk.id))
            .send_json(&json!({ "completed": true }))
            .await
            .unwrap(),
    )
    .await;
    assert!(updated.completed);
    assert_eq!(updated.title, "Buy milk");
    assert_eq!(updated.created_at, milk.created_at);

    let list = async |status: &str| -> Vec<Todo> {
        read_json(
            server
                .get(format!("/api/v1/todos?status={status}"))
                .send()
                .await
                .unwrap(),
        )
        .await
    };
    assert_eq!(list("all").await, [updated.clone(), dog.clone()]);
    assert_eq!(list("active").await, std::slice::from_ref(&dog));
    assert_eq!(list("completed").await, std::slice::from_ref(&updated));

    let fetched: Todo = read_json(
        server
            .get(format!("/api/v1/todos/{}", milk.id))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(fetched, updated);

    let url = format!("/api/v1/todos/{}", milk.id);
    let deleted = server.delete(&url).send().await.unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let again = server.delete(&url).send().await.unwrap();
    assert_eq!(again.status(), StatusCode::NOT_FOUND);
    assert_eq!(list("all").await, [dog]);
}

#[ntex::test]
async fn invalid_titles_are_rejected() {
    let server = spawn_app().await;

    let response = server
        .post("/api/v1/todos")
        .send_json(&json!({ "title": "   " }))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = problem(response).await;
    assert_eq!(body.type_uri, "about:blank");
    assert_eq!(body.status, 422);
    assert_eq!(body.instance.as_deref(), Some("/api/v1/todos"));
    assert_eq!(body.errors["title"], ["must not be empty"]);

    let response = server
        .post("/api/v1/todos")
        .send_json(&json!({ "title": "x".repeat(201) }))
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 422);

    let milk = create(&server, "Buy milk").await;
    let response = server
        .patch(format!("/api/v1/todos/{}", milk.id))
        .send_json(&json!({ "title": "" }))
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 422);
}

#[ntex::test]
async fn unknown_todos_are_not_found() {
    let server = spawn_app().await;
    let url = "/api/v1/todos/019a0000-0000-7000-8000-000000000000";

    let response = server.get(url).send().await.unwrap();
    let body = problem(response).await;
    assert_eq!(body.status, 404);
    assert_eq!(body.instance.as_deref(), Some(url));
    let response = server
        .patch(url)
        .send_json(&json!({ "completed": true }))
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 404);
    let response = server.delete(url).send().await.unwrap();
    assert_eq!(problem(response).await.status, 404);
}
