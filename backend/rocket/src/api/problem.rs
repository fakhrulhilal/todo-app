//! API errors as RFC 9457 problem details (`application/problem+json`).
//!
//! Every error a request can end in becomes a problem, each the Rocket way:
//!
//! - `ApiError` is a `Responder`, so a handler that returns one renders its
//!   problem. A responder sees the request, so `instance` is filled in there.
//! - A guard that fails (`api/extract.rs`) leaves its problem in the
//!   request-local cache and fails with its status; the catcher for that
//!   status (`catchers()`) renders what it left.
//! - Everything else Rocket ends with an error status (no route matched, a
//!   panic, a request it could not parse) reaches the default catcher, which
//!   writes a problem with the status alone.
//! - Rocket has no 405: a route only matches its own method, and a path with
//!   another method is a 404. `method_not_allowed` adds, for each path, a
//!   route for every other method that answers 405 with `Allow`.

use std::{collections::BTreeMap, io::Cursor};

use rocket::{
    Catcher, Data, Request, Response, Route,
    http::{ContentType, Header, Method, Status},
    response::{self, Responder},
    route::{self, Handler},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::store::StoreError;

pub const PROBLEM_JSON: &str = "application/problem+json";

/// An RFC 9457 problem details object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Problem {
    /// URI reference identifying the problem type.
    #[serde(rename = "type", default = "about_blank")]
    #[schema(example = "about:blank")]
    pub type_uri: String,
    /// Short summary of the problem type.
    #[schema(example = "Not Found")]
    pub title: String,
    /// The HTTP status code.
    #[schema(example = 404)]
    pub status: u16,
    /// Explanation specific to this occurrence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// URI reference identifying this occurrence: the path that was requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(example = "/api/v1/todos/01981f3e-7d55-7c42-9b0a-3b1f6c2f4a10")]
    pub instance: Option<String>,
    /// The members of the request that failed validation: what is wrong
    /// with each, by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schema(example = json!({ "title": ["must not be empty"] }))]
    pub errors: BTreeMap<String, Vec<String>>,
}

fn about_blank() -> String {
    "about:blank".to_owned()
}

impl Problem {
    pub fn new(status: Status) -> Self {
        Self {
            type_uri: about_blank(),
            title: status.reason().unwrap_or("Unknown Error").to_owned(),
            status: status.code,
            detail: None,
            instance: None,
            errors: BTreeMap::new(),
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// The problem, with `instance` set to the path that was requested (without
/// the query string).
impl<'r> Responder<'r, 'static> for Problem {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        let status = Status::from_code(self.status).unwrap_or(Status::InternalServerError);
        let problem = Self {
            instance: Some(request.uri().path().to_string()),
            ..self
        };
        // A problem always serializes: it is strings, a number and a map of strings.
        let body = serde_json::to_vec(&problem).unwrap_or_default();
        Response::build()
            .status(status)
            .header(ContentType::new("application", "problem+json"))
            .sized_body(body.len(), Cursor::new(body))
            .ok()
    }
}

/// Everything an API handler can fail with.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),
    #[error("request validation failed")]
    Validation(BTreeMap<String, Vec<String>>),
    /// The request could not be read: malformed JSON, a bad path or query value.
    #[error("{1}")]
    Rejected(Status, String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl ApiError {
    pub fn invalid(member: &str, detail: impl Into<String>) -> Self {
        Self::Validation(BTreeMap::from([(member.to_owned(), vec![detail.into()])]))
    }

    pub fn status(&self) -> Status {
        match self {
            Self::NotFound(_) => Status::NotFound,
            Self::Validation(_) => Status::UnprocessableEntity,
            Self::Rejected(status, _) => *status,
            Self::Store(_) => Status::InternalServerError,
        }
    }

    pub fn into_problem(self) -> Problem {
        match self {
            Self::NotFound(detail) => Problem::new(Status::NotFound).detail(detail),
            Self::Validation(errors) => Problem {
                errors,
                ..Problem::new(Status::UnprocessableEntity)
                    .detail("One or more members of the request are invalid.")
            },
            Self::Rejected(status, detail) => Problem::new(status).detail(detail),
            Self::Store(error) => {
                // The cause stays in the log; clients only learn that it failed.
                tracing::error!(error = %error, "request failed");
                Problem::new(Status::InternalServerError)
                    .detail("The request could not be completed.")
            }
        }
    }
}

impl<'r> Responder<'r, 'static> for ApiError {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        self.into_problem().respond_to(request)
    }
}

/// What a failed guard leaves for the catcher, in the request-local cache.
/// Rocket hands a catcher the status and the request, not the guard's error.
pub(crate) struct Rejection(Option<Problem>);

impl Rejection {
    /// Keeps `error` for the catcher and returns the status to fail with.
    pub(crate) fn keep(request: &Request<'_>, error: ApiError) -> Status {
        let status = error.status();
        request.local_cache(|| Self(Some(error.into_problem())));
        status
    }
}

/// The catchers, registered at the root so they cover every path.
pub fn catchers() -> Vec<Catcher> {
    rocket::catchers![unknown_endpoint, rejected]
}

/// No route matched the path.
#[rocket::catch(404)]
fn unknown_endpoint() -> ApiError {
    ApiError::NotFound("No such endpoint.".to_owned())
}

/// Every other error status: what a failed guard left, or the status alone
/// (a panic is a 500, a request Rocket could not parse a 400).
#[rocket::catch(default)]
fn rejected(status: Status, request: &Request<'_>) -> Problem {
    match &request.local_cache(|| Rejection(None)).0 {
        Some(problem) if problem.status == status.code => problem.clone(),
        _ => Problem::new(status),
    }
}

/// The 405 routes of `path`, whose routes answer `allowed`: one route per
/// other method. HEAD is never one of them when GET is allowed, since Rocket
/// answers HEAD with the GET route.
pub fn method_not_allowed(path: &str, allowed: &[Method]) -> Vec<Route> {
    let mut allow: Vec<Method> = allowed.to_vec();
    if allowed.contains(&Method::Get) && !allowed.contains(&Method::Head) {
        allow.insert(1, Method::Head);
    }
    let handler = MethodNotAllowed(
        allow
            .iter()
            .map(|method| method.as_str())
            .collect::<Vec<_>>()
            .join(","),
    );
    [
        Method::Get,
        Method::Head,
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
        Method::Options,
        Method::Trace,
        Method::Connect,
    ]
    .into_iter()
    .filter(|method| !allow.contains(method))
    .map(|method| Route::ranked(FALLBACK_RANK, method, path, handler.clone()))
    .collect()
}

/// After every other route of the path, though they never collide: they
/// differ in method.
const FALLBACK_RANK: isize = 100;

/// Answers 405 with the methods the path has, as in `Allow`.
#[derive(Clone)]
struct MethodNotAllowed(String);

#[rocket::async_trait]
impl Handler for MethodNotAllowed {
    async fn handle<'r>(&self, request: &'r Request<'_>, _: Data<'r>) -> route::Outcome<'r> {
        let mut response = match Problem::new(Status::MethodNotAllowed).respond_to(request) {
            Ok(response) => response,
            Err(status) => return route::Outcome::Error(status),
        };
        response.set_header(Header::new("Allow", self.0.clone()));
        route::Outcome::Success(response)
    }
}
