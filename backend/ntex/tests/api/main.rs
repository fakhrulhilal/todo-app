//! Drives the real application over HTTP with an in-memory database.
//!
//! One module per source file under `src/`: `todos` for `api/todos.rs`,
//! `problem` for `api/problem.rs`, and so on. What they share is here.

// The test server nests the app in more service types than the default limit allows.
#![recursion_limit = "256"]

mod cors;
mod health;
mod openapi;
mod problem;
mod todos;

use ntex::{
    client::ClientResponse,
    http::{StatusCode, header::CONTENT_TYPE},
    web::test::{self, TestServer},
};
use serde::de::DeserializeOwned;
use serde_json::json;
use todo_backend_ntex::{
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

/// Starts the app on a free port. It stops when the server is dropped.
async fn spawn_app() -> TestServer {
    let store = todo_backend_ntex::store::connect(&DatabaseConfig {
        url: "sqlite::memory:".to_owned(),
        max_connections: 1,
        migration: true,
    })
    .await
    .unwrap();
    let state = AppState::new(store);
    let cors = CorsOrigins::parse(WEB_ORIGIN).unwrap();

    // The test server has no state of its own: the app gets ours.
    test::server(async move |_| {
        ntex::service::map_state_factory(state.clone(), todo_backend_ntex::app(&cors))
    })
}

/// Reads a JSON body. The OpenAPI document is larger than the client's default limit.
async fn read_json<T: DeserializeOwned>(response: ClientResponse) -> T {
    response.json().limit(1024 * 1024).await.unwrap()
}

async fn text(response: ClientResponse) -> String {
    String::from_utf8(response.body().await.unwrap().to_vec()).unwrap()
}

async fn create(server: &TestServer, title: &str) -> Todo {
    let response = server
        .post("/api/v1/todos")
        .send_json(&json!({ "title": title }))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    read_json(response).await
}

/// Checks the content type and reads the body as a problem.
async fn problem(response: ClientResponse) -> Problem {
    assert_eq!(response.header(CONTENT_TYPE).unwrap(), PROBLEM_JSON);
    read_json(response).await
}
