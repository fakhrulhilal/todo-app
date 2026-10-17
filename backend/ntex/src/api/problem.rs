//! API errors as RFC 9457 problem details (`application/problem+json`).
//!
//! `ApiError` is the error domain of `AppState` (see `api.rs`): every error that
//! can reach a response, from a handler, an extractor, a responder or CORS,
//! needs a `WebResponseError<AppState, ApiError>` impl here, or its route does
//! not compile. The renderers do not see the request, so the `ProblemDetails`
//! middleware fills in `instance` afterwards.

use std::collections::BTreeMap;

use ntex::{
    Ctx, Middleware, Service,
    http::{
        StatusCode,
        header::{ALLOW, CONTENT_LENGTH, CONTENT_TYPE, HeaderValue},
    },
    web::{
        HttpRequest, HttpResponse, WebRequest, WebResponse, WebResponseError,
        error::{JsonError, JsonPayloadError, PathError, QueryPayloadError},
    },
};
use ntex_cors::CorsError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::AppState;
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

    pub fn into_response(self) -> HttpResponse {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let response = HttpResponse::builder(status)
            .content_type(PROBLEM_JSON)
            .json(&self);
        // Kept so `ProblemDetails` can add what only the request knows.
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

impl WebResponseError<AppState, ApiError> for ApiError {
    fn error_response(&self, _: &AppState) -> HttpResponse {
        self.to_problem().into_response()
    }
}

/// Renders an extractor's error as `ApiError::Rejected`.
fn rejected(status: StatusCode, detail: impl Into<String>) -> HttpResponse {
    ApiError::Rejected(status, detail.into())
        .to_problem()
        .into_response()
}

/// `Json<T>` extractor failures, with the statuses and wording of axum's
/// `JsonRejection`: a body that is not JSON is 400, JSON of the wrong shape 422.
impl WebResponseError<AppState, ApiError> for JsonPayloadError {
    fn error_response(&self, _: &AppState) -> HttpResponse {
        match self {
            Self::Overflow => rejected(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Failed to buffer the request body: length limit exceeded",
            ),
            Self::ContentType => rejected(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Expected request with `Content-Type: application/json`",
            ),
            Self::Deserialize(error) if error.is_data() => rejected(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("Failed to deserialize the JSON body into the target type: {error}"),
            ),
            Self::Deserialize(error) => rejected(
                StatusCode::BAD_REQUEST,
                format!("Failed to parse the request body as JSON: {error}"),
            ),
            Self::Payload(error) => rejected(
                StatusCode::BAD_REQUEST,
                format!("Failed to buffer the request body: {error}"),
            ),
        }
    }
}

/// `Path<T>` extractor failures, such as an id that is not a UUID.
impl WebResponseError<AppState, ApiError> for PathError {
    fn error_response(&self, _: &AppState) -> HttpResponse {
        let Self::Deserialize(error) = self;
        rejected(StatusCode::BAD_REQUEST, format!("Invalid URL: {error}"))
    }
}

/// `Query<T>` extractor failures, such as an unknown `status`.
impl WebResponseError<AppState, ApiError> for QueryPayloadError {
    fn error_response(&self, _: &AppState) -> HttpResponse {
        let Self::Deserialize(error) = self;
        rejected(
            StatusCode::BAD_REQUEST,
            format!("Failed to deserialize query string: {error}"),
        )
    }
}

/// `Json<T>` responder failures. Never expected, and the detail stays generic.
impl WebResponseError<AppState, ApiError> for JsonError {
    fn error_response(&self, _: &AppState) -> HttpResponse {
        tracing::error!(error = %self, "serializing a response failed");
        Problem::new(StatusCode::INTERNAL_SERVER_ERROR)
            .detail("The request could not be completed.")
            .into_response()
    }
}

/// A browser request from an origin that is not allowed, or a malformed
/// preflight.
impl WebResponseError<AppState, ApiError> for CorsError {
    fn error_response(&self, _: &AppState) -> HttpResponse {
        rejected(StatusCode::BAD_REQUEST, self.to_string())
    }
}

/// Default service of the app and of each scope: no route matched the path.
pub async fn unknown_endpoint() -> HttpResponse {
    ApiError::NotFound("No such endpoint.".to_owned())
        .to_problem()
        .into_response()
}

/// Default service of a resource: the path matched, the method did not.
/// `allow` lists the methods the resource has, as in the `Allow` header.
pub fn method_not_allowed(
    allow: &'static str,
) -> impl AsyncFn(HttpRequest) -> HttpResponse + Clone + 'static {
    async move |_: HttpRequest| {
        let mut response = Problem::new(StatusCode::METHOD_NOT_ALLOWED).into_response();
        response
            .headers_mut()
            .insert(ALLOW, HeaderValue::from_static(allow));
        response
    }
}

/// Finishes every error response as a problem: sets `instance` to the
/// requested path, and gives bodiless errors a problem body.
pub struct ProblemDetails;

pub struct ProblemDetailsService<S> {
    service: S,
}

impl<S> Middleware<S, AppState> for ProblemDetails {
    type Service = ProblemDetailsService<S>;

    fn create(&self, _: &AppState, service: S) -> Self::Service {
        ProblemDetailsService { service }
    }
}

impl<S, R> Service<AppState, WebRequest<R>> for ProblemDetailsService<S>
where
    S: Service<AppState, WebRequest<R>, Res = WebResponse>,
{
    type Res = WebResponse;
    type Error = S::Error;

    ntex::forward_ready!(AppState, service);
    ntex::forward_shutdown!(AppState, service);

    async fn call(
        &self,
        request: WebRequest<R>,
        ctx: Ctx<'_, Self, AppState>,
    ) -> Result<WebResponse, S::Error> {
        let response = ctx.call(&self.service, request).await?;

        let status = response.status();
        let stored = response.response().extensions_mut().remove::<Problem>();
        let problem = match stored {
            Some(problem) => problem,
            None => {
                let is_error = status.is_client_error() || status.is_server_error();
                if !is_error || response.headers().contains_key(CONTENT_TYPE) {
                    return Ok(response);
                }
                Problem::new(status)
            }
        };

        let (original, request) = response.into_parts();
        let mut finished = Problem {
            instance: Some(request.path().to_owned()),
            ..problem
        }
        .into_response();
        // Keep what the original response said besides its body, such as `Allow`.
        for (name, value) in original.headers() {
            if name != CONTENT_TYPE && name != CONTENT_LENGTH {
                finished.headers_mut().insert(name.clone(), value.clone());
            }
        }
        Ok(WebResponse::new(finished, request))
    }
}
