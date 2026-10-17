use axum::{extract::State, http::StatusCode};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{
    AppState,
    extract::{Json, Path, Query},
    problem::{ApiError, PROBLEM_JSON, Problem},
};
use crate::domain::{CreateTodo, ListTodos, Todo, UpdateTodo, new_todo, validate_title};

pub const TAG: &str = "todos";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_todos, create_todo))
        .routes(routes!(get_todo, update_todo, delete_todo))
}

fn not_found(id: Uuid) -> ApiError {
    ApiError::NotFound(format!("Todo {id} does not exist."))
}

/// List todos
///
/// Returns todos oldest first.
#[utoipa::path(
    get,
    path = "/todos",
    tag = TAG,
    params(ListTodos),
    responses(
        (status = 200, description = "The matching todos", body = [Todo]),
        (status = 400, description = "Invalid query", body = Problem, content_type = PROBLEM_JSON),
        (status = 500, description = "Unexpected failure", body = Problem, content_type = PROBLEM_JSON),
    )
)]
async fn list_todos(
    State(state): State<AppState>,
    Query(query): Query<ListTodos>,
) -> Result<axum::Json<Vec<Todo>>, ApiError> {
    Ok(axum::Json(state.store.list(query.status).await?))
}

/// Create a todo
#[utoipa::path(
    post,
    path = "/todos",
    tag = TAG,
    request_body = CreateTodo,
    responses(
        (status = 201, description = "The created todo", body = Todo),
        (status = 400, description = "Malformed request", body = Problem, content_type = PROBLEM_JSON),
        (status = 422, description = "Validation failed", body = Problem, content_type = PROBLEM_JSON),
        (status = 500, description = "Unexpected failure", body = Problem, content_type = PROBLEM_JSON),
    )
)]
async fn create_todo(
    State(state): State<AppState>,
    Json(input): Json<CreateTodo>,
) -> Result<(StatusCode, axum::Json<Todo>), ApiError> {
    let title =
        validate_title(&input.title).map_err(|detail| ApiError::invalid("title", detail))?;
    let todo = new_todo(title);
    state.store.insert(&todo).await?;
    Ok((StatusCode::CREATED, axum::Json(todo)))
}

/// Get a todo
#[utoipa::path(
    get,
    path = "/todos/{id}",
    tag = TAG,
    params(("id" = Uuid, Path, description = "Todo id")),
    responses(
        (status = 200, description = "The todo", body = Todo),
        (status = 400, description = "Invalid id", body = Problem, content_type = PROBLEM_JSON),
        (status = 404, description = "No such todo", body = Problem, content_type = PROBLEM_JSON),
        (status = 500, description = "Unexpected failure", body = Problem, content_type = PROBLEM_JSON),
    )
)]
async fn get_todo(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<axum::Json<Todo>, ApiError> {
    let todo = state.store.get(id).await?.ok_or_else(|| not_found(id))?;
    Ok(axum::Json(todo))
}

/// Update a todo
///
/// Changes the title, the completed flag, or both.
#[utoipa::path(
    patch,
    path = "/todos/{id}",
    tag = TAG,
    params(("id" = Uuid, Path, description = "Todo id")),
    request_body = UpdateTodo,
    responses(
        (status = 200, description = "The updated todo", body = Todo),
        (status = 400, description = "Malformed request", body = Problem, content_type = PROBLEM_JSON),
        (status = 404, description = "No such todo", body = Problem, content_type = PROBLEM_JSON),
        (status = 422, description = "Validation failed", body = Problem, content_type = PROBLEM_JSON),
        (status = 500, description = "Unexpected failure", body = Problem, content_type = PROBLEM_JSON),
    )
)]
async fn update_todo(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(mut changes): Json<UpdateTodo>,
) -> Result<axum::Json<Todo>, ApiError> {
    if let Some(title) = &changes.title {
        changes.title =
            Some(validate_title(title).map_err(|detail| ApiError::invalid("title", detail))?);
    }
    let todo = state
        .store
        .update(id, &changes)
        .await?
        .ok_or_else(|| not_found(id))?;
    Ok(axum::Json(todo))
}

/// Delete a todo
#[utoipa::path(
    delete,
    path = "/todos/{id}",
    tag = TAG,
    params(("id" = Uuid, Path, description = "Todo id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 400, description = "Invalid id", body = Problem, content_type = PROBLEM_JSON),
        (status = 404, description = "No such todo", body = Problem, content_type = PROBLEM_JSON),
        (status = 500, description = "Unexpected failure", body = Problem, content_type = PROBLEM_JSON),
    )
)]
async fn delete_todo(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if !state.store.delete(id).await? {
        return Err(not_found(id));
    }
    Ok(StatusCode::NO_CONTENT)
}
