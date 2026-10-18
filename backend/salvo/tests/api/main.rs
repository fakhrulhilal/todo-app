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
use salvo::{
    conn::{Acceptor, TcpListener},
    prelude::*,
};
use serde_json::json;
use todo_backend_salvo::{
    api::problem::{PROBLEM_JSON, Problem},
    config::DatabaseConfig,
    domain::Todo,
};

/// Where the frontend is served from in these tests.
const WEB_ORIGIN: &str = "https://todos.example";

/// Starts the app on a free port and returns its base URL.
async fn spawn_app() -> String {
    let store = todo_backend_salvo::store::connect(&DatabaseConfig {
        url: "sqlite::memory:".to_owned(),
        max_connections: 1,
        migration: true,
    })
    .await
    .unwrap();

    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let base_url = format!(
        "http://{}",
        acceptor.holdings()[0]
            .local_addr
            .clone()
            .into_std()
            .unwrap()
    );
    let app = todo_backend_salvo::app(store, WEB_ORIGIN).unwrap();
    tokio::spawn(Server::new(acceptor).serve(app));
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
