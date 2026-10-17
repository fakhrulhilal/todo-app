//! `src/api/todos.rs`

use reqwest::{Client, StatusCode};
use serde_json::json;
use todo_api::domain::Todo;

use crate::{create, problem, spawn_app};

#[tokio::test]
async fn todo_lifecycle() {
    let base_url = spawn_app().await;
    let client = Client::new();

    let milk = create(&client, &base_url, "  Buy milk  ").await;
    assert_eq!(milk.title, "Buy milk");
    assert!(!milk.completed);
    let dog = create(&client, &base_url, "Walk the dog").await;

    let updated: Todo = client
        .patch(format!("{base_url}/api/v1/todos/{}", milk.id))
        .json(&json!({ "completed": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(updated.completed);
    assert_eq!(updated.title, "Buy milk");
    assert_eq!(updated.created_at, milk.created_at);

    let list = |status: &'static str| {
        let request = client.get(format!("{base_url}/api/v1/todos?status={status}"));
        async move {
            request
                .send()
                .await
                .unwrap()
                .json::<Vec<Todo>>()
                .await
                .unwrap()
        }
    };
    assert_eq!(list("all").await, [updated.clone(), dog.clone()]);
    assert_eq!(list("active").await, std::slice::from_ref(&dog));
    assert_eq!(list("completed").await, std::slice::from_ref(&updated));

    let fetched: Todo = client
        .get(format!("{base_url}/api/v1/todos/{}", milk.id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(fetched, updated);

    let url = format!("{base_url}/api/v1/todos/{}", milk.id);
    let deleted = client.delete(&url).send().await.unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let again = client.delete(&url).send().await.unwrap();
    assert_eq!(again.status(), StatusCode::NOT_FOUND);
    assert_eq!(list("all").await, [dog]);
}

#[tokio::test]
async fn invalid_titles_are_rejected() {
    let base_url = spawn_app().await;
    let client = Client::new();
    let todos = format!("{base_url}/api/v1/todos");

    let response = client
        .post(&todos)
        .json(&json!({ "title": "   " }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = problem(response).await;
    assert_eq!(body.type_uri, "about:blank");
    assert_eq!(body.status, 422);
    assert_eq!(body.instance.as_deref(), Some("/api/v1/todos"));
    assert_eq!(body.errors["title"], ["must not be empty"]);

    let response = client
        .post(&todos)
        .json(&json!({ "title": "x".repeat(201) }))
        .send()
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 422);

    let milk = create(&client, &base_url, "Buy milk").await;
    let response = client
        .patch(format!("{todos}/{}", milk.id))
        .json(&json!({ "title": "" }))
        .send()
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 422);
}

#[tokio::test]
async fn unknown_todos_are_not_found() {
    let base_url = spawn_app().await;
    let client = Client::new();
    let url = format!("{base_url}/api/v1/todos/019a0000-0000-7000-8000-000000000000");

    let response = client.get(&url).send().await.unwrap();
    let body = problem(response).await;
    assert_eq!(body.status, 404);
    assert_eq!(
        body.instance.as_deref(),
        Some("/api/v1/todos/019a0000-0000-7000-8000-000000000000")
    );
    let response = client
        .patch(&url)
        .json(&json!({ "completed": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(problem(response).await.status, 404);
    let response = client.delete(&url).send().await.unwrap();
    assert_eq!(problem(response).await.status, 404);
}
