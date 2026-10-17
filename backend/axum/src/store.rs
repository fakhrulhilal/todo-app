//! Persistence. The backend is chosen by the database URL scheme.

mod postgres;
mod turso;

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    config::DatabaseConfig,
    domain::{StatusFilter, Todo, UpdateTodo},
};

pub use self::{postgres::PostgresStore, turso::TursoStore};

pub type Store = Arc<dyn TodoStore>;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(
        "unsupported database URL `{0}`: expected sqlite://<path>, sqlite::memory: or postgres://..."
    )]
    UnsupportedUrl(String),
    #[error("database error: {0}")]
    Database(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("stored todo is corrupt: {0}")]
    Corrupt(String),
}

#[async_trait]
pub trait TodoStore: Send + Sync {
    /// Checks that the database answers a trivial query.
    async fn ping(&self) -> Result<(), StoreError>;
    /// Oldest first.
    async fn list(&self, filter: StatusFilter) -> Result<Vec<Todo>, StoreError>;
    async fn get(&self, id: Uuid) -> Result<Option<Todo>, StoreError>;
    async fn insert(&self, todo: &Todo) -> Result<(), StoreError>;
    /// Returns the updated todo, or `None` when the id does not exist.
    async fn update(&self, id: Uuid, changes: &UpdateTodo) -> Result<Option<Todo>, StoreError>;
    /// Returns whether a todo was deleted.
    async fn delete(&self, id: Uuid) -> Result<bool, StoreError>;
}

/// Connects to the configured database and applies pending migrations.
pub async fn connect(config: &DatabaseConfig) -> Result<Store, StoreError> {
    let url = config.url.trim();

    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        return Ok(Arc::new(
            PostgresStore::connect(url, config.max_connections).await?,
        ));
    }

    // sqlite://data/todo.db, sqlite:///var/lib/todo.db, sqlite:todo.db, sqlite::memory:
    if let Some(path) = url.strip_prefix("sqlite:") {
        let path = path.strip_prefix("//").unwrap_or(path);
        if !path.is_empty() {
            return Ok(Arc::new(TursoStore::open(path).await?));
        }
    }

    Err(StoreError::UnsupportedUrl(redact(url)))
}

/// Keeps credentials out of error messages and logs.
fn redact(url: &str) -> String {
    match url.split_once("://") {
        Some((scheme, _)) => format!("{scheme}://..."),
        None => url.to_owned(),
    }
}
