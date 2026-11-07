//! Drives the real application with an in-memory database, through Rocket's
//! local client: every request goes through the same routing, guards,
//! catchers and fairings as one over the network, without a socket.
//!
//! One module per source file under `src/`: `todos` for `api/todos.rs`,
//! `problem` for `api/problem.rs`, and so on. What they share is here.

mod cors;
mod health;
mod openapi;
mod problem;
mod todos;

use rocket::{
    http::{ContentType, Status},
    local::asynchronous::{Client, LocalResponse},
};
use serde_json::json;
use todo_backend_rocket::{
    api::problem::{PROBLEM_JSON, Problem},
    cli::Cli,
    config::{Config, DatabaseConfig},
    domain::Todo,
};

/// Where the frontend is served from in these tests.
const WEB_ORIGIN: &str = "https://todos.example";

/// The app with the built-in defaults, an in-memory database and the web
/// client's origin as the only CORS origin.
async fn client() -> Client {
    let mut config = Config::load(&Cli::default()).unwrap();
    config.database = DatabaseConfig {
        url: "sqlite::memory:".to_owned(),
        max_connections: 1,
        migration: true,
    };
    config.server.cors_origins = WEB_ORIGIN.to_owned();
    let rocket = todo_backend_rocket::rocket(&config).unwrap();
    Client::untracked(rocket).await.unwrap()
}

async fn create(client: &Client, title: &str) -> Todo {
    let response = client
        .post("/api/v1/todos")
        .json(&json!({ "title": title }))
        .dispatch()
        .await;
    assert_eq!(response.status(), Status::Created);
    response.into_json().await.unwrap()
}

/// Checks the content type and reads the body as a problem.
async fn problem(response: LocalResponse<'_>) -> Problem {
    assert_eq!(
        response.content_type(),
        Some(ContentType::new("application", "problem+json")),
        "{PROBLEM_JSON}"
    );
    response.into_json().await.unwrap()
}
