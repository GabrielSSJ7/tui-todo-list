//! In-test fake store. Named type, not an inline stub (per test policy):
//! commands tests drive this instead of touching SQLite.

use chrono::Utc;

use crate::error::{Result, TodoError};
use crate::model::{NewTask, Status, Task};
use crate::store::{StatusFilter, TaskStore};

#[derive(Default)]
pub struct FakeStore {
    tasks: Vec<Task>,
    next_id: i64,
}

impl FakeStore {
    pub fn new() -> Self {
        FakeStore { tasks: Vec::new(), next_id: 1 }
    }

    fn index_of(&self, id: i64) -> Result<usize> {
        self.tasks
            .iter()
            .position(|t| t.id == Some(id))
            .ok_or(TodoError::NotFound(id))
    }
}

impl TaskStore for FakeStore {
    fn add(&mut self, task: NewTask) -> Result<Task> {
        let stored = Task {
            id: Some(self.next_id),
            title: task.title,
            status: Status::Open,
            priority: task.priority,
            created_at: Utc::now(),
        };
        self.next_id += 1;
        self.tasks.push(stored.clone());
        Ok(stored)
    }

    fn list(&self, filter: StatusFilter) -> Result<Vec<Task>> {
        let out = self
            .tasks
            .iter()
            .filter(|t| match filter {
                StatusFilter::All => true,
                StatusFilter::Only(s) => t.status == s,
            })
            .cloned()
            .collect();
        Ok(out)
    }

    fn get(&self, id: i64) -> Result<Task> {
        Ok(self.tasks[self.index_of(id)?].clone())
    }

    fn set_status(&mut self, id: i64, status: Status) -> Result<Task> {
        let i = self.index_of(id)?;
        self.tasks[i].status = status;
        Ok(self.tasks[i].clone())
    }

    fn remove(&mut self, id: i64) -> Result<()> {
        let i = self.index_of(id)?;
        self.tasks.remove(i);
        Ok(())
    }

    fn open_count(&self) -> Result<usize> {
        Ok(self.tasks.iter().filter(|t| t.status == Status::Open).count())
    }
}
