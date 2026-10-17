//! SQLite-compatible storage on the embedded Turso engine.

use std::{path::Path, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use turso::{Builder, Connection, Database, Row, Value};
use uuid::Uuid;

use super::{StoreError, TodoStore};
use crate::domain::{self, StatusFilter, Todo, UpdateTodo};

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Applied in order; each runs once and is recorded in `_migrations`.
const MIGRATIONS: &[(i64, &str, &str)] = &[(
    1,
    "create_todos",
    include_str!("../../migrations/sqlite/0001_create_todos.sql"),
)];

pub struct TursoStore {
    db: Database,
}

impl TursoStore {
    /// Opens (creating if needed) a database file, or `:memory:`.
    pub async fn open(path: &str) -> Result<Self, StoreError> {
        if path != ":memory:"
            && let Some(dir) = Path::new(path).parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).map_err(|error| StoreError::Database(error.into()))?;
        }

        let db = Builder::new_local(path).build().await?;
        let store = Self { db };
        store.migrate().await?;
        Ok(store)
    }

    fn connection(&self) -> Result<Connection, StoreError> {
        let connection = self.db.connect()?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        Ok(connection)
    }

    async fn migrate(&self) -> Result<(), StoreError> {
        let connection = self.connection()?;
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS _migrations (\
                    version INTEGER PRIMARY KEY, \
                    name TEXT NOT NULL, \
                    applied_at TEXT NOT NULL)",
                (),
            )
            .await?;

        for (version, name, sql) in MIGRATIONS {
            let mut rows = connection
                .query("SELECT 1 FROM _migrations WHERE version = ?1", [*version])
                .await?;
            if rows.next().await?.is_some() {
                continue;
            }
            drop(rows);

            connection.execute_batch(sql).await?;
            connection
                .execute(
                    "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
                    vec![
                        Value::Integer(*version),
                        Value::Text((*name).to_owned()),
                        Value::Text(timestamp(domain::now())),
                    ],
                )
                .await?;
            tracing::info!(version, name, "applied migration");
        }
        Ok(())
    }
}

#[async_trait]
impl TodoStore for TursoStore {
    async fn ping(&self) -> Result<(), StoreError> {
        let mut rows = self.connection()?.query("SELECT 1", ()).await?;
        rows.next().await?;
        Ok(())
    }

    async fn list(&self, filter: StatusFilter) -> Result<Vec<Todo>, StoreError> {
        let connection = self.connection()?;
        let mut rows = match filter.completed() {
            None => {
                connection
                    .query(
                        "SELECT id, title, completed, created_at, updated_at FROM todos \
                         ORDER BY created_at, id",
                        (),
                    )
                    .await?
            }
            Some(completed) => {
                connection
                    .query(
                        "SELECT id, title, completed, created_at, updated_at FROM todos \
                         WHERE completed = ?1 ORDER BY created_at, id",
                        [i64::from(completed)],
                    )
                    .await?
            }
        };

        let mut todos = Vec::new();
        while let Some(row) = rows.next().await? {
            todos.push(todo_from_row(&row)?);
        }
        Ok(todos)
    }

    async fn get(&self, id: Uuid) -> Result<Option<Todo>, StoreError> {
        let mut rows = self
            .connection()?
            .query(
                "SELECT id, title, completed, created_at, updated_at FROM todos WHERE id = ?1",
                [id.to_string()],
            )
            .await?;
        rows.next().await?.as_ref().map(todo_from_row).transpose()
    }

    async fn insert(&self, todo: &Todo) -> Result<(), StoreError> {
        self.connection()?
            .execute(
                "INSERT INTO todos (id, title, completed, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                vec![
                    Value::Text(todo.id.to_string()),
                    Value::Text(todo.title.clone()),
                    Value::Integer(i64::from(todo.completed)),
                    Value::Text(timestamp(todo.created_at)),
                    Value::Text(timestamp(todo.updated_at)),
                ],
            )
            .await?;
        Ok(())
    }

    async fn update(&self, id: Uuid, changes: &UpdateTodo) -> Result<Option<Todo>, StoreError> {
        let changed = self
            .connection()?
            .execute(
                "UPDATE todos SET \
                    title = COALESCE(?2, title), \
                    completed = COALESCE(?3, completed), \
                    updated_at = ?4 \
                 WHERE id = ?1",
                vec![
                    Value::Text(id.to_string()),
                    changes.title.clone().map_or(Value::Null, Value::Text),
                    changes.completed.map_or(Value::Null, |completed| {
                        Value::Integer(i64::from(completed))
                    }),
                    Value::Text(timestamp(domain::now())),
                ],
            )
            .await?;
        if changed == 0 {
            return Ok(None);
        }
        self.get(id).await
    }

    async fn delete(&self, id: Uuid) -> Result<bool, StoreError> {
        let deleted = self
            .connection()?
            .execute("DELETE FROM todos WHERE id = ?1", [id.to_string()])
            .await?;
        Ok(deleted > 0)
    }
}

/// Fixed-width RFC 3339 in UTC, so the text sorts chronologically.
fn timestamp(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn todo_from_row(row: &Row) -> Result<Todo, StoreError> {
    let id: String = row.get(0)?;
    let created_at: String = row.get(3)?;
    let updated_at: String = row.get(4)?;

    Ok(Todo {
        id: Uuid::parse_str(&id).map_err(|error| StoreError::Corrupt(format!("id: {error}")))?,
        title: row.get(1)?,
        completed: row.get::<i64>(2)? != 0,
        created_at: parse_timestamp(&created_at)?,
        updated_at: parse_timestamp(&updated_at)?,
    })
}

fn parse_timestamp(text: &str) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(text)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| StoreError::Corrupt(format!("timestamp `{text}`: {error}")))
}

impl From<turso::Error> for StoreError {
    fn from(error: turso::Error) -> Self {
        Self::Database(error.into())
    }
}
