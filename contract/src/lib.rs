//! The todo model shared by the backend and the frontend, so both sides
//! of the wire are checked against the same types.

use std::{convert::Infallible, fmt, str::FromStr};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

pub const MAX_TITLE_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Todo {
    pub id: Uuid,
    #[cfg_attr(feature = "openapi", schema(example = "Buy milk"))]
    pub title: String,
    pub completed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateTodo {
    /// 1 to 200 characters after trimming.
    #[cfg_attr(feature = "openapi", schema(example = "Buy milk"))]
    pub title: String,
}

/// Fields left out keep their current value.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateTodo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(example = "Buy oat milk"))]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum StatusFilter {
    #[default]
    All,
    Active,
    Completed,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams))]
#[cfg_attr(feature = "openapi", into_params(parameter_in = Query))]
pub struct ListTodos {
    /// Which todos to return.
    #[serde(default)]
    pub status: StatusFilter,
}

impl StatusFilter {
    pub const VALUES: [Self; 3] = [Self::All, Self::Active, Self::Completed];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Active => "active",
            Self::Completed => "completed",
        }
    }

    /// The `completed` value this filter selects, if it restricts at all.
    pub fn completed(self) -> Option<bool> {
        match self {
            Self::All => None,
            Self::Active => Some(false),
            Self::Completed => Some(true),
        }
    }

    pub fn matches(self, todo: &Todo) -> bool {
        self.completed()
            .is_none_or(|completed| completed == todo.completed)
    }
}

impl fmt::Display for StatusFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Unknown values fall back to `All`.
impl FromStr for StatusFilter {
    type Err = Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::VALUES
            .into_iter()
            .find(|filter| filter.as_str() == value)
            .unwrap_or_default())
    }
}

/// Trims a title and checks its length. The error is a client-safe message.
pub fn validate_title(title: &str) -> Result<String, String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("must not be empty".to_owned());
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("must be at most {MAX_TITLE_CHARS} characters"));
    }
    Ok(title.to_owned())
}
