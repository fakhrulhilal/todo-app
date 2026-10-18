//! API errors as RFC 9457 problem details (`application/problem+json`).
//!
//! Two paths lead here. `ApiError` is a `Writer`, so a handler that returns
//! one renders its problem itself, `instance` included. Every other error
//! response that leaves the body empty (an unknown path, a panic) reaches the
//! service's catcher, `catch`, which gives it a problem body.

use std::collections::BTreeMap;

use async_trait::async_trait;
use salvo::{
    http::{
        HeaderValue, ResBody, StatusCode,
        header::{ALLOW, CONTENT_TYPE},
    },
    prelude::*,
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

    /// Writes the problem as the response, for the path that was requested.
    /// Headers already on the response, such as CORS's, are kept.
    pub fn render(self, req: &Request, res: &mut Response) {
        let problem = Self {
            instance: Some(req.uri().path().to_owned()),
            ..self
        };
        let status =
            StatusCode::from_u16(problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        res.status_code(status);
        res.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
        // A problem always serializes: it is strings, a number and a map of strings.
        let body = serde_json::to_vec(&problem).unwrap_or_default();
        res.replace_body(ResBody::Once(body.into()));
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

    fn into_problem(self) -> Problem {
        match self {
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
        }
    }
}

#[async_trait]
impl Writer for ApiError {
    async fn write(self, req: &mut Request, _: &mut Depot, res: &mut Response) {
        self.into_problem().render(req, res);
    }
}

/// The service's catcher: every 4xx or 5xx response that has no body yet
/// becomes a problem. Salvo leaves HEAD responses out, as they have no body.
#[handler]
pub async fn catch(req: &mut Request, res: &mut Response) {
    let status = res.status_code.unwrap_or(StatusCode::NOT_FOUND);
    let is_error = status.is_client_error() || status.is_server_error();
    if !is_error || !(res.body.is_none() || res.body.is_error()) {
        return;
    }

    let problem = match status {
        // No route matched the path.
        StatusCode::NOT_FOUND => Problem::new(status).detail("No such endpoint."),
        // A panic, caught by `CatchPanic`.
        StatusCode::INTERNAL_SERVER_ERROR => {
            Problem::new(status).detail("The request could not be completed.")
        }
        _ => Problem::new(status),
    };
    problem.render(req, res);
}

/// The goal of a path's last child router: the path matched, none of its
/// methods did. Salvo's own 405 has no `Allow` header, so this answers
/// instead. The field lists the methods the path has, as in `Allow`.
pub struct MethodNotAllowed(pub &'static str);

#[async_trait]
impl Handler for MethodNotAllowed {
    async fn handle(&self, req: &mut Request, _: &mut Depot, res: &mut Response, _: &mut FlowCtrl) {
        res.headers_mut()
            .insert(ALLOW, HeaderValue::from_static(self.0));
        Problem::new(StatusCode::METHOD_NOT_ALLOWED).render(req, res);
    }
}
