//! API errors as RFC 9457 problem details (`application/problem+json`).

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{
        Request,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{
        HeaderValue, StatusCode,
        header::{CONTENT_LENGTH, CONTENT_TYPE},
    },
    middleware::Next,
    response::{IntoResponse, Response},
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
    pub fn new(status: StatusCode) -> Self {
        Self {
            type_uri: about_blank(),
            title: status
                .canonical_reason()
                .unwrap_or("Unknown Error")
                .to_owned(),
            status: status.as_u16(),
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

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, Json(&self)).into_response();
        response
            .headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
        // Kept so `problem_details` can add what only the request knows.
        response.extensions_mut().insert(self);
        response
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
    Rejected(StatusCode, String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl ApiError {
    pub fn invalid(member: &str, detail: impl Into<String>) -> Self {
        Self::Validation(BTreeMap::from([(member.to_owned(), vec![detail.into()])]))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let problem = match self {
            Self::NotFound(detail) => Problem::new(StatusCode::NOT_FOUND).detail(detail),
            Self::Validation(errors) => Problem {
                errors,
                ..Problem::new(StatusCode::UNPROCESSABLE_ENTITY)
                    .detail("One or more members of the request are invalid.")
            },
            Self::Rejected(status, detail) => Problem::new(status).detail(detail),
            Self::Store(error) => {
                // The cause stays in the log; clients only learn that it failed.
                tracing::error!(error = %error, "request failed");
                Problem::new(StatusCode::INTERNAL_SERVER_ERROR)
                    .detail("The request could not be completed.")
            }
        };
        problem.into_response()
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self::Rejected(rejection.status(), rejection.body_text())
    }
}

impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        Self::Rejected(rejection.status(), rejection.body_text())
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        Self::Rejected(rejection.status(), rejection.body_text())
    }
}

/// Finishes every error response as a problem: sets `instance` to the
/// requested path, and gives the router's own bodiless errors (404, 405, ...)
/// a problem body.
pub async fn problem_details(request: Request, next: Next) -> Response {
    let instance = request.uri().path().to_owned();
    let response = next.run(request).await;

    let status = response.status();
    let (mut parts, body) = response.into_parts();
    let problem = match parts.extensions.remove::<Problem>() {
        Some(problem) => problem,
        None => {
            let is_error = status.is_client_error() || status.is_server_error();
            if !is_error || parts.headers.contains_key(CONTENT_TYPE) {
                return Response::from_parts(parts, body);
            }
            Problem::new(status)
        }
    };

    let mut response = Problem {
        instance: Some(instance),
        ..problem
    }
    .into_response();
    // Keep what the original response said besides its body, such as `Allow`.
    for (name, value) in &parts.headers {
        if name != CONTENT_TYPE && name != CONTENT_LENGTH {
            response.headers_mut().insert(name, value.clone());
        }
    }
    response
}
