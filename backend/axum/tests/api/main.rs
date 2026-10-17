//! Drives the real application over HTTP with an in-memory database.
//!
//! One module per source file under `src/`: `todos` for `api/todos.rs`,
//! `problem` for `api/problem.rs`, and so on. What they share is here.

mod cors;
mod health;
mod openapi;
mod problem;
mod todos;

use reqwest::{Client, StatusCode, header::CONTENT_TYPE};
use serde_json::json;
use todo_backend::{
    api::problem::{PROBLEM_JSON, Problem},
    config::DatabaseConfig,
    domain::Todo,
};
use tokio::net::TcpListener;

/// Where the frontend is served from in these tests.
const WEB_ORIGIN: &str = "https://todos.example";

/// Starts the app on a free port and returns its base URL.
async fn spawn_app() -> String {
    let store = todo_backend::store::connect(&DatabaseConfig {
        url: "sqlite::memory:".to_owned(),
        max_connections: 1,
    })
    .await
    .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let app = todo_backend::app(store, WEB_ORIGIN).unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base_url
}

async fn create(client: &Client, base_url: &str, title: &str) -> Todo {
    let response = client
        .post(format!("{base_url}/api/v1/todos"))
        .json(&json!({ "title": title }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json().await.unwrap()
}

/// Checks the content type and reads the body as a problem.
async fn problem(response: reqwest::Response) -> Problem {
    assert_eq!(response.headers()[CONTENT_TYPE], PROBLEM_JSON);
    response.json().await.unwrap()
}
