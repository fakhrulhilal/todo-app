//! The JSON API and its OpenAPI document. No documentation UI is served: any
//! OpenAPI viewer (Scalar in compose.yaml) reads the document instead.

mod extract;
pub mod health;
pub mod problem;
pub mod todos;

use health_checks::HealthCheckService;
use rocket::{
    Build, Rocket, Route, State,
    http::{ContentType, Method},
    response::Redirect,
};
use utoipa::OpenApi;

use self::problem::method_not_allowed;
use crate::store::Store;

pub const OPENAPI_PATH: &str = "/api/openapi.json";

/// Application state, managed by Rocket (`State<AppState>` in a handler).
pub struct AppState {
    pub store: Store,
    pub health: HealthCheckService,
    /// The OpenAPI document, serialized once.
    openapi: String,
}

impl AppState {
    /// `health` comes from `health::checks`.
    pub fn new(store: Store, health: HealthCheckService) -> Self {
        Self {
            health,
            store,
            openapi: ApiDoc::openapi()
                .to_json()
                .expect("the OpenAPI document serializes"),
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(title = "Todo API", description = "A todo list. Errors are RFC 9457 problem details."),
    paths(
        todos::list_todos,
        todos::create_todo,
        todos::get_todo,
        todos::update_todo,
        todos::delete_todo,
        health::live,
        health::ready,
    ),
    tags(
        (name = todos::TAG, description = "Manage todos"),
        (name = health::TAG, description = "Liveness and readiness probes"),
    )
)]
struct ApiDoc;

/// Mounts the routes under `/api/v1` and `/health`, the OpenAPI document and
/// the root redirect, and registers the catchers that make every error a
/// problem.
pub fn mount(rocket: Rocket<Build>) -> Rocket<Build> {
    rocket
        .mount("/api/v1", todos::routes())
        .mount("/", health::routes())
        .mount("/", routes())
        .register("/", problem::catchers())
}

fn routes() -> Vec<Route> {
    let mut routes = rocket::routes![openapi, root];
    routes.extend(method_not_allowed(OPENAPI_PATH, &[Method::Get]));
    routes.extend(method_not_allowed("/", &[Method::Get]));
    routes
}

#[rocket::get("/api/openapi.json")]
fn openapi(state: &State<AppState>) -> (ContentType, String) {
    (ContentType::JSON, state.openapi.clone())
}

#[rocket::get("/")]
fn root() -> Redirect {
    Redirect::temporary(OPENAPI_PATH)
}
