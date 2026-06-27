//! Storage abstraction owned by this project. Commands and the TUI depend
//! on `TaskStore`, never on rusqlite directly — so they stay testable with
//! a fake store and swappable to another backend later.

pub mod sqlite;

use crate::error::Result;
use crate::model::{NewTask, Status, Task};

/// What the rest of the app needs from persistence. Kept small on purpose.
pub trait TaskStore {
    fn add(&mut self, task: NewTask) -> Result<Task>;
    fn list(&self, filter: StatusFilter) -> Result<Vec<Task>>;
    fn get(&self, id: i64) -> Result<Task>;
    fn set_status(&mut self, id: i64, status: Status) -> Result<Task>;
    fn remove(&mut self, id: i64) -> Result<()>;

    /// Count of open tasks — cheap query for the tmux status line.
    fn open_count(&self) -> Result<usize>;
}

/// Which tasks `list` returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    All,
    Only(Status),
}

#[cfg(test)]
pub mod fake;
