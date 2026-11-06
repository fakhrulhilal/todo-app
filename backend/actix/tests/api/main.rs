//! Drives the real application over HTTP with an in-memory database.
//!
//! One module per source file under `src/`: `todos` for `api/todos.rs`,
//! `problem` for `api/problem.rs`, and so on. What they share is here.

mod cors;
mod health;
mod openapi;
mod problem;
mod todos;

use actix_web::{HttpServer, web::Data};
use reqwest::{Client, StatusCode, header::CONTENT_TYPE};
use serde_json::json;
use todo_backend_actix::{
    CorsOrigins,
    api::{
        AppState,
        problem::{PROBLEM_JSON, Problem},
    },
    config::DatabaseConfig,
    domain::Todo,
};

/// Where the frontend is served from in these tests.
const WEB_ORIGIN: &str = "https://todos.example";

/// Starts the app on a free port and returns its base URL. One worker, and no
/// signal handling: the server stops with the test's runtime.
async fn spawn_app() -> String {
    let store = todo_backend_actix::store::connect(&DatabaseConfig {
        url: "sqlite::memory:".to_owned(),
        max_connections: 1,
        migration: true,
    })
    .await
    .unwrap();
    let state = Data::new(AppState::new(store));
    let cors = CorsOrigins::parse(WEB_ORIGIN).unwrap();

    let server = HttpServer::new(move || todo_backend_actix::app(state.clone(), cors.clone()))
        .workers(1)
        .disable_signals()
        .bind(("127.0.0.1", 0))
        .unwrap();
    let base_url = format!("http://{}", server.addrs()[0]);
    actix_web::rt::spawn(server.run());
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
