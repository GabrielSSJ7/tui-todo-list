//! TUI state machine. Pure logic: key in, state/store mutation out — no
//! terminal or rendering here, so it's unit-testable with a FakeStore.

use crossterm::event::{KeyCode, KeyEvent};

use crate::error::Result;
use crate::model::{clean_project_name, NewTask, Priority, Project, Status, Task};
use crate::store::{ProjectFilter, StatusFilter, TaskQuery, TaskStore};

/// Which pane has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    Tasks,
}

/// What the UI is currently doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Browsing.
    Normal,
    /// Typing a new task title; holds the in-progress buffer.
    AddingTask(String),
    /// Typing a new project name.
    AddingProject(String),
}

pub struct App<'a> {
    store: &'a mut dyn TaskStore,
    pub projects: Vec<Project>,
    pub selected_project: usize,
    pub tasks: Vec<Task>,
    pub selected: usize,
    pub focus: Focus,
    pub mode: Mode,
    pub show_done: bool,
    pub status: String,
    pub should_quit: bool,
}

impl<'a> App<'a> {
    pub fn new(store: &'a mut dyn TaskStore) -> Result<Self> {
        let mut app = App {
            store,
            projects: Vec::new(),
            selected_project: 0,
            tasks: Vec::new(),
            selected: 0,
            focus: Focus::Tasks,
            mode: Mode::Normal,
            show_done: true,
            status: "tab switch · a add · n project · space toggle · p pri · d del · q quit"
                .to_string(),
            should_quit: false,
        };
        app.refresh()?;
        Ok(app)
    }

    /// The currently highlighted project, if any.
    pub fn current_project(&self) -> Option<&Project> {
        self.projects.get(self.selected_project)
    }

    fn current_project_id(&self) -> Option<i64> {
        self.current_project().and_then(|p| p.id)
    }

    /// Reload projects and the selected project's tasks; clamp cursors.
    pub fn refresh(&mut self) -> Result<()> {
        self.projects = self.store.list_projects()?;
        if self.selected_project >= self.projects.len() {
            self.selected_project = self.projects.len().saturating_sub(1);
        }
        self.reload_tasks()
    }

    /// Reload only the task list for the selected project.
    fn reload_tasks(&mut self) -> Result<()> {
        let status = if self.show_done {
            StatusFilter::All
        } else {
            StatusFilter::Only(Status::Open)
        };
        let project = match self.current_project_id() {
            Some(id) => ProjectFilter::Only(id),
            None => ProjectFilter::Any,
        };
        self.tasks = self.store.list(TaskQuery { status, project })?;
        if self.selected >= self.tasks.len() {
            self.selected = self.tasks.len().saturating_sub(1);
        }
        Ok(())
    }

    /// Route a key to the active mode's handler.
    pub fn on_key(&mut self, key: KeyEvent) -> Result<()> {
        match &self.mode {
            Mode::Normal => self.on_normal_key(key),
            Mode::AddingTask(_) => self.on_task_input(key),
            Mode::AddingProject(_) => self.on_project_input(key),
        }
    }

    fn on_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab | KeyCode::BackTab => self.toggle_focus(),
            KeyCode::Char('n') => self.mode = Mode::AddingProject(String::new()),
            KeyCode::Char('h') => self.toggle_show_done()?,
            KeyCode::Char('j') | KeyCode::Down => self.move_down()?,
            KeyCode::Char('k') | KeyCode::Up => self.move_up()?,
            _ => self.on_action_key(key)?,
        }
        Ok(())
    }

    /// Keys that act on tasks — only meaningful when the Tasks pane is focused.
    fn on_action_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.focus != Focus::Tasks {
            return Ok(());
        }
        match key.code {
            KeyCode::Char('a') => self.mode = Mode::AddingTask(String::new()),
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_selected()?,
            KeyCode::Char('d') => self.delete_selected()?,
            KeyCode::Char('p') => self.cycle_priority()?,
            _ => {}
        }
        Ok(())
    }

    fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Projects => Focus::Tasks,
            Focus::Tasks => Focus::Projects,
        };
    }

    fn move_down(&mut self) -> Result<()> {
        match self.focus {
            Focus::Tasks => self.move_cursor(1),
            Focus::Projects => self.move_project(1)?,
        }
        Ok(())
    }

    fn move_up(&mut self) -> Result<()> {
        match self.focus {
            Focus::Tasks => self.move_cursor(-1),
            Focus::Projects => self.move_project(-1)?,
        }
        Ok(())
    }

    fn move_cursor(&mut self, delta: i32) {
        self.selected = clamp_index(self.selected, delta, self.tasks.len());
    }

    /// Move the project selection and reload that project's tasks.
    fn move_project(&mut self, delta: i32) -> Result<()> {
        self.selected_project = clamp_index(self.selected_project, delta, self.projects.len());
        self.selected = 0;
        self.reload_tasks()
    }

    fn selected_id(&self) -> Option<i64> {
        self.tasks.get(self.selected).and_then(|t| t.id)
    }

    fn toggle_selected(&mut self) -> Result<()> {
        let Some(task) = self.tasks.get(self.selected) else {
            return Ok(());
        };
        let next = match task.status {
            Status::Open => Status::Done,
            Status::Done => Status::Open,
        };
        let id = task.id.expect("listed task has an id");
        self.store.set_status(id, next)?;
        self.reload_tasks()
    }

    fn delete_selected(&mut self) -> Result<()> {
        let Some(id) = self.selected_id() else {
            return Ok(());
        };
        self.store.remove(id)?;
        self.status = format!("removed #{id}");
        self.reload_tasks()
    }

    /// Bump the selected task's priority one step, wrapping High → Low.
    fn cycle_priority(&mut self) -> Result<()> {
        let Some(task) = self.tasks.get(self.selected) else {
            return Ok(());
        };
        let next = match task.priority {
            Priority::Low => Priority::Medium,
            Priority::Medium => Priority::High,
            Priority::High => Priority::Low,
        };
        let id = task.id.expect("listed task has an id");
        self.store.set_priority(id, next)?;
        self.reload_tasks()
    }

    fn toggle_show_done(&mut self) -> Result<()> {
        self.show_done = !self.show_done;
        self.reload_tasks()
    }

    fn on_task_input(&mut self, key: KeyEvent) -> Result<()> {
        let Mode::AddingTask(buf) = &mut self.mode else {
            return Ok(());
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => self.commit_task()?,
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) => buf.push(c),
            _ => {}
        }
        Ok(())
    }

    fn on_project_input(&mut self, key: KeyEvent) -> Result<()> {
        let Mode::AddingProject(buf) = &mut self.mode else {
            return Ok(());
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => self.commit_project()?,
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) => buf.push(c),
            _ => {}
        }
        Ok(())
    }

    fn commit_task(&mut self) -> Result<()> {
        let Mode::AddingTask(buf) = &self.mode else {
            return Ok(());
        };
        let Some(project_id) = self.current_project_id() else {
            self.status = "no project selected".to_string();
            self.mode = Mode::Normal;
            return Ok(());
        };
        // Blank input just cancels — no error popup needed mid-flow.
        match NewTask::new(buf.clone(), Priority::Medium, project_id) {
            Ok(new) => {
                let task = self.store.add(new)?;
                self.status = format!("added #{}", task.id.unwrap_or_default());
            }
            Err(_) => self.status = "empty title — not added".to_string(),
        }
        self.mode = Mode::Normal;
        self.reload_tasks()
    }

    fn commit_project(&mut self) -> Result<()> {
        let Mode::AddingProject(buf) = &self.mode else {
            return Ok(());
        };
        match clean_project_name(buf.clone()) {
            Ok(name) => match self.store.add_project(&name) {
                Ok(p) => self.status = format!("created @{}", p.name),
                Err(e) => self.status = e.to_string(),
            },
            Err(_) => self.status = "empty name — not created".to_string(),
        }
        self.mode = Mode::Normal;
        self.refresh()
    }
}

/// Move `index` by `delta`, clamped to `[0, len-1]`. Returns 0 for empty.
fn clamp_index(index: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let last = (len - 1) as i32;
    (index as i32 + delta).clamp(0, last) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::fake::FakeStore;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            app.on_key(key(KeyCode::Char(c))).unwrap();
        }
    }

    fn add_task(app: &mut App, title: &str) {
        app.focus = Focus::Tasks;
        app.on_key(key(KeyCode::Char('a'))).unwrap();
        type_str(app, title);
        app.on_key(key(KeyCode::Enter)).unwrap();
    }

    #[test]
    fn starts_with_inbox_selected() {
        let mut store = FakeStore::new();
        let app = App::new(&mut store).unwrap();
        assert_eq!(app.current_project().unwrap().name, "Inbox");
    }

    #[test]
    fn add_flow_creates_task_in_current_project() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        add_task(&mut app, "hello");
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "hello");
    }

    #[test]
    fn create_project_flow() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        app.on_key(key(KeyCode::Char('n'))).unwrap();
        type_str(&mut app, "Work");
        app.on_key(key(KeyCode::Enter)).unwrap();
        assert!(app.projects.iter().any(|p| p.name == "Work"));
    }

    #[test]
    fn tab_switches_focus_and_project_nav_filters_tasks() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        // Inbox task.
        add_task(&mut app, "inbox task");
        // New project + a task there.
        app.on_key(key(KeyCode::Char('n'))).unwrap();
        type_str(&mut app, "Work");
        app.on_key(key(KeyCode::Enter)).unwrap();
        // Focus projects, move to Work (index 1), add a task.
        app.on_key(key(KeyCode::Tab)).unwrap();
        assert_eq!(app.focus, Focus::Projects);
        app.on_key(key(KeyCode::Char('j'))).unwrap();
        assert_eq!(app.current_project().unwrap().name, "Work");
        assert!(app.tasks.is_empty(), "Work starts empty");
        app.on_key(key(KeyCode::Tab)).unwrap();
        add_task(&mut app, "work task");
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "work task");
    }

    #[test]
    fn space_toggles_status() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        add_task(&mut app, "task");
        app.on_key(key(KeyCode::Char(' '))).unwrap();
        assert_eq!(app.tasks[0].status, Status::Done);
    }

    #[test]
    fn delete_removes_selected() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        add_task(&mut app, "doomed");
        app.on_key(key(KeyCode::Char('d'))).unwrap();
        assert!(app.tasks.is_empty());
    }

    #[test]
    fn p_cycles_priority() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        add_task(&mut app, "task");
        app.on_key(key(KeyCode::Char('p'))).unwrap();
        assert_eq!(app.tasks[0].priority, Priority::High);
    }

    #[test]
    fn action_keys_ignored_when_projects_focused() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        app.focus = Focus::Projects;
        // 'a' should not enter add-task mode while projects are focused.
        app.on_key(key(KeyCode::Char('a'))).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn q_sets_quit() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        app.on_key(key(KeyCode::Char('q'))).unwrap();
        assert!(app.should_quit);
    }
}
