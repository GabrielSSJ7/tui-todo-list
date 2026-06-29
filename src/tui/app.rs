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

/// Ordering of the task list. Default is by priority (the store's order);
/// `Project` groups tasks of the same project together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Priority,
    Project,
}

/// One line in the grouped overview: a project header or a task under it.
#[derive(Debug, Clone)]
pub enum OverviewRow {
    Header { id: i64, name: String, count: usize },
    Task(Task),
}

/// An edit applied to the selected task while the overview is open.
#[derive(Debug, Clone, Copy)]
enum OverviewAct {
    Toggle,
    Priority,
    Delete,
}

/// What the UI is currently doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Browsing.
    Normal,
    /// Choosing which project a new task goes to (compact add flow).
    PickingProject,
    /// Typing a new task title; holds the in-progress buffer.
    AddingTask(String),
    /// Typing a new project name.
    AddingProject(String),
}

/// A reversible step recorded on the undo stack. Each variant is the
/// *inverse* operation needed to undo what the user just did.
#[derive(Debug, Clone)]
enum UndoAction {
    /// Undo a status toggle: set the task back to `prev`.
    Status { id: i64, prev: Status },
    /// Undo a priority change: set the task back to `prev`.
    Priority { id: i64, prev: Priority },
    /// Undo an add: remove the task that was created.
    RemoveTask { id: i64 },
    /// Undo a delete: re-insert the removed task verbatim.
    RestoreTask { task: Task },
    /// Undo a project creation: remove the project that was created.
    RemoveProject { id: i64 },
}

impl UndoAction {
    /// Short label for the status line, e.g. "delete".
    fn label(&self) -> &'static str {
        match self {
            UndoAction::Status { .. } => "toggle",
            UndoAction::Priority { .. } => "priority",
            UndoAction::RemoveTask { .. } => "add",
            UndoAction::RestoreTask { .. } => "delete",
            UndoAction::RemoveProject { .. } => "new project",
        }
    }
}

pub struct App<'a> {
    store: &'a mut dyn TaskStore,
    undo_stack: Vec<UndoAction>,
    pub projects: Vec<Project>,
    pub selected_project: usize,
    pub tasks: Vec<Task>,
    pub selected: usize,
    pub focus: Focus,
    pub mode: Mode,
    pub show_done: bool,
    /// Compact view: no sidebar, open tasks across all projects. For the
    /// small Hyprland floating window.
    pub compact: bool,
    /// Overview: a single grouped list of every project and its tasks.
    pub show_overview: bool,
    pub overview_rows: Vec<OverviewRow>,
    pub overview_selected: usize,
    pub sort_mode: SortMode,
    /// Cursor in the project picker (PickingProject mode).
    pub pick_selected: usize,
    /// Project a queued AddingTask should land in; overrides the default.
    add_target: Option<i64>,
    pub status: String,
    pub should_quit: bool,
}

impl<'a> App<'a> {
    pub fn new(store: &'a mut dyn TaskStore, compact: bool) -> Result<Self> {
        let status = if compact {
            "a add · space done · p pri · u undo · o overview · q quit".to_string()
        } else {
            "tab · a add · n proj · space done · p pri · s sort · d del · u undo · o overview · q quit"
                .to_string()
        };
        let mut app = App {
            store,
            undo_stack: Vec::new(),
            projects: Vec::new(),
            selected_project: 0,
            tasks: Vec::new(),
            selected: 0,
            focus: Focus::Tasks,
            mode: Mode::Normal,
            // Compact glances at open work only; full view shows everything.
            show_done: !compact,
            compact,
            show_overview: false,
            overview_rows: Vec::new(),
            overview_selected: 0,
            sort_mode: SortMode::Priority,
            pick_selected: 0,
            add_target: None,
            status,
            should_quit: false,
        };
        app.refresh()?;
        Ok(app)
    }

    /// The currently highlighted project, if any.
    pub fn current_project(&self) -> Option<&Project> {
        self.projects.get(self.selected_project)
    }

    /// Name of the project a task belongs to, for the compact all-projects view.
    pub fn project_name(&self, project_id: i64) -> Option<&str> {
        self.projects
            .iter()
            .find(|p| p.id == Some(project_id))
            .map(|p| p.name.as_str())
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
        self.reload_tasks()?;
        self.build_overview()
    }

    /// Rebuild the grouped overview: each project header followed by its
    /// tasks (honoring show_done). Kept in sync on every refresh.
    fn build_overview(&mut self) -> Result<()> {
        let status = if self.show_done {
            StatusFilter::All
        } else {
            StatusFilter::Only(Status::Open)
        };
        let mut rows = Vec::new();
        for project in &self.projects {
            let id = project.id.expect("listed project has id");
            let tasks = self
                .store
                .list(TaskQuery::all().with_status(status).in_project(id))?;
            rows.push(OverviewRow::Header {
                id,
                name: project.name.clone(),
                count: tasks.len(),
            });
            rows.extend(tasks.into_iter().map(OverviewRow::Task));
        }
        self.overview_rows = rows;
        if self.overview_selected >= self.overview_rows.len() {
            self.overview_selected = self.overview_rows.len().saturating_sub(1);
        }
        Ok(())
    }

    /// Reload only the task list for the selected project.
    fn reload_tasks(&mut self) -> Result<()> {
        let status = if self.show_done {
            StatusFilter::All
        } else {
            StatusFilter::Only(Status::Open)
        };
        // Compact ignores the selected project and shows every project's work.
        let project = match (self.compact, self.current_project_id()) {
            (true, _) => ProjectFilter::Any,
            (false, Some(id)) => ProjectFilter::Only(id),
            (false, None) => ProjectFilter::Any,
        };
        self.tasks = self.store.list(TaskQuery { status, project })?;
        self.apply_sort();
        if self.selected >= self.tasks.len() {
            self.selected = self.tasks.len().saturating_sub(1);
        }
        Ok(())
    }

    /// Reorder the loaded tasks per the active sort mode. Priority order is
    /// the store default (already applied); Project groups by project rank
    /// while preserving priority order within each group (stable sort).
    fn apply_sort(&mut self) {
        if self.sort_mode != SortMode::Project {
            return;
        }
        // Precompute project ranks so the sort closure doesn't borrow self.
        let rank: std::collections::HashMap<i64, usize> = self
            .projects
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.id.map(|id| (id, i)))
            .collect();
        self.tasks
            .sort_by_key(|t| *rank.get(&t.project_id).unwrap_or(&usize::MAX));
    }

    fn toggle_sort(&mut self) -> Result<()> {
        self.sort_mode = match self.sort_mode {
            SortMode::Priority => SortMode::Project,
            SortMode::Project => SortMode::Priority,
        };
        let label = match self.sort_mode {
            SortMode::Priority => "priority",
            SortMode::Project => "project",
        };
        self.status = format!("sort: {label}");
        self.reload_tasks()
    }

    /// Route a key to the active mode's handler.
    pub fn on_key(&mut self, key: KeyEvent) -> Result<()> {
        match &self.mode {
            Mode::Normal => self.on_normal_key(key),
            Mode::PickingProject => self.on_pick_key(key),
            Mode::AddingTask(_) => self.on_task_input(key),
            Mode::AddingProject(_) => self.on_project_input(key),
        }
    }

    fn on_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        // Overview is a read-only glance — only toggle, quit, and scroll.
        if self.show_overview {
            return self.on_overview_key(key);
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('o') => self.toggle_overview()?,
            KeyCode::Char('s') => self.toggle_sort()?,
            KeyCode::Char('a') => self.begin_add(),
            KeyCode::Tab | KeyCode::BackTab => self.toggle_focus(),
            KeyCode::Char('n') => self.mode = Mode::AddingProject(String::new()),
            KeyCode::Char('u') => self.undo()?,
            KeyCode::Char('h') => self.toggle_show_done()?,
            KeyCode::Char('j') | KeyCode::Down => self.move_down()?,
            KeyCode::Char('k') | KeyCode::Up => self.move_up()?,
            _ => self.on_action_key(key)?,
        }
        Ok(())
    }

    fn on_overview_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('o') | KeyCode::Esc => self.toggle_overview()?,
            KeyCode::Char('j') | KeyCode::Down => {
                self.overview_selected =
                    clamp_index(self.overview_selected, 1, self.overview_rows.len());
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.overview_selected =
                    clamp_index(self.overview_selected, -1, self.overview_rows.len());
            }
            KeyCode::Char(' ') | KeyCode::Enter => self.act_on_overview(OverviewAct::Toggle)?,
            KeyCode::Char('a') => self.begin_overview_add(),
            KeyCode::Char('p') => self.act_on_overview(OverviewAct::Priority)?,
            KeyCode::Char('d') => self.act_on_overview(OverviewAct::Delete)?,
            KeyCode::Char('u') => self.undo_in_overview()?,
            _ => {}
        }
        Ok(())
    }

    /// The task on the currently selected overview row, if it's not a header.
    fn overview_selected_task(&self) -> Option<Task> {
        match self.overview_rows.get(self.overview_selected) {
            Some(OverviewRow::Task(task)) => Some(task.clone()),
            _ => None,
        }
    }

    /// Apply an action to the selected overview task, then rebuild both the
    /// main task list and the overview so the view stays current.
    fn act_on_overview(&mut self, act: OverviewAct) -> Result<()> {
        let Some(task) = self.overview_selected_task() else {
            return Ok(());
        };
        let id = task.id.expect("listed task has an id");
        match act {
            OverviewAct::Toggle => {
                let prev = task.status;
                let next = match prev {
                    Status::Open => Status::Done,
                    Status::Done => Status::Open,
                };
                self.store.set_status(id, next)?;
                self.undo_stack.push(UndoAction::Status { id, prev });
            }
            OverviewAct::Priority => {
                let prev = task.priority;
                let next = match prev {
                    Priority::Low => Priority::Medium,
                    Priority::Medium => Priority::High,
                    Priority::High => Priority::Low,
                };
                self.store.set_priority(id, next)?;
                self.undo_stack.push(UndoAction::Priority { id, prev });
            }
            OverviewAct::Delete => {
                self.store.remove(id)?;
                self.undo_stack.push(UndoAction::RestoreTask { task });
                self.status = format!("removed #{id} (u to undo)");
            }
        }
        self.sync_views()
    }

    fn undo_in_overview(&mut self) -> Result<()> {
        let Some(action) = self.undo_stack.pop() else {
            self.status = "nothing to undo".to_string();
            return Ok(());
        };
        let label = action.label();
        match self.apply_undo(action) {
            Ok(()) => self.status = format!("undid {label}"),
            Err(e) => self.status = format!("undo failed: {e}"),
        }
        self.sync_views()
    }

    /// Keep the hidden main task list and the overview rows consistent after
    /// a mutation made while the overview is open.
    fn sync_views(&mut self) -> Result<()> {
        self.reload_tasks()?;
        self.build_overview()
    }

    fn toggle_overview(&mut self) -> Result<()> {
        self.show_overview = !self.show_overview;
        self.overview_selected = 0;
        // Rebuild from current store state when opening.
        if self.show_overview {
            self.build_overview()?;
        }
        Ok(())
    }

    /// Keys that act on tasks — only meaningful when the Tasks pane is focused.
    fn on_action_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.focus != Focus::Tasks {
            return Ok(());
        }
        match key.code {
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle_selected()?,
            KeyCode::Char('d') => self.delete_selected()?,
            KeyCode::Char('p') => self.cycle_priority()?,
            _ => {}
        }
        Ok(())
    }

    /// Start adding a task from the normal view. Compact mode has no sidebar,
    /// so it first asks which project; the full view uses the selected project.
    fn begin_add(&mut self) {
        if self.compact {
            self.pick_selected = 0;
            self.mode = Mode::PickingProject;
        } else if self.focus == Focus::Tasks {
            self.add_target = self.current_project_id();
            self.mode = Mode::AddingTask(String::new());
        }
    }

    /// Start adding a task from the overview: it lands in the project the
    /// cursor is currently inside (the selected task's, or the header's).
    fn begin_overview_add(&mut self) {
        let Some(pid) = self.overview_cursor_project() else {
            return;
        };
        self.add_target = Some(pid);
        self.mode = Mode::AddingTask(String::new());
    }

    /// Project the overview cursor sits in: a task row's project, or a header.
    fn overview_cursor_project(&self) -> Option<i64> {
        match self.overview_rows.get(self.overview_selected)? {
            OverviewRow::Task(task) => Some(task.project_id),
            OverviewRow::Header { id, .. } => Some(*id),
        }
    }

    fn on_pick_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Char('j') | KeyCode::Down => {
                self.pick_selected = clamp_index(self.pick_selected, 1, self.projects.len());
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.pick_selected = clamp_index(self.pick_selected, -1, self.projects.len());
            }
            KeyCode::Enter => {
                self.add_target = self.projects.get(self.pick_selected).and_then(|p| p.id);
                self.mode = Mode::AddingTask(String::new());
            }
            _ => {}
        }
        Ok(())
    }

    fn toggle_focus(&mut self) {
        // No sidebar in compact mode — focus stays on tasks.
        if self.compact {
            return;
        }
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

    fn toggle_selected(&mut self) -> Result<()> {
        let Some(task) = self.tasks.get(self.selected) else {
            return Ok(());
        };
        let prev = task.status;
        let next = match prev {
            Status::Open => Status::Done,
            Status::Done => Status::Open,
        };
        let id = task.id.expect("listed task has an id");
        self.store.set_status(id, next)?;
        self.undo_stack.push(UndoAction::Status { id, prev });
        self.reload_tasks()
    }

    fn delete_selected(&mut self) -> Result<()> {
        let Some(task) = self.tasks.get(self.selected).cloned() else {
            return Ok(());
        };
        let id = task.id.expect("listed task has an id");
        self.store.remove(id)?;
        self.undo_stack.push(UndoAction::RestoreTask { task });
        self.status = format!("removed #{id} (u to undo)");
        self.reload_tasks()
    }

    /// Bump the selected task's priority one step, wrapping High → Low.
    fn cycle_priority(&mut self) -> Result<()> {
        let Some(task) = self.tasks.get(self.selected) else {
            return Ok(());
        };
        let prev = task.priority;
        let next = match prev {
            Priority::Low => Priority::Medium,
            Priority::Medium => Priority::High,
            Priority::High => Priority::Low,
        };
        let id = task.id.expect("listed task has an id");
        self.store.set_priority(id, next)?;
        self.undo_stack.push(UndoAction::Priority { id, prev });
        self.reload_tasks()
    }

    fn toggle_show_done(&mut self) -> Result<()> {
        self.show_done = !self.show_done;
        self.reload_tasks()
    }

    /// Pop and apply the most recent inverse action. A failed undo (e.g. the
    /// target was deleted by another action) is reported, not fatal.
    fn undo(&mut self) -> Result<()> {
        let Some(action) = self.undo_stack.pop() else {
            self.status = "nothing to undo".to_string();
            return Ok(());
        };
        let label = action.label();
        match self.apply_undo(action) {
            Ok(()) => self.status = format!("undid {label}"),
            Err(e) => self.status = format!("undo failed: {e}"),
        }
        self.refresh()
    }

    fn apply_undo(&mut self, action: UndoAction) -> Result<()> {
        match action {
            UndoAction::Status { id, prev } => {
                self.store.set_status(id, prev)?;
            }
            UndoAction::Priority { id, prev } => {
                self.store.set_priority(id, prev)?;
            }
            UndoAction::RemoveTask { id } => self.store.remove(id)?,
            UndoAction::RestoreTask { task } => self.store.restore_task(&task)?,
            UndoAction::RemoveProject { id } => self.store.remove_project(id)?,
        }
        Ok(())
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
        // Target is whatever the add flow chose (picker/overview), else the
        // selected project.
        let Some(project_id) = self.add_target.or_else(|| self.current_project_id()) else {
            self.status = "no project selected".to_string();
            self.mode = Mode::Normal;
            self.add_target = None;
            return Ok(());
        };
        // Blank input just cancels — no error popup needed mid-flow.
        match NewTask::new(buf.clone(), Priority::Medium, project_id) {
            Ok(new) => {
                let task = self.store.add(new)?;
                let id = task.id.unwrap_or_default();
                self.undo_stack.push(UndoAction::RemoveTask { id });
                let where_to = self.project_name(project_id).unwrap_or("?");
                self.status = format!("added #{id} to @{where_to}");
            }
            Err(_) => self.status = "empty title — not added".to_string(),
        }
        self.mode = Mode::Normal;
        self.add_target = None;
        self.sync_views()
    }

    fn commit_project(&mut self) -> Result<()> {
        let Mode::AddingProject(buf) = &self.mode else {
            return Ok(());
        };
        match clean_project_name(buf.clone()) {
            Ok(name) => match self.store.add_project(&name) {
                Ok(p) => {
                    if let Some(id) = p.id {
                        self.undo_stack.push(UndoAction::RemoveProject { id });
                    }
                    self.status = format!("created @{}", p.name);
                }
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
        let app = App::new(&mut store, false).unwrap();
        assert_eq!(app.current_project().unwrap().name, "Inbox");
    }

    #[test]
    fn add_flow_creates_task_in_current_project() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "hello");
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "hello");
    }

    #[test]
    fn create_project_flow() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        app.on_key(key(KeyCode::Char('n'))).unwrap();
        type_str(&mut app, "Work");
        app.on_key(key(KeyCode::Enter)).unwrap();
        assert!(app.projects.iter().any(|p| p.name == "Work"));
    }

    #[test]
    fn tab_switches_focus_and_project_nav_filters_tasks() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
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
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "task");
        app.on_key(key(KeyCode::Char(' '))).unwrap();
        assert_eq!(app.tasks[0].status, Status::Done);
    }

    #[test]
    fn delete_removes_selected() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "doomed");
        app.on_key(key(KeyCode::Char('d'))).unwrap();
        assert!(app.tasks.is_empty());
    }

    #[test]
    fn p_cycles_priority() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "task");
        app.on_key(key(KeyCode::Char('p'))).unwrap();
        assert_eq!(app.tasks[0].priority, Priority::High);
    }

    #[test]
    fn action_keys_ignored_when_projects_focused() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        app.focus = Focus::Projects;
        // 'a' should not enter add-task mode while projects are focused.
        app.on_key(key(KeyCode::Char('a'))).unwrap();
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn q_sets_quit() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        app.on_key(key(KeyCode::Char('q'))).unwrap();
        assert!(app.should_quit);
    }

    #[test]
    fn undo_delete_restores_task() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "precious");
        app.on_key(key(KeyCode::Char('d'))).unwrap();
        assert!(app.tasks.is_empty());
        app.on_key(key(KeyCode::Char('u'))).unwrap();
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].title, "precious");
    }

    #[test]
    fn undo_add_removes_task() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "oops");
        app.on_key(key(KeyCode::Char('u'))).unwrap();
        assert!(app.tasks.is_empty());
    }

    #[test]
    fn undo_toggle_reverts_status() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "task");
        app.on_key(key(KeyCode::Char(' '))).unwrap();
        assert_eq!(app.tasks[0].status, Status::Done);
        app.on_key(key(KeyCode::Char('u'))).unwrap();
        assert_eq!(app.tasks[0].status, Status::Open);
    }

    #[test]
    fn undo_priority_reverts() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "task"); // Medium
        app.on_key(key(KeyCode::Char('p'))).unwrap();
        assert_eq!(app.tasks[0].priority, Priority::High);
        app.on_key(key(KeyCode::Char('u'))).unwrap();
        assert_eq!(app.tasks[0].priority, Priority::Medium);
    }

    #[test]
    fn undo_is_lifo_across_actions() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "keep");
        app.on_key(key(KeyCode::Char('p'))).unwrap(); // High
        app.on_key(key(KeyCode::Char('u'))).unwrap(); // undo priority -> Medium
        assert_eq!(app.tasks[0].priority, Priority::Medium);
        assert_eq!(app.tasks.len(), 1);
        app.on_key(key(KeyCode::Char('u'))).unwrap(); // undo add -> gone
        assert!(app.tasks.is_empty());
    }

    #[test]
    fn undo_empty_stack_is_noop() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        app.on_key(key(KeyCode::Char('u'))).unwrap();
        assert_eq!(app.status, "nothing to undo");
    }

    #[test]
    fn s_sorts_tasks_by_project() {
        let mut store = FakeStore::new();
        let work = store.add_project("Work").unwrap();
        // Two Inbox tasks and one Work task, interleaved priorities.
        store.add(NewTask::new("inbox a", Priority::High, 1).unwrap()).unwrap();
        store
            .add(NewTask::new("work a", Priority::High, work.id.unwrap()).unwrap())
            .unwrap();
        store.add(NewTask::new("inbox b", Priority::High, 1).unwrap()).unwrap();
        // Compact spans all projects so sort is observable.
        let mut app = App::new(&mut store, true).unwrap();
        app.on_key(key(KeyCode::Char('s'))).unwrap(); // -> sort by project
        assert_eq!(app.sort_mode, SortMode::Project);
        // Inbox (rank 0) tasks come before Work (rank 1).
        let projects: Vec<i64> = app.tasks.iter().map(|t| t.project_id).collect();
        let first_work = projects.iter().position(|&p| p == work.id.unwrap()).unwrap();
        assert!(projects[..first_work].iter().all(|&p| p == 1));
    }

    #[test]
    fn compact_add_picks_project_then_creates_task_there() {
        let mut store = FakeStore::new();
        let work = store.add_project("Work").unwrap();
        let mut app = App::new(&mut store, true).unwrap();
        app.on_key(key(KeyCode::Char('a'))).unwrap();
        assert_eq!(app.mode, Mode::PickingProject);
        app.on_key(key(KeyCode::Char('j'))).unwrap(); // Inbox -> Work
        app.on_key(key(KeyCode::Enter)).unwrap(); // choose Work
        assert!(matches!(app.mode, Mode::AddingTask(_)));
        type_str(&mut app, "compact task");
        app.on_key(key(KeyCode::Enter)).unwrap();
        let added = app.tasks.iter().find(|t| t.title == "compact task").unwrap();
        assert_eq!(added.project_id, work.id.unwrap());
    }

    #[test]
    fn overview_add_uses_project_under_cursor() {
        let mut store = FakeStore::new();
        let work = store.add_project("Work").unwrap();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "inbox task");
        app.on_key(key(KeyCode::Char('o'))).unwrap();
        // Rows: [Header Inbox, Task, Header Work]. Land on the Work header.
        while app.overview_cursor_project() != work.id {
            app.on_key(key(KeyCode::Char('j'))).unwrap();
        }
        app.on_key(key(KeyCode::Char('a'))).unwrap();
        type_str(&mut app, "work task");
        app.on_key(key(KeyCode::Enter)).unwrap();
        let in_work = app.overview_rows.iter().any(|r| {
            matches!(r, OverviewRow::Task(t) if t.title == "work task" && t.project_id == work.id.unwrap())
        });
        assert!(in_work, "new task should be in Work");
    }

    #[test]
    fn o_toggles_overview_and_builds_grouped_rows() {
        let mut store = FakeStore::new();
        store.add_project("Work").unwrap();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "inbox task"); // goes to Inbox (selected)
        app.on_key(key(KeyCode::Char('o'))).unwrap();
        assert!(app.show_overview);
        // Expect a header for each project plus the one task row.
        let headers = app
            .overview_rows
            .iter()
            .filter(|r| matches!(r, OverviewRow::Header { .. }))
            .count();
        assert_eq!(headers, 2, "Inbox + Work headers");
        let tasks = app
            .overview_rows
            .iter()
            .filter(|r| matches!(r, OverviewRow::Task(_)))
            .count();
        assert_eq!(tasks, 1);
        // Toggle off.
        app.on_key(key(KeyCode::Char('o'))).unwrap();
        assert!(!app.show_overview);
    }

    #[test]
    fn overview_space_toggles_selected_task() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "finish me");
        app.on_key(key(KeyCode::Char('o'))).unwrap();
        // Row 0 is the Inbox header; move down to the task row.
        app.on_key(key(KeyCode::Char('j'))).unwrap();
        app.on_key(key(KeyCode::Char(' '))).unwrap();
        let done = app.overview_rows.iter().any(|r| {
            matches!(r, OverviewRow::Task(t) if t.status == Status::Done)
        });
        assert!(done, "selected task should be marked done");
    }

    #[test]
    fn overview_delete_then_undo_restores() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "doomed");
        app.on_key(key(KeyCode::Char('o'))).unwrap();
        app.on_key(key(KeyCode::Char('j'))).unwrap(); // onto the task
        app.on_key(key(KeyCode::Char('d'))).unwrap();
        assert!(!app.overview_rows.iter().any(|r| matches!(r, OverviewRow::Task(_))));
        app.on_key(key(KeyCode::Char('u'))).unwrap();
        assert!(app.overview_rows.iter().any(|r| matches!(r, OverviewRow::Task(_))));
    }

    #[test]
    fn overview_action_on_header_is_noop() {
        let mut store = FakeStore::new();
        let mut app = App::new(&mut store, false).unwrap();
        add_task(&mut app, "keep");
        app.on_key(key(KeyCode::Char('o'))).unwrap();
        // overview_selected = 0 → Inbox header; delete must do nothing.
        app.on_key(key(KeyCode::Char('d'))).unwrap();
        assert!(app.overview_rows.iter().any(|r| matches!(r, OverviewRow::Task(_))));
    }

    #[test]
    fn compact_shows_open_tasks_across_projects_and_locks_focus() {
        let mut store = FakeStore::new();
        let work = store.add_project("Work").unwrap();
        store
            .add(NewTask::new("inbox open", Priority::Low, 1).unwrap())
            .unwrap();
        store
            .add(NewTask::new("work open", Priority::High, work.id.unwrap()).unwrap())
            .unwrap();
        let mut app = App::new(&mut store, true).unwrap();
        assert_eq!(app.tasks.len(), 2, "both projects' open tasks shown");
        assert!(!app.show_done);
        // Tab is a no-op in compact — focus stays on tasks.
        app.on_key(key(KeyCode::Tab)).unwrap();
        assert_eq!(app.focus, Focus::Tasks);
    }
}
