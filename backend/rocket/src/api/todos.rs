use rocket::{
    Route, State,
    http::{Method, Status},
    response::status::Created,
    serde::{json::Json, uuid::Error as UuidError},
};
use uuid::Uuid;

use super::{
    AppState,
    extract::{self, path, query},
    problem::{ApiError, PROBLEM_JSON, Problem, method_not_allowed},
};
use crate::domain::{CreateTodo, ListTodos, Todo, UpdateTodo, new_todo, validate_title};

pub const TAG: &str = "todos";

/// Routes under `/api/v1`. GET answers HEAD too. Each path ends in the 405
/// routes for the methods it does not have.
pub fn routes() -> Vec<Route> {
    let mut routes = rocket::routes![list_todos, create_todo, get_todo, update_todo, delete_todo];
    routes.extend(method_not_allowed("/todos", &[Method::Get, Method::Post]));
    routes.extend(method_not_allowed(
        "/todos/<_>",
        &[Method::Get, Method::Patch, Method::Delete],
    ));
    routes
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
#[rocket::get("/todos?<status>")]
async fn list_todos(
    state: &State<AppState>,
    status: Option<String>,
) -> Result<Json<Vec<Todo>>, ApiError> {
    let query = ListTodos {
        status: query(status.as_deref())?,
    };
    Ok(Json(state.store.list(query.status).await?))
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
#[rocket::post("/todos", data = "<input>")]
async fn create_todo(
    state: &State<AppState>,
    input: extract::Json<CreateTodo>,
) -> Result<Created<Json<Todo>>, ApiError> {
    let title =
        validate_title(&input.0.title).map_err(|detail| ApiError::invalid("title", detail))?;
    let todo = new_todo(title);
    state.store.insert(&todo).await?;
    // With `Location`, the address of the new todo.
    Ok(Created::new(format!("/api/v1/todos/{}", todo.id)).body(Json(todo)))
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
#[rocket::get("/todos/<id>")]
async fn get_todo(
    state: &State<AppState>,
    id: Result<Uuid, UuidError>,
) -> Result<Json<Todo>, ApiError> {
    let id = path(id)?;
    let todo = state.store.get(id).await?.ok_or_else(|| not_found(id))?;
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
#[rocket::patch("/todos/<id>", data = "<changes>")]
async fn update_todo(
    state: &State<AppState>,
    id: Result<Uuid, UuidError>,
    changes: extract::Json<UpdateTodo>,
) -> Result<Json<Todo>, ApiError> {
    let id = path(id)?;
    let mut changes = changes.0;
    if let Some(title) = &changes.title {
        changes.title =
            Some(validate_title(title).map_err(|detail| ApiError::invalid("title", detail))?);
    }
    let todo = state
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
#[rocket::delete("/todos/<id>")]
async fn delete_todo(
    state: &State<AppState>,
    id: Result<Uuid, UuidError>,
) -> Result<Status, ApiError> {
    let id = path(id)?;
    if !state.store.delete(id).await? {
        return Err(not_found(id));
    }
    Ok(Status::NoContent)
}
