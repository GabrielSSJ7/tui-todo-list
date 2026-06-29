//! rusqlite-backed `TaskStore`. The only module that imports rusqlite.

use chrono::{DateTime, Utc};
use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection, OptionalExtension, Row};
use std::path::Path;
use std::str::FromStr;

use crate::error::{Result, TodoError};
use crate::model::{NewTask, Priority, Project, Status, Task, INBOX_NAME};
use crate::store::{ProjectFilter, StatusFilter, TaskQuery, TaskStore};

/// High priority first, then newest. Shared by every task listing.
const TASK_ORDER: &str =
    "ORDER BY CASE priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END, created_at DESC";

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
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS projects (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT    NOT NULL UNIQUE,
                created_at TEXT    NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tasks (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                title      TEXT    NOT NULL,
                status     TEXT    NOT NULL DEFAULT 'open',
                priority   TEXT    NOT NULL DEFAULT 'medium',
                project_id INTEGER NOT NULL DEFAULT 1 REFERENCES projects(id),
                created_at TEXT    NOT NULL
            );",
        )
        .map_err(storage)?;
        self.seed_inbox()?;
        self.ensure_project_column()?;
        Ok(())
    }

    /// Guarantee the default Inbox project exists. Idempotent.
    fn seed_inbox(&self) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO projects (name, created_at) VALUES (?1, ?2)",
                rusqlite::params![INBOX_NAME, Utc::now().to_rfc3339()],
            )
            .map_err(storage)?;
        Ok(())
    }

    /// Add tasks.project_id and backfill to Inbox for databases created
    /// before projects existed. Safe to run every startup.
    ///
    /// SQLite forbids `ALTER TABLE ADD COLUMN` with a REFERENCES clause or a
    /// non-NULL default on a populated table, so we add a plain nullable
    /// column and backfill it; integrity is enforced by the app layer.
    fn ensure_project_column(&self) -> Result<()> {
        if self.has_column("tasks", "project_id")? {
            return Ok(());
        }
        let inbox_id = self.inbox()?.id.expect("seeded inbox has id");
        self.conn
            .execute_batch("ALTER TABLE tasks ADD COLUMN project_id INTEGER;")
            .map_err(storage)?;
        self.conn
            .execute(
                "UPDATE tasks SET project_id = ?1 WHERE project_id IS NULL",
                [inbox_id],
            )
            .map_err(storage)?;
        Ok(())
    }

    fn has_column(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(storage)?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>("name"))
            .map_err(storage)?;
        for name in names {
            if name.map_err(storage)? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Run a parameterized task SELECT and collect rows.
    fn query_tasks(&self, sql: &str, params: Vec<Value>) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(sql).map_err(storage)?;
        let rows = stmt
            .query_map(params_from_iter(params), row_to_task)
            .map_err(storage)?;
        let mut tasks = Vec::new();
        for r in rows {
            tasks.push(r.map_err(storage)?);
        }
        Ok(tasks)
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
        project_id: row.get("project_id")?,
        created_at: parse_ts(&created_raw),
    })
}

fn row_to_project(row: &Row<'_>) -> rusqlite::Result<Project> {
    let created_raw: String = row.get("created_at")?;
    Ok(Project {
        id: Some(row.get("id")?),
        name: row.get("name")?,
        created_at: parse_ts(&created_raw),
    })
}

fn parse_ts(raw: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

impl TaskStore for SqliteStore {
    fn add(&mut self, task: NewTask) -> Result<Task> {
        let created = Utc::now();
        self.conn
            .execute(
                "INSERT INTO tasks (title, status, priority, project_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    task.title,
                    Status::Open.as_tag(),
                    task.priority.as_tag(),
                    task.project_id,
                    created.to_rfc3339(),
                ],
            )
            .map_err(storage)?;
        Ok(Task {
            id: Some(self.conn.last_insert_rowid()),
            title: task.title,
            status: Status::Open,
            priority: task.priority,
            project_id: task.project_id,
            created_at: created,
        })
    }

    fn list(&self, query: TaskQuery) -> Result<Vec<Task>> {
        let mut conditions: Vec<&str> = Vec::new();
        let mut params: Vec<Value> = Vec::new();
        if let StatusFilter::Only(status) = query.status {
            conditions.push("status = ?");
            params.push(Value::Text(status.as_tag().to_string()));
        }
        if let ProjectFilter::Only(pid) = query.project {
            conditions.push("project_id = ?");
            params.push(Value::Integer(pid));
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!("SELECT * FROM tasks {where_clause} {TASK_ORDER}");
        self.query_tasks(&sql, params)
    }

    fn get(&self, id: i64) -> Result<Task> {
        self.conn
            .query_row("SELECT * FROM tasks WHERE id = ?1", [id], row_to_task)
            .optional()
            .map_err(storage)?
            .ok_or(TodoError::NotFound(id))
    }

    fn set_status(&mut self, id: i64, status: Status) -> Result<Task> {
        self.update_task(id, "status", Value::Text(status.as_tag().to_string()))
    }

    fn set_priority(&mut self, id: i64, priority: Priority) -> Result<Task> {
        self.update_task(id, "priority", Value::Text(priority.as_tag().to_string()))
    }

    fn set_project(&mut self, id: i64, project_id: i64) -> Result<Task> {
        // Surface a clear error rather than a raw FK violation.
        self.get_project(project_id)?;
        self.update_task(id, "project_id", Value::Integer(project_id))
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

    fn restore_task(&mut self, task: &Task) -> Result<()> {
        let id = task.id.ok_or_else(|| {
            TodoError::Invalid("cannot restore a task without an id".to_string())
        })?;
        self.conn
            .execute(
                "INSERT INTO tasks (id, title, status, priority, project_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    id,
                    task.title,
                    task.status.as_tag(),
                    task.priority.as_tag(),
                    task.project_id,
                    task.created_at.to_rfc3339(),
                ],
            )
            .map_err(storage)?;
        Ok(())
    }

    fn open_count(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE status = 'open'", [], |r| {
                r.get(0)
            })
            .map_err(storage)?;
        Ok(n as usize)
    }

    fn add_project(&mut self, name: &str) -> Result<Project> {
        if self.find_project(name)?.is_some() {
            return Err(TodoError::Invalid(format!(
                "project {name:?} already exists"
            )));
        }
        let created = Utc::now();
        self.conn
            .execute(
                "INSERT INTO projects (name, created_at) VALUES (?1, ?2)",
                rusqlite::params![name, created.to_rfc3339()],
            )
            .map_err(storage)?;
        Ok(Project {
            id: Some(self.conn.last_insert_rowid()),
            name: name.to_string(),
            created_at: created,
        })
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        // Inbox always first, then alphabetical.
        let sql = format!(
            "SELECT * FROM projects ORDER BY CASE name WHEN '{INBOX_NAME}' THEN 0 ELSE 1 END, name"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(storage)?;
        let rows = stmt.query_map([], row_to_project).map_err(storage)?;
        let mut projects = Vec::new();
        for r in rows {
            projects.push(r.map_err(storage)?);
        }
        Ok(projects)
    }

    fn find_project(&self, name: &str) -> Result<Option<Project>> {
        self.conn
            .query_row(
                "SELECT * FROM projects WHERE name = ?1",
                [name],
                row_to_project,
            )
            .optional()
            .map_err(storage)
    }

    fn inbox(&self) -> Result<Project> {
        self.find_project(INBOX_NAME)?
            .ok_or_else(|| TodoError::Storage("Inbox project missing".to_string()))
    }

    fn remove_project(&mut self, id: i64) -> Result<()> {
        let inbox = self.inbox()?;
        let inbox_id = inbox.id.expect("seeded inbox has id");
        if id == inbox_id {
            return Err(TodoError::Invalid("cannot delete the Inbox project".to_string()));
        }
        self.get_project(id)?; // 404 if missing
        // Orphaned tasks fall back to Inbox rather than being deleted.
        self.conn
            .execute(
                "UPDATE tasks SET project_id = ?1 WHERE project_id = ?2",
                rusqlite::params![inbox_id, id],
            )
            .map_err(storage)?;
        self.conn
            .execute("DELETE FROM projects WHERE id = ?1", [id])
            .map_err(storage)?;
        Ok(())
    }
}

impl SqliteStore {
    /// Apply a single-column UPDATE to one task, 404 if it doesn't exist.
    fn update_task(&self, id: i64, column: &str, value: Value) -> Result<Task> {
        let sql = format!("UPDATE tasks SET {column} = ?1 WHERE id = ?2");
        let changed = self
            .conn
            .execute(&sql, params_from_iter([value, Value::Integer(id)]))
            .map_err(storage)?;
        if changed == 0 {
            return Err(TodoError::NotFound(id));
        }
        self.get(id)
    }

    fn get_project(&self, id: i64) -> Result<Project> {
        self.conn
            .query_row("SELECT * FROM projects WHERE id = ?1", [id], row_to_project)
            .optional()
            .map_err(storage)?
            .ok_or(TodoError::NotFound(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SqliteStore {
        SqliteStore::in_memory().expect("in-memory db opens")
    }

    fn new_task(store: &SqliteStore, title: &str, p: Priority) -> NewTask {
        let inbox = store.inbox().unwrap().id.unwrap();
        NewTask::new(title, p, inbox).unwrap()
    }

    #[test]
    fn migrate_seeds_inbox() {
        let s = store();
        let inbox = s.inbox().unwrap();
        assert_eq!(inbox.name, INBOX_NAME);
        assert_eq!(s.list_projects().unwrap().len(), 1);
    }

    #[test]
    fn add_then_get_roundtrips_with_project() {
        let mut s = store();
        let t = new_task(&s, "write tests", Priority::High);
        let added = s.add(t).unwrap();
        let fetched = s.get(added.id.unwrap()).unwrap();
        assert_eq!(fetched.title, "write tests");
        assert_eq!(fetched.project_id, s.inbox().unwrap().id.unwrap());
    }

    #[test]
    fn list_orders_high_priority_first() {
        let mut s = store();
        let low = new_task(&s, "low", Priority::Low);
        let high = new_task(&s, "high", Priority::High);
        s.add(low).unwrap();
        s.add(high).unwrap();
        let all = s.list(TaskQuery::all()).unwrap();
        assert_eq!(all.first().unwrap().title, "high");
    }

    #[test]
    fn list_filters_by_project() {
        let mut s = store();
        let work = s.add_project("Work").unwrap();
        let inbox_task = new_task(&s, "inbox one", Priority::Medium);
        s.add(inbox_task).unwrap();
        let work_task = NewTask::new("work one", Priority::Medium, work.id.unwrap()).unwrap();
        s.add(work_task).unwrap();
        let only_work = s.list(TaskQuery::all().in_project(work.id.unwrap())).unwrap();
        assert_eq!(only_work.len(), 1);
        assert_eq!(only_work[0].title, "work one");
    }

    #[test]
    fn set_project_moves_task() {
        let mut s = store();
        let work = s.add_project("Work").unwrap();
        let t = s.add(new_task(&s, "movable", Priority::Low)).unwrap();
        let moved = s.set_project(t.id.unwrap(), work.id.unwrap()).unwrap();
        assert_eq!(moved.project_id, work.id.unwrap());
    }

    #[test]
    fn add_duplicate_project_errors() {
        let mut s = store();
        s.add_project("Work").unwrap();
        assert!(matches!(s.add_project("Work"), Err(TodoError::Invalid(_))));
    }

    #[test]
    fn remove_project_reassigns_tasks_to_inbox() {
        let mut s = store();
        let work = s.add_project("Work").unwrap();
        let t = NewTask::new("orphan me", Priority::Low, work.id.unwrap()).unwrap();
        let task = s.add(t).unwrap();
        s.remove_project(work.id.unwrap()).unwrap();
        let reloaded = s.get(task.id.unwrap()).unwrap();
        assert_eq!(reloaded.project_id, s.inbox().unwrap().id.unwrap());
    }

    #[test]
    fn cannot_remove_inbox() {
        let mut s = store();
        let inbox_id = s.inbox().unwrap().id.unwrap();
        assert!(matches!(
            s.remove_project(inbox_id),
            Err(TodoError::Invalid(_))
        ));
    }

    #[test]
    fn set_status_filters_and_counts() {
        let mut s = store();
        let t = s.add(new_task(&s, "close me", Priority::Medium)).unwrap();
        s.set_status(t.id.unwrap(), Status::Done).unwrap();
        assert_eq!(s.open_count().unwrap(), 0);
        let done = s.list(TaskQuery::all().with_status(StatusFilter::Only(Status::Done)));
        assert_eq!(done.unwrap().len(), 1);
    }

    #[test]
    fn restore_task_reinserts_with_same_id() {
        let mut s = store();
        let t = s.add(new_task(&s, "bring back", Priority::High)).unwrap();
        let id = t.id.unwrap();
        s.remove(id).unwrap();
        assert!(matches!(s.get(id), Err(TodoError::NotFound(_))));
        s.restore_task(&t).unwrap();
        let back = s.get(id).unwrap();
        assert_eq!(back.title, "bring back");
        assert_eq!(back.priority, Priority::High);
    }

    #[test]
    fn missing_id_errors_not_found() {
        let mut s = store();
        assert!(matches!(s.get(999), Err(TodoError::NotFound(999))));
        assert!(matches!(s.remove(999), Err(TodoError::NotFound(999))));
    }
}
