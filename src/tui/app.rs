//! TUI state machine. Pure logic: key in, state/store mutation out — no
//! terminal or rendering here, so it's unit-testable with a FakeStore.

use crossterm::event::{KeyCode, KeyEvent};

use crate::error::Result;
use crate::model::{NewTask, Priority, Status, Task};
use crate::store::{StatusFilter, TaskStore};

/// What the UI is currently doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Browsing the list.
    Normal,
    /// Typing a new task title; holds the in-progress buffer.
    Adding(String),
}

pub struct App<'a> {
    store: &'a mut dyn TaskStore,
    pub tasks: Vec<Task>,
    pub selected: usize,
    pub mode: Mode,
    pub show_done: bool,
    pub status: String,
    pub should_quit: bool,
}

impl<'a> App<'a> {
    pub fn new(store: &'a mut dyn TaskStore) -> Result<Self> {
        let mut app = App {
            store,
            tasks: Vec::new(),
            selected: 0,
            mode: Mode::Normal,
            show_done: true,
            status: "a add · space toggle · d del · q quit".to_string(),
            should_quit: false,
        };
        app.refresh()?;
        Ok(app)
    }

    /// Reload tasks from the store and clamp the cursor.
    pub fn refresh(&mut self) -> Result<()> {
        let filter = if self.show_done {
            StatusFilter::All
        } else {
            StatusFilter::Only(Status::Open)
        };
        self.tasks = self.store.list(filter)?;
        if self.selected >= self.tasks.len() {
            self.selected = self.tasks.len().saturating_sub(1);
        }
        Ok(())
    }

    /// Route a key to the active mode's handler.
    pub fn on_key(&mut self, key: KeyEvent) -> Result<()> {
        match &self.mode {
            Mode::Normal => self.on_normal_key(key),
            Mode::Adding(_) => self.on_adding_key(key),
        }
    }

    fn on_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_cursor(-1),
            KeyCode::Char('a') => self.mode = Mode::Adding(String::new()),
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_selected()?,
            KeyCode::Char('d') => self.delete_selected()?,
            KeyCode::Char('h') => self.toggle_show_done()?,
            _ => {}
        }
        Ok(())
    }

    fn on_adding_key(&mut self, key: KeyEvent) -> Result<()> {
        let Mode::Adding(buf) = &mut self.mode else {
            return Ok(());
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => self.commit_new()?,
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) => buf.push(c),
            _ => {}
        }
        Ok(())
    }

    fn move_cursor(&mut self, delta: i32) {
        if self.tasks.is_empty() {
            return;
        }
        let last = self.tasks.len() - 1;
        let next = self.selected as i32 + delta;
        self.selected = next.clamp(0, last as i32) as usize;
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
        self.refresh()
    }

    fn delete_selected(&mut self) -> Result<()> {
        let Some(id) = self.selected_id() else {
            return Ok(());
        };
        self.store.remove(id)?;
        self.status = format!("removed #{id}");
        self.refresh()
    }

    fn toggle_show_done(&mut self) -> Result<()> {
        self.show_done = !self.show_done;
        self.refresh()
    }

    fn commit_new(&mut self) -> Result<()> {
        let Mode::Adding(buf) = &self.mode else {
            return Ok(());
        };
        // Blank input just cancels — no error popup needed mid-flow.
        match NewTask::new(buf.clone(), Priority::Medium) {
            Ok(new) => {
                let task = self.store.add(new)?;
                self.status = format!("added #{}", task.id.unwrap_or_default());
            }
            Err(_) => self.status = "empty title — not added".to_string(),
        }
        self.mode = Mode::Normal;
        self.refresh()
    }
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

    #[test]
    fn add_flow_creates_task() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        app.on_key(key(KeyCode::Char('a'))).unwrap();
        type_str(&mut app, "hello");
        app.on_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "hello");
    }

    #[test]
    fn esc_cancels_add() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        app.on_key(key(KeyCode::Char('a'))).unwrap();
        type_str(&mut app, "draft");
        app.on_key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.tasks.is_empty());
    }

    #[test]
    fn space_toggles_status() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        app.on_key(key(KeyCode::Char('a'))).unwrap();
        type_str(&mut app, "task");
        app.on_key(key(KeyCode::Enter)).unwrap();
        app.on_key(key(KeyCode::Char(' '))).unwrap();
        assert_eq!(app.tasks[0].status, Status::Done);
        app.on_key(key(KeyCode::Char(' '))).unwrap();
        assert_eq!(app.tasks[0].status, Status::Open);
    }

    #[test]
    fn delete_removes_selected() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        app.on_key(key(KeyCode::Char('a'))).unwrap();
        type_str(&mut app, "doomed");
        app.on_key(key(KeyCode::Enter)).unwrap();
        app.on_key(key(KeyCode::Char('d'))).unwrap();
        assert!(app.tasks.is_empty());
    }

    #[test]
    fn cursor_clamps_at_bounds() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        for t in ["a", "b"] {
            app.on_key(key(KeyCode::Char('a'))).unwrap();
            type_str(&mut app, t);
            app.on_key(key(KeyCode::Enter)).unwrap();
        }
        app.on_key(key(KeyCode::Char('k'))).unwrap(); // up past top
        assert_eq!(app.selected, 0);
        for _ in 0..5 {
            app.on_key(key(KeyCode::Char('j'))).unwrap(); // down past bottom
        }
        assert_eq!(app.selected, app.tasks.len() - 1);
    }

    #[test]
    fn q_sets_quit() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store).unwrap();
        app.on_key(key(KeyCode::Char('q'))).unwrap();
        assert!(app.should_quit);
    }
}
