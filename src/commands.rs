//! Non-interactive command handlers. Each takes a store and returns the
//! user-facing text to print — so they're testable without capturing stdout.

use crate::error::{Result, TodoError};
use crate::model::{NewTask, Priority, Status, Task};
use crate::store::{StatusFilter, TaskStore};

/// Add a task and confirm it.
pub fn add(store: &mut dyn TaskStore, title: &str, priority: Priority) -> Result<String> {
    let new = NewTask::new(title, priority).map_err(TodoError::Invalid)?;
    let task = store.add(new)?;
    Ok(format!("added #{} {}", task.id.unwrap_or_default(), task.title))
}

/// List tasks as aligned plain-text rows.
pub fn list(store: &dyn TaskStore, include_done: bool) -> Result<String> {
    let filter = if include_done {
        StatusFilter::All
    } else {
        StatusFilter::Only(Status::Open)
    };
    let tasks = store.list(filter)?;
    if tasks.is_empty() {
        return Ok("no tasks".to_string());
    }
    let lines: Vec<String> = tasks.iter().map(format_row).collect();
    Ok(lines.join("\n"))
}

/// One list row: `#id [x] (priority) title`.
fn format_row(task: &Task) -> String {
    let mark = match task.status {
        Status::Open => " ",
        Status::Done => "x",
    };
    format!(
        "#{:<4} [{}] {:<6} {}",
        task.id.unwrap_or_default(),
        mark,
        task.priority,
        task.title
    )
}

/// Mark a task done.
pub fn done(store: &mut dyn TaskStore, id: i64) -> Result<String> {
    let task = store.set_status(id, Status::Done)?;
    Ok(format!("done #{} {}", id, task.title))
}

/// Reopen a completed task.
pub fn reopen(store: &mut dyn TaskStore, id: i64) -> Result<String> {
    let task = store.set_status(id, Status::Open)?;
    Ok(format!("reopened #{} {}", id, task.title))
}

/// Delete a task.
pub fn remove(store: &mut dyn TaskStore, id: i64) -> Result<String> {
    store.remove(id)?;
    Ok(format!("removed #{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fake::FakeStore;

    fn seeded() -> FakeStore {
        let mut s = FakeStore::new();
        add(&mut s, "buy milk", Priority::High).unwrap();
        s
    }

    #[test]
    fn add_rejects_blank_title() {
        let mut s = FakeStore::new();
        assert!(matches!(add(&mut s, "   ", Priority::Low), Err(TodoError::Invalid(_))));
    }

    #[test]
    fn list_hides_done_by_default() {
        let mut s = seeded();
        done(&mut s, 1).unwrap();
        assert_eq!(list(&s, false).unwrap(), "no tasks");
        assert!(list(&s, true).unwrap().contains("buy milk"));
    }

    #[test]
    fn done_then_reopen_changes_status() {
        let mut s = seeded();
        assert!(done(&mut s, 1).unwrap().starts_with("done #1"));
        assert!(reopen(&mut s, 1).unwrap().starts_with("reopened #1"));
        assert!(list(&s, false).unwrap().contains("buy milk"));
    }

    #[test]
    fn remove_deletes() {
        let mut s = seeded();
        assert_eq!(remove(&mut s, 1).unwrap(), "removed #1");
        assert_eq!(list(&s, true).unwrap(), "no tasks");
    }

    #[test]
    fn missing_id_surfaces_not_found() {
        let mut s = FakeStore::new();
        assert!(matches!(done(&mut s, 7), Err(TodoError::NotFound(7))));
    }
}
