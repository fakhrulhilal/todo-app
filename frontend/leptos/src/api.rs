//! Calls to the JSON API. Failures are messages ready to show to the user.

use std::collections::BTreeMap;

use gloo_net::http::{Request, Response};
use serde::{Deserialize, de::DeserializeOwned};
use todo_contract::{CreateTodo, StatusFilter, Todo, UpdateTodo};
use uuid::Uuid;

/// Where the API lives. A static site cannot be configured when it starts,
/// so this is fixed when it is built: `TODO_API_URL=https://... trunk build`.
/// Empty means the address the site itself is served from.
const API_URL: &str = match option_env!("TODO_API_URL") {
    Some(url) => url,
    None => "http://127.0.0.1:8080",
};

const UNREACHABLE: &str = "The todo service is unreachable.";

fn url(path: &str) -> String {
    format!("{}{path}", API_URL.trim_end_matches('/'))
}

fn todo_url(id: Uuid) -> String {
    url(&format!("/api/v1/todos/{id}"))
}

/// The todos under a filter. Used as a leptos-fetch query, keyed by the filter.
pub async fn list(filter: StatusFilter) -> Result<Vec<Todo>, String> {
    let request = Request::get(&url(&format!("/api/v1/todos?status={filter}"))).build();
    json(send(request).await?).await
}

pub async fn create(input: CreateTodo) -> Result<Todo, String> {
    let request = Request::post(&url("/api/v1/todos")).json(&input);
    json(send(request).await?).await
}

pub async fn update(id: Uuid, changes: UpdateTodo) -> Result<Todo, String> {
    let request = Request::patch(&todo_url(id)).json(&changes);
    json(send(request).await?).await
}

pub async fn delete(id: Uuid) -> Result<(), String> {
    send(Request::delete(&todo_url(id)).build()).await?;
    Ok(())
}

async fn send(request: Result<Request, gloo_net::Error>) -> Result<Response, String> {
    let response = match request {
        Ok(request) => request.send().await,
        Err(error) => Err(error),
    }
    .map_err(|_| UNREACHABLE.to_owned())?;

    if response.ok() {
        Ok(response)
    } else {
        Err(problem_message(response).await)
    }
}

async fn json<T: DeserializeOwned>(response: Response) -> Result<T, String> {
    response
        .json()
        .await
        .map_err(|_| "The todo service sent an unexpected answer.".to_owned())
}

/// The parts of an RFC 9457 problem that are shown to the user.
#[derive(Default, Deserialize)]
struct Problem {
    title: Option<String>,
    detail: Option<String>,
    /// What is wrong with each invalid member of the request, by name.
    #[serde(default)]
    errors: BTreeMap<String, Vec<String>>,
}

async fn problem_message(response: Response) -> String {
    let status = response.status();
    // Not a problem document: fall through to the generic message.
    let problem: Problem = response.json().await.unwrap_or_default();

    let invalid: Vec<String> = problem
        .errors
        .iter()
        .flat_map(|(member, details)| {
            details
                .iter()
                .map(move |detail| format!("{member} {detail}"))
        })
        .collect();
    if !invalid.is_empty() {
        return invalid.join("; ");
    }
    problem
        .detail
        .or(problem.title)
        .unwrap_or_else(|| format!("Request failed ({status})."))
}
