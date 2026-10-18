use salvo::{http::StatusCode, prelude::*};
use uuid::Uuid;

use super::{
    extract::{json, query, uuid_param},
    problem::{ApiError, MethodNotAllowed, PROBLEM_JSON, Problem},
    state,
};
use crate::domain::{CreateTodo, ListTodos, Todo, UpdateTodo, new_todo, validate_title};

pub const TAG: &str = "todos";

/// Routes under `/api/v1`. Each path ends in a child without a method filter,
/// so a wrong method gets its 405 with `Allow`. GET routes answer HEAD too,
/// as axum's do; Salvo leaves the body out.
pub fn router() -> Router {
    Router::with_path("todos")
        .get(list_todos)
        .head(list_todos)
        .post(create_todo)
        .push(
            Router::with_path("{id}")
                .get(get_todo)
                .head(get_todo)
                .patch(update_todo)
                .delete(delete_todo)
                .push(Router::new().goal(MethodNotAllowed("GET,HEAD,PATCH,DELETE"))),
        )
        .push(Router::new().goal(MethodNotAllowed("GET,HEAD,POST")))
}

fn not_found(id: Uuid) -> ApiError {
    ApiError::NotFound(format!("Todo {id} does not exist."))
}

/// List todos
///
/// Returns todos oldest first.
#[utoipa::path(
    get,
    path = "/api/v1/todos",
    tag = TAG,
    params(ListTodos),
    responses(
        (status = 200, description = "The matching todos", body = [Todo]),
        (status = 400, description = "Invalid query", body = Problem, content_type = PROBLEM_JSON),
        (status = 500, description = "Unexpected failure", body = Problem, content_type = PROBLEM_JSON),
    )
)]
#[handler]
async fn list_todos(req: &mut Request, depot: &mut Depot) -> Result<Json<Vec<Todo>>, ApiError> {
    let query: ListTodos = query(req)?;
    Ok(Json(state(depot).store.list(query.status).await?))
}

/// Create a todo
#[utoipa::path(
    post,
    path = "/api/v1/todos",
    tag = TAG,
    request_body = CreateTodo,
    responses(
        (status = 201, description = "The created todo", body = Todo),
        (status = 400, description = "Malformed request", body = Problem, content_type = PROBLEM_JSON),
        (status = 422, description = "Validation failed", body = Problem, content_type = PROBLEM_JSON),
        (status = 500, description = "Unexpected failure", body = Problem, content_type = PROBLEM_JSON),
    )
)]
#[handler]
async fn create_todo(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) -> Result<Json<Todo>, ApiError> {
    let input: CreateTodo = json(req).await?;
    let title =
        validate_title(&input.title).map_err(|detail| ApiError::invalid("title", detail))?;
    let todo = new_todo(title);
    state(depot).store.insert(&todo).await?;
    res.status_code(StatusCode::CREATED);
    Ok(Json(todo))
}

/// Get a todo
#[utoipa::path(
    get,
    path = "/api/v1/todos/{id}",
    tag = TAG,
    params(("id" = Uuid, Path, description = "Todo id")),
    responses(
        (status = 200, description = "The todo", body = Todo),
        (status = 400, description = "Invalid id", body = Problem, content_type = PROBLEM_JSON),
        (status = 404, description = "No such todo", body = Problem, content_type = PROBLEM_JSON),
        (status = 500, description = "Unexpected failure", body = Problem, content_type = PROBLEM_JSON),
    )
)]
#[handler]
async fn get_todo(req: &mut Request, depot: &mut Depot) -> Result<Json<Todo>, ApiError> {
    let id = uuid_param(req, "id")?;
    let todo = state(depot)
        .store
        .get(id)
        .await?
        .ok_or_else(|| not_found(id))?;
    Ok(Json(todo))
}

/// Update a todo
///
/// Changes the title, the completed flag, or both.
#[utoipa::path(
    patch,
    path = "/api/v1/todos/{id}",
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
#[handler]
async fn update_todo(req: &mut Request, depot: &mut Depot) -> Result<Json<Todo>, ApiError> {
    let id = uuid_param(req, "id")?;
    let mut changes: UpdateTodo = json(req).await?;
    if let Some(title) = &changes.title {
        changes.title =
            Some(validate_title(title).map_err(|detail| ApiError::invalid("title", detail))?);
    }
    let todo = state(depot)
        .store
        .update(id, &changes)
        .await?
        .ok_or_else(|| not_found(id))?;
    Ok(Json(todo))
}

/// Delete a todo
#[utoipa::path(
    delete,
    path = "/api/v1/todos/{id}",
    tag = TAG,
    params(("id" = Uuid, Path, description = "Todo id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 400, description = "Invalid id", body = Problem, content_type = PROBLEM_JSON),
        (status = 404, description = "No such todo", body = Problem, content_type = PROBLEM_JSON),
        (status = 500, description = "Unexpected failure", body = Problem, content_type = PROBLEM_JSON),
    )
)]
#[handler]
async fn delete_todo(req: &mut Request, depot: &mut Depot) -> Result<StatusCode, ApiError> {
    let id = uuid_param(req, "id")?;
    if !state(depot).store.delete(id).await? {
        return Err(not_found(id));
    }
    Ok(StatusCode::NO_CONTENT)
}
