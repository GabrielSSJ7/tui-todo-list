//! rusqlite-backed `TaskStore`. The only module that imports rusqlite.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Row};
use std::path::Path;
use std::str::FromStr;

use crate::error::{Result, TodoError};
use crate::model::{NewTask, Priority, Status, Task};
use crate::store::{StatusFilter, TaskStore};

/// Map any rusqlite error into our storage error, preserving the message.
fn storage<E: std::fmt::Display>(e: E) -> TodoError {
    TodoError::Storage(e.to_string())
}

pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    /// Open (creating if absent) a database file and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(storage)?;
        let store = SqliteStore { conn };
        store.migrate()?;
        Ok(store)
    }

    /// In-memory database — used by tests and ephemeral runs.
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(storage)?;
        let store = SqliteStore { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS tasks (
                    id         INTEGER PRIMARY KEY AUTOINCREMENT,
                    title      TEXT    NOT NULL,
                    status     TEXT    NOT NULL DEFAULT 'open',
                    priority   TEXT    NOT NULL DEFAULT 'medium',
                    created_at TEXT    NOT NULL
                );",
            )
            .map_err(storage)
    }
}

/// Build a `Task` from a result row. Centralized so column order lives once.
fn row_to_task(row: &Row<'_>) -> rusqlite::Result<Task> {
    let status_tag: String = row.get("status")?;
    let priority_tag: String = row.get("priority")?;
    let created_raw: String = row.get("created_at")?;
    Ok(Task {
        id: Some(row.get("id")?),
        title: row.get("title")?,
        status: Status::from_str(&status_tag).unwrap_or(Status::Open),
        priority: Priority::from_str(&priority_tag).unwrap_or(Priority::Medium),
        created_at: DateTime::parse_from_rfc3339(&created_raw)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

impl TaskStore for SqliteStore {
    fn add(&mut self, task: NewTask) -> Result<Task> {
        let created = Utc::now();
        self.conn
            .execute(
                "INSERT INTO tasks (title, status, priority, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    task.title,
                    Status::Open.as_tag(),
                    task.priority.as_tag(),
                    created.to_rfc3339(),
                ],
            )
            .map_err(storage)?;
        let id = self.conn.last_insert_rowid();
        Ok(Task {
            id: Some(id),
            title: task.title,
            status: Status::Open,
            priority: task.priority,
            created_at: created,
        })
    }

    fn list(&self, filter: StatusFilter) -> Result<Vec<Task>> {
        // High priority first, then newest. Two query shapes by filter.
        let order = "ORDER BY CASE priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END,
                     created_at DESC";
        let mut tasks = Vec::new();
        match filter {
            StatusFilter::All => {
                let sql = format!("SELECT * FROM tasks {order}");
                let mut stmt = self.conn.prepare(&sql).map_err(storage)?;
                let rows = stmt.query_map([], row_to_task).map_err(storage)?;
                for r in rows {
                    tasks.push(r.map_err(storage)?);
                }
            }
            StatusFilter::Only(status) => {
                let sql = format!("SELECT * FROM tasks WHERE status = ?1 {order}");
                let mut stmt = self.conn.prepare(&sql).map_err(storage)?;
                let rows = stmt
                    .query_map([status.as_tag()], row_to_task)
                    .map_err(storage)?;
                for r in rows {
                    tasks.push(r.map_err(storage)?);
                }
            }
        }
        Ok(tasks)
    }

    fn get(&self, id: i64) -> Result<Task> {
        self.conn
            .query_row("SELECT * FROM tasks WHERE id = ?1", [id], row_to_task)
            .optional()
            .map_err(storage)?
            .ok_or(TodoError::NotFound(id))
    }

    fn set_status(&mut self, id: i64, status: Status) -> Result<Task> {
        let changed = self
            .conn
            .execute(
                "UPDATE tasks SET status = ?1 WHERE id = ?2",
                rusqlite::params![status.as_tag(), id],
            )
            .map_err(storage)?;
        if changed == 0 {
            return Err(TodoError::NotFound(id));
        }
        self.get(id)
    }

    fn remove(&mut self, id: i64) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM tasks WHERE id = ?1", [id])
            .map_err(storage)?;
        if changed == 0 {
            return Err(TodoError::NotFound(id));
        }
        Ok(())
    }

    fn open_count(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE status = 'open'",
                [],
                |row| row.get(0),
            )
            .map_err(storage)?;
        Ok(n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SqliteStore {
        SqliteStore::in_memory().expect("in-memory db opens")
    }

    fn new_task(title: &str, p: Priority) -> NewTask {
        NewTask::new(title, p).unwrap()
    }

    #[test]
    fn add_then_get_roundtrips() {
        let mut s = store();
        let added = s.add(new_task("write tests", Priority::High)).unwrap();
        let fetched = s.get(added.id.unwrap()).unwrap();
        assert_eq!(fetched.title, "write tests");
        assert_eq!(fetched.status, Status::Open);
        assert_eq!(fetched.priority, Priority::High);
    }

    #[test]
    fn list_orders_high_priority_first() {
        let mut s = store();
        s.add(new_task("low", Priority::Low)).unwrap();
        s.add(new_task("high", Priority::High)).unwrap();
        let all = s.list(StatusFilter::All).unwrap();
        assert_eq!(all.first().unwrap().title, "high");
    }

    #[test]
    fn set_status_filters_and_counts() {
        let mut s = store();
        let t = s.add(new_task("close me", Priority::Medium)).unwrap();
        s.set_status(t.id.unwrap(), Status::Done).unwrap();
        assert_eq!(s.open_count().unwrap(), 0);
        assert_eq!(s.list(StatusFilter::Only(Status::Done)).unwrap().len(), 1);
        assert_eq!(s.list(StatusFilter::Only(Status::Open)).unwrap().len(), 0);
    }

    #[test]
    fn missing_id_errors_not_found() {
        let mut s = store();
        assert!(matches!(s.get(999), Err(TodoError::NotFound(999))));
        assert!(matches!(s.remove(999), Err(TodoError::NotFound(999))));
        assert!(matches!(
            s.set_status(999, Status::Done),
            Err(TodoError::NotFound(999))
        ));
    }
}
