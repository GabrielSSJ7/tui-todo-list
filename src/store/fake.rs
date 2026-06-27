//! In-test fake store. Named type, not an inline stub (per test policy):
//! commands/TUI tests drive this instead of touching SQLite.

use chrono::Utc;

use crate::error::{Result, TodoError};
use crate::model::{NewTask, Priority, Project, Status, Task, INBOX_NAME};
use crate::store::{ProjectFilter, StatusFilter, TaskQuery, TaskStore};

pub struct FakeStore {
    tasks: Vec<Task>,
    projects: Vec<Project>,
    next_task_id: i64,
    next_project_id: i64,
}

impl Default for FakeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeStore {
    /// Starts seeded with an Inbox project (id 1), matching SqliteStore.
    pub fn new() -> Self {
        let inbox = Project {
            id: Some(1),
            name: INBOX_NAME.to_string(),
            created_at: Utc::now(),
        };
        FakeStore {
            tasks: Vec::new(),
            projects: vec![inbox],
            next_task_id: 1,
            next_project_id: 2,
        }
    }

    fn task_index(&self, id: i64) -> Result<usize> {
        self.tasks
            .iter()
            .position(|t| t.id == Some(id))
            .ok_or(TodoError::NotFound(id))
    }
}

impl TaskStore for FakeStore {
    fn add(&mut self, task: NewTask) -> Result<Task> {
        let stored = Task {
            id: Some(self.next_task_id),
            title: task.title,
            status: Status::Open,
            priority: task.priority,
            project_id: task.project_id,
            created_at: Utc::now(),
        };
        self.next_task_id += 1;
        self.tasks.push(stored.clone());
        Ok(stored)
    }

    fn list(&self, query: TaskQuery) -> Result<Vec<Task>> {
        let out = self
            .tasks
            .iter()
            .filter(|t| match query.status {
                StatusFilter::All => true,
                StatusFilter::Only(s) => t.status == s,
            })
            .filter(|t| match query.project {
                ProjectFilter::Any => true,
                ProjectFilter::Only(pid) => t.project_id == pid,
            })
            .cloned()
            .collect();
        Ok(out)
    }

    fn get(&self, id: i64) -> Result<Task> {
        Ok(self.tasks[self.task_index(id)?].clone())
    }

    fn set_status(&mut self, id: i64, status: Status) -> Result<Task> {
        let i = self.task_index(id)?;
        self.tasks[i].status = status;
        Ok(self.tasks[i].clone())
    }

    fn set_priority(&mut self, id: i64, priority: Priority) -> Result<Task> {
        let i = self.task_index(id)?;
        self.tasks[i].priority = priority;
        Ok(self.tasks[i].clone())
    }

    fn set_project(&mut self, id: i64, project_id: i64) -> Result<Task> {
        if !self.projects.iter().any(|p| p.id == Some(project_id)) {
            return Err(TodoError::NotFound(project_id));
        }
        let i = self.task_index(id)?;
        self.tasks[i].project_id = project_id;
        Ok(self.tasks[i].clone())
    }

    fn remove(&mut self, id: i64) -> Result<()> {
        let i = self.task_index(id)?;
        self.tasks.remove(i);
        Ok(())
    }

    fn open_count(&self) -> Result<usize> {
        Ok(self.tasks.iter().filter(|t| t.status == Status::Open).count())
    }

    fn add_project(&mut self, name: &str) -> Result<Project> {
        if self.find_project(name)?.is_some() {
            return Err(TodoError::Invalid(format!("project {name:?} already exists")));
        }
        let project = Project {
            id: Some(self.next_project_id),
            name: name.to_string(),
            created_at: Utc::now(),
        };
        self.next_project_id += 1;
        self.projects.push(project.clone());
        Ok(project)
    }

    fn list_projects(&self) -> Result<Vec<Project>> {
        Ok(self.projects.clone())
    }

    fn find_project(&self, name: &str) -> Result<Option<Project>> {
        Ok(self.projects.iter().find(|p| p.name == name).cloned())
    }

    fn inbox(&self) -> Result<Project> {
        self.find_project(INBOX_NAME)?
            .ok_or_else(|| TodoError::Storage("Inbox project missing".to_string()))
    }

    fn remove_project(&mut self, id: i64) -> Result<()> {
        let inbox_id = self.inbox()?.id.expect("seeded inbox has id");
        if id == inbox_id {
            return Err(TodoError::Invalid("cannot delete the Inbox project".to_string()));
        }
        let i = self
            .projects
            .iter()
            .position(|p| p.id == Some(id))
            .ok_or(TodoError::NotFound(id))?;
        for task in self.tasks.iter_mut().filter(|t| t.project_id == id) {
            task.project_id = inbox_id;
        }
        self.projects.remove(i);
        Ok(())
    }
}
