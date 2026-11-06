//! API errors as RFC 9457 problem details (`application/problem+json`).
//!
//! Every error a request can end in becomes a problem:
//!
//! - `ApiError` is a `ResponseError`, so a handler that returns one renders
//!   its problem.
//! - The extractors' errors (`Json`, `Path`, `Query`) go through the error
//!   handlers that `lib.rs` registers in their configs, which turn them into
//!   `ApiError::Rejected` with the statuses and wording of axum's rejections.
//! - The default services answer an unknown path (404) and a known path with
//!   another method (405, with `Allow`).
//!
//! `ResponseError::error_response` does not see the request, so the
//! `problem_details` middleware fills in `instance` afterwards, and turns any
//! other error response (from actix itself or from middleware) into a problem.

use std::collections::BTreeMap;

use actix_web::{
    Error, HttpRequest, HttpResponse, ResponseError,
    body::{BoxBody, MessageBody},
    dev::{ServiceRequest, ServiceResponse},
    error::{JsonPayloadError, PathError, QueryPayloadError},
    http::{
        StatusCode,
        header::{ALLOW, CONTENT_LENGTH, CONTENT_TYPE, HeaderValue},
    },
    middleware::Next,
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

    fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    pub fn into_response(self) -> HttpResponse {
        // A problem always serializes: it is strings, a number and a map of strings.
        let body = serde_json::to_vec(&self).unwrap_or_default();
        let mut response = HttpResponse::build(self.status_code())
            .content_type(PROBLEM_JSON)
            .body(body);
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

    fn to_problem(&self) -> Problem {
        match self {
            Self::NotFound(detail) => Problem::new(StatusCode::NOT_FOUND).detail(detail),
            Self::Validation(errors) => Problem {
                errors: errors.clone(),
                ..Problem::new(StatusCode::UNPROCESSABLE_ENTITY)
                    .detail("One or more members of the request are invalid.")
            },
            Self::Rejected(status, detail) => Problem::new(*status).detail(detail),
            Self::Store(error) => {
                // The cause stays in the log; clients only learn that it failed.
                tracing::error!(error = %error, "request failed");
                Problem::new(StatusCode::INTERNAL_SERVER_ERROR)
                    .detail("The request could not be completed.")
            }
        }
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Rejected(status, _) => *status,
            Self::Store(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        self.to_problem().into_response()
    }
}

fn rejected(status: StatusCode, detail: impl Into<String>) -> Error {
    ApiError::Rejected(status, detail.into()).into()
}

/// `web::Json<T>` extractor failures (`JsonConfig::error_handler`), with the
/// statuses and wording of axum's `JsonRejection`: a body that is not JSON is
/// 400, JSON of the wrong shape 422, another content type 415.
pub fn json_error(error: JsonPayloadError, _: &HttpRequest) -> Error {
    match error {
        JsonPayloadError::OverflowKnownLength { .. } | JsonPayloadError::Overflow { .. } => {
            rejected(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Failed to buffer the request body: length limit exceeded",
            )
        }
        JsonPayloadError::ContentType => rejected(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected request with `Content-Type: application/json`",
        ),
        JsonPayloadError::Deserialize(error) if error.is_data() => rejected(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Failed to deserialize the JSON body into the target type: {error}"),
        ),
        JsonPayloadError::Deserialize(error) => rejected(
            StatusCode::BAD_REQUEST,
            format!("Failed to parse the request body as JSON: {error}"),
        ),
        error => rejected(
            StatusCode::BAD_REQUEST,
            format!("Failed to buffer the request body: {error}"),
        ),
    }
}

/// `web::Path<T>` extractor failures (`PathConfig::error_handler`), such as an
/// id that is not a UUID.
pub fn path_error(error: PathError, _: &HttpRequest) -> Error {
    let detail = match error {
        PathError::Deserialize(error) => error.to_string(),
        error => error.to_string(),
    };
    rejected(StatusCode::BAD_REQUEST, format!("Invalid URL: {detail}"))
}

/// `web::Query<T>` extractor failures (`QueryConfig::error_handler`), such as
/// an unknown `status`.
pub fn query_error(error: QueryPayloadError, _: &HttpRequest) -> Error {
    let detail = match error {
        QueryPayloadError::Deserialize(error) => error.to_string(),
        error => error.to_string(),
    };
    rejected(
        StatusCode::BAD_REQUEST,
        format!("Failed to deserialize query string: {detail}"),
    )
}

/// Default service of the app: no route matched the path. A scope without a
/// default service of its own falls back to this one too.
pub async fn unknown_endpoint() -> HttpResponse {
    ApiError::NotFound("No such endpoint.".to_owned()).error_response()
}

/// Default service of a resource: the path matched, the method did not.
/// `allow` lists the methods the resource has, as in the `Allow` header.
pub fn method_not_allowed(allow: &'static str) -> actix_web::Route {
    actix_web::web::to(move || async move {
        let mut response = Problem::new(StatusCode::METHOD_NOT_ALLOWED).into_response();
        response
            .headers_mut()
            .insert(ALLOW, HeaderValue::from_static(allow));
        response
    })
}

/// Finishes every error response as a problem: sets `instance` to the
/// requested path, and gives the error responses that are not problems yet
/// (from actix itself or from a middleware such as CORS) a problem body.
pub async fn problem_details(
    request: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<BoxBody>, Error> {
    // Not `request.request().clone()` here to keep the request for later: a
    // clone made before routing makes actix's router panic (it needs the only
    // reference to fill in the path parameters). Every error arrives as a
    // response anyway: a handler's, an extractor's, and CORS's refusals
    // (`req.error_response(..)`), with the error attached.
    let mut response = next.call(request).await?.map_into_boxed_body();

    let status = response.status();
    let stored = response.response_mut().extensions_mut().remove::<Problem>();
    let problem = match stored {
        Some(problem) => problem,
        None => {
            let is_error = status.is_client_error() || status.is_server_error();
            let error = response.response().error();
            if !is_error || (error.is_none() && response.headers().contains_key(CONTENT_TYPE)) {
                return Ok(response);
            }
            match error {
                // Written for the client, such as CORS's "Origin is not allowed".
                Some(error) if status.is_client_error() => {
                    Problem::new(status).detail(error.to_string())
                }
                _ => Problem::new(status),
            }
        }
    };

    let (request, original) = response.into_parts();
    let mut finished = Problem {
        instance: Some(request.path().to_owned()),
        ..problem
    }
    .into_response();
    // Keep what the original response said besides its body, such as `Allow`
    // and the CORS headers.
    for (name, value) in original.headers() {
        if name != CONTENT_TYPE && name != CONTENT_LENGTH {
            finished.headers_mut().append(name.clone(), value.clone());
        }
    }
    Ok(ServiceResponse::new(request, finished))
}
