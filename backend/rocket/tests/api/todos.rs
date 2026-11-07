//! `src/api/todos.rs`

use rocket::http::Status;
use serde_json::json;
use todo_backend_rocket::domain::Todo;

use crate::{client, create, problem};

#[rocket::async_test]
async fn todo_lifecycle() {
    let client = client().await;

    let milk = create(&client, "  Buy milk  ").await;
    assert_eq!(milk.title, "Buy milk");
    assert!(!milk.completed);
    let dog = create(&client, "Walk the dog").await;

    let updated: Todo = client
        .patch(format!("/api/v1/todos/{}", milk.id))
        .json(&json!({ "completed": true }))
        .dispatch()
        .await
        .into_json()
        .await
        .unwrap();
    assert!(updated.completed);
    assert_eq!(updated.title, "Buy milk");
    assert_eq!(updated.created_at, milk.created_at);

    let list = async |status: &str| {
        client
            .get(format!("/api/v1/todos?status={status}"))
            .dispatch()
            .await
            .into_json::<Vec<Todo>>()
            .await
            .unwrap()
    };
    assert_eq!(list("all").await, [updated.clone(), dog.clone()]);
    assert_eq!(list("active").await, std::slice::from_ref(&dog));
    assert_eq!(list("completed").await, std::slice::from_ref(&updated));

    let fetched: Todo = client
        .get(format!("/api/v1/todos/{}", milk.id))
        .dispatch()
        .await
        .into_json()
        .await
        .unwrap();
    assert_eq!(fetched, updated);

    let url = format!("/api/v1/todos/{}", milk.id);
    let deleted = client.delete(&url).dispatch().await;
    assert_eq!(deleted.status(), Status::NoContent);
    let again = client.delete(&url).dispatch().await;
    assert_eq!(again.status(), Status::NotFound);
    assert_eq!(list("all").await, [dog]);
}

#[rocket::async_test]
async fn a_created_todo_says_where_it_is() {
    let client = client().await;

    let response = client
        .post("/api/v1/todos")
        .json(&json!({ "title": "Buy milk" }))
        .dispatch()
        .await;
    let location = response.headers().get_one("location").unwrap().to_owned();
    let milk: Todo = response.into_json().await.unwrap();
    assert_eq!(location, format!("/api/v1/todos/{}", milk.id));
}

#[rocket::async_test]
async fn invalid_titles_are_rejected() {
    let client = client().await;
    let todos = "/api/v1/todos";

    let response = client
        .post(todos)
        .json(&json!({ "title": "   " }))
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::UnprocessableEntity);
    let body = problem(response).await;
    assert_eq!(body.type_uri, "about:blank");
    assert_eq!(body.status, 422);
    assert_eq!(body.instance.as_deref(), Some("/api/v1/todos"));
    assert_eq!(body.errors["title"], ["must not be empty"]);

    let response = client
        .post(todos)
        .json(&json!({ "title": "x".repeat(201) }))
        .dispatch()
        .await;
    assert_eq!(problem(response).await.status, 422);

    let milk = create(&client, "Buy milk").await;
    let response = client
        .patch(format!("{todos}/{}", milk.id))
        .json(&json!({ "title": "" }))
        .dispatch()
        .await;
    assert_eq!(problem(response).await.status, 422);
}

#[rocket::async_test]
async fn unknown_todos_are_not_found() {
    let client = client().await;
    let url = "/api/v1/todos/019a0000-0000-7000-8000-000000000000";

    let response = client.get(url).dispatch().await;
    let body = problem(response).await;
    assert_eq!(body.status, 404);
    assert_eq!(body.instance.as_deref(), Some(url));
    let response = client
        .patch(url)
        .json(&json!({ "completed": true }))
        .dispatch()
        .await;
    assert_eq!(problem(response).await.status, 404);
    let response = client.delete(url).dispatch().await;
    assert_eq!(problem(response).await.status, 404);
}
