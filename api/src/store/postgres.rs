use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use super::{StoreError, TodoStore};
use crate::domain::{self, StatusFilter, Todo, UpdateTodo};

pub struct PostgresStore {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct TodoRow {
    id: Uuid,
    title: String,
    completed: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PostgresStore {
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(url)
            .await?;
        sqlx::migrate!("migrations/postgres")
            .run(&pool)
            .await
            .map_err(|error| StoreError::Database(error.into()))?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl TodoStore for PostgresStore {
    async fn ping(&self) -> Result<(), StoreError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn list(&self, filter: StatusFilter) -> Result<Vec<Todo>, StoreError> {
        let rows: Vec<TodoRow> = sqlx::query_as(
            "SELECT id, title, completed, created_at, updated_at FROM todos \
             WHERE $1::boolean IS NULL OR completed = $1 \
             ORDER BY created_at, id",
        )
        .bind(filter.completed())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Todo::from).collect())
    }

    async fn get(&self, id: Uuid) -> Result<Option<Todo>, StoreError> {
        let row: Option<TodoRow> = sqlx::query_as(
            "SELECT id, title, completed, created_at, updated_at FROM todos WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Todo::from))
    }

    async fn insert(&self, todo: &Todo) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO todos (id, title, completed, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(todo.id)
        .bind(&todo.title)
        .bind(todo.completed)
        .bind(todo.created_at)
        .bind(todo.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update(&self, id: Uuid, changes: &UpdateTodo) -> Result<Option<Todo>, StoreError> {
        let row: Option<TodoRow> = sqlx::query_as(
            "UPDATE todos SET \
                title = COALESCE($2, title), \
                completed = COALESCE($3, completed), \
                updated_at = $4 \
             WHERE id = $1 \
             RETURNING id, title, completed, created_at, updated_at",
        )
        .bind(id)
        .bind(changes.title.as_deref())
        .bind(changes.completed)
        .bind(domain::now())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Todo::from))
    }

    async fn delete(&self, id: Uuid) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM todos WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

impl From<TodoRow> for Todo {
    fn from(row: TodoRow) -> Self {
        Self {
            id: row.id,
            title: row.title,
            completed: row.completed,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error.into())
    }
}
