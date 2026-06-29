//! Storage abstraction owned by this project. Commands and the TUI depend
//! on `TaskStore`, never on rusqlite directly — so they stay testable with
//! a fake store and swappable to another backend later.

pub mod sqlite;

use crate::error::Result;
use crate::model::{NewTask, Priority, Project, Status, Task};

/// What the rest of the app needs from persistence. Kept small on purpose.
pub trait TaskStore {
    fn add(&mut self, task: NewTask) -> Result<Task>;
    fn list(&self, query: TaskQuery) -> Result<Vec<Task>>;
    fn get(&self, id: i64) -> Result<Task>;
    fn set_status(&mut self, id: i64, status: Status) -> Result<Task>;
    fn set_priority(&mut self, id: i64, priority: Priority) -> Result<Task>;
    fn set_project(&mut self, id: i64, project_id: i64) -> Result<Task>;
    fn remove(&mut self, id: i64) -> Result<()>;
    /// Re-insert a previously removed task, preserving its id and fields.
    /// Used to undo a deletion faithfully.
    fn restore_task(&mut self, task: &Task) -> Result<()>;

    /// Count of open tasks across all projects — for the tmux status line.
    fn open_count(&self) -> Result<usize>;

    // --- Projects ---

    /// Create a project. Errors if the name already exists.
    fn add_project(&mut self, name: &str) -> Result<Project>;
    fn list_projects(&self) -> Result<Vec<Project>>;
    fn find_project(&self, name: &str) -> Result<Option<Project>>;
    /// The seeded default project; every store guarantees it exists.
    fn inbox(&self) -> Result<Project>;
    /// Delete a project, reassigning its tasks to Inbox. Errors on Inbox.
    fn remove_project(&mut self, id: i64) -> Result<()>;
}

/// Which tasks `list` returns. Combines a status and a project filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskQuery {
    pub status: StatusFilter,
    pub project: ProjectFilter,
}

impl TaskQuery {
    /// All tasks in every project, any status.
    pub fn all() -> Self {
        TaskQuery { status: StatusFilter::All, project: ProjectFilter::Any }
    }

    pub fn with_status(mut self, status: StatusFilter) -> Self {
        self.status = status;
        self
    }

    pub fn in_project(mut self, project_id: i64) -> Self {
        self.project = ProjectFilter::Only(project_id);
        self
    }
}

/// Which tasks `list` returns by lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    All,
    Only(Status),
}

/// Which tasks `list` returns by project membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectFilter {
    Any,
    Only(i64),
}

#[cfg(test)]
pub mod fake;
