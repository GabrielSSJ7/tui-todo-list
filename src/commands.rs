//! Non-interactive command handlers. Each takes a store and returns the
//! user-facing text to print — so they're testable without capturing stdout.

use std::collections::HashMap;

use crate::error::{Result, TodoError};
use crate::model::{clean_project_name, NewTask, Priority, Status, Task};
use crate::store::{ProjectFilter, StatusFilter, TaskQuery, TaskStore};

/// Resolve a project name to its id, erroring if it doesn't exist.
/// `None` means the caller wants the default Inbox.
fn resolve_project(store: &dyn TaskStore, name: Option<&str>) -> Result<i64> {
    match name {
        None => Ok(store.inbox()?.id.expect("inbox has id")),
        Some(name) => {
            let clean = clean_project_name(name).map_err(TodoError::Invalid)?;
            store
                .find_project(&clean)?
                .and_then(|p| p.id)
                .ok_or_else(|| {
                    TodoError::Invalid(format!(
                        "unknown project {clean:?}; create it with `todo project add {clean:?}`"
                    ))
                })
        }
    }
}

/// Add a task to a project (Inbox if `project` is None).
pub fn add(
    store: &mut dyn TaskStore,
    title: &str,
    priority: Priority,
    project: Option<&str>,
) -> Result<String> {
    let project_id = resolve_project(store, project)?;
    let new = NewTask::new(title, priority, project_id).map_err(TodoError::Invalid)?;
    let task = store.add(new)?;
    Ok(format!("added #{} {}", task.id.unwrap_or_default(), task.title))
}

/// List tasks as aligned plain-text rows, optionally scoped to one project.
pub fn list(
    store: &dyn TaskStore,
    include_done: bool,
    project: Option<&str>,
) -> Result<String> {
    let status = if include_done {
        StatusFilter::All
    } else {
        StatusFilter::Only(Status::Open)
    };
    let mut query = TaskQuery::all().with_status(status);
    if project.is_some() {
        query.project = ProjectFilter::Only(resolve_project(store, project)?);
    }
    let tasks = store.list(query)?;
    if tasks.is_empty() {
        return Ok("no tasks".to_string());
    }
    let names = project_names(store)?;
    let lines: Vec<String> = tasks.iter().map(|t| format_row(t, &names)).collect();
    Ok(lines.join("\n"))
}

/// Map project id → name for annotating task rows.
fn project_names(store: &dyn TaskStore) -> Result<HashMap<i64, String>> {
    let map = store
        .list_projects()?
        .into_iter()
        .filter_map(|p| p.id.map(|id| (id, p.name)))
        .collect();
    Ok(map)
}

/// One list row: `#id [x] priority @Project title`.
fn format_row(task: &Task, names: &HashMap<i64, String>) -> String {
    let mark = match task.status {
        Status::Open => " ",
        Status::Done => "x",
    };
    let unknown = "?".to_string();
    let project = names.get(&task.project_id).unwrap_or(&unknown);
    format!(
        "#{:<4} [{}] {:<6} @{:<10} {}",
        task.id.unwrap_or_default(),
        mark,
        task.priority,
        project,
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

/// Change a task's priority.
pub fn set_priority(store: &mut dyn TaskStore, id: i64, priority: Priority) -> Result<String> {
    let task = store.set_priority(id, priority)?;
    Ok(format!("#{} priority {} {}", id, task.priority, task.title))
}

/// Move a task to another project by name.
pub fn move_task(store: &mut dyn TaskStore, id: i64, project: &str) -> Result<String> {
    let project_id = resolve_project(store, Some(project))?;
    let task = store.set_project(id, project_id)?;
    Ok(format!("moved #{} ({}) to @{}", id, task.title, project.trim()))
}

/// Delete a task.
pub fn remove(store: &mut dyn TaskStore, id: i64) -> Result<String> {
    store.remove(id)?;
    Ok(format!("removed #{id}"))
}

/// Create a project.
pub fn add_project(store: &mut dyn TaskStore, name: &str) -> Result<String> {
    let clean = clean_project_name(name).map_err(TodoError::Invalid)?;
    let project = store.add_project(&clean)?;
    Ok(format!("created project @{}", project.name))
}

/// List projects with their open-task counts.
pub fn list_projects(store: &dyn TaskStore) -> Result<String> {
    let projects = store.list_projects()?;
    let mut lines = Vec::new();
    for p in projects {
        let id = p.id.expect("listed project has id");
        let open = store
            .list(
                TaskQuery::all()
                    .with_status(StatusFilter::Only(Status::Open))
                    .in_project(id),
            )?
            .len();
        lines.push(format!("#{:<4} @{:<12} {} open", id, p.name, open));
    }
    Ok(lines.join("\n"))
}

/// Delete a project; its tasks fall back to Inbox.
pub fn remove_project(store: &mut dyn TaskStore, id: i64) -> Result<String> {
    store.remove_project(id)?;
    Ok(format!("removed project #{id}; its tasks moved to Inbox"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fake::FakeStore;

    fn seeded() -> FakeStore {
        let mut s = FakeStore::new();
        add(&mut s, "buy milk", Priority::High, None).unwrap();
        s
    }

    #[test]
    fn add_rejects_blank_title() {
        let mut s = FakeStore::new();
        assert!(matches!(
            add(&mut s, "   ", Priority::Low, None),
            Err(TodoError::Invalid(_))
        ));
    }

    #[test]
    fn add_to_unknown_project_errors() {
        let mut s = FakeStore::new();
        assert!(matches!(
            add(&mut s, "task", Priority::Low, Some("Ghost")),
            Err(TodoError::Invalid(_))
        ));
    }

    #[test]
    fn add_into_named_project() {
        let mut s = FakeStore::new();
        add_project(&mut s, "Work").unwrap();
        add(&mut s, "ship it", Priority::High, Some("Work")).unwrap();
        let work_only = list(&s, true, Some("Work")).unwrap();
        assert!(work_only.contains("ship it"));
        assert!(work_only.contains("@Work"));
    }

    #[test]
    fn list_hides_done_by_default() {
        let mut s = seeded();
        done(&mut s, 1).unwrap();
        assert_eq!(list(&s, false, None).unwrap(), "no tasks");
        assert!(list(&s, true, None).unwrap().contains("buy milk"));
    }

    #[test]
    fn move_task_changes_project() {
        let mut s = seeded();
        add_project(&mut s, "Work").unwrap();
        let msg = move_task(&mut s, 1, "Work").unwrap();
        assert!(msg.contains("@Work"));
        assert!(list(&s, true, Some("Work")).unwrap().contains("buy milk"));
    }

    #[test]
    fn list_projects_counts_open() {
        let mut s = seeded(); // 1 open task in Inbox
        let out = list_projects(&s).unwrap();
        assert!(out.contains("@Inbox"));
        assert!(out.contains("1 open"));
    }

    #[test]
    fn remove_project_moves_tasks_to_inbox() {
        let mut s = FakeStore::new();
        let work = s.add_project("Work").unwrap();
        add(&mut s, "stay alive", Priority::Low, Some("Work")).unwrap();
        remove_project(&mut s, work.id.unwrap()).unwrap();
        assert!(list(&s, true, None).unwrap().contains("stay alive"));
    }

    #[test]
    fn missing_id_surfaces_not_found() {
        let mut s = FakeStore::new();
        assert!(matches!(done(&mut s, 7), Err(TodoError::NotFound(7))));
    }
}
