use ntex::web::{
    self, HttpResponse,
    types::{Json, Path, Query},
};
use uuid::Uuid;

use super::{
    AppState,
    problem::{ApiError, PROBLEM_JSON, Problem, method_not_allowed},
};
use crate::domain::{CreateTodo, ListTodos, Todo, UpdateTodo, new_todo, validate_title};

pub const TAG: &str = "todos";

/// Routes under `/api/v1`. A resource groups the methods of one path, so a
/// wrong method gets its 405 rather than the scope's 404.
pub fn routes(config: &mut web::ServiceConfig<AppState>) {
    config.service(
        web::resource("/todos")
            .route(super::get().to_with_state(list_todos))
            .route(web::post().to_with_state(create_todo))
            .default_service(web::to(method_not_allowed("GET,HEAD,POST"))),
    );
    config.service(
        web::resource("/todos/{id}")
            .route(super::get().to_with_state(get_todo))
            .route(web::patch().to_with_state(update_todo))
            .route(web::delete().to_with_state(delete_todo))
            .default_service(web::to(method_not_allowed("GET,HEAD,PATCH,DELETE"))),
    );
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
async fn list_todos(
    state: &AppState,
    _: (),
    query: Query<ListTodos>,
) -> Result<Json<Vec<Todo>>, ApiError> {
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
async fn create_todo(
    state: &AppState,
    _: (),
    input: Json<CreateTodo>,
) -> Result<HttpResponse, ApiError> {
    let title =
        validate_title(&input.title).map_err(|detail| ApiError::invalid("title", detail))?;
    let todo = new_todo(title);
    state.store.insert(&todo).await?;
    Ok(HttpResponse::Created().json(&todo))
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
async fn get_todo(state: &AppState, _: (), id: Path<Uuid>) -> Result<Json<Todo>, ApiError> {
    let id = id.into_inner();
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
async fn update_todo(
    state: &AppState,
    _: (),
    id: Path<Uuid>,
    changes: Json<UpdateTodo>,
) -> Result<Json<Todo>, ApiError> {
    let id = id.into_inner();
    let mut changes = changes.into_inner();
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
async fn delete_todo(state: &AppState, _: (), id: Path<Uuid>) -> Result<HttpResponse, ApiError> {
    let id = id.into_inner();
    if !state.store.delete(id).await? {
        return Err(not_found(id));
    }
    Ok(HttpResponse::NoContent().build())
}
