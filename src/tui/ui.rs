//! Pure rendering: reads `App` state, draws widgets. No state mutation.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use chrono::Local;

use crate::model::{Priority, Project, Status, Task};
use crate::tui::app::{App, Focus, Mode, OverviewRow};

/// Top-level draw entry: sidebar + task list on top, footer below.
pub fn render(frame: &mut Frame, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    // Overview replaces the panes with one grouped, full-width list.
    if app.show_overview {
        render_overview(frame, app, rows[0]);
        render_footer(frame, app, rows[1]);
        return;
    }

    // Compact mode drops the sidebar so the task list fills the small window.
    if app.compact {
        render_tasks(frame, app, rows[0]);
        render_footer(frame, app, rows[1]);
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(28), Constraint::Min(20)])
        .split(rows[0]);

    render_projects(frame, app, cols[0]);
    render_tasks(frame, app, cols[1]);
    render_footer(frame, app, rows[1]);
}

/// Border highlights the focused pane, like htop's active column.
fn pane_border(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_projects(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.projects.iter().map(project_to_item).collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(pane_border(app.focus == Focus::Projects))
                .title(" projects "),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    if !app.projects.is_empty() {
        state.select(Some(app.selected_project));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn project_to_item(project: &Project) -> ListItem<'_> {
    ListItem::new(Line::from(project.name.clone()))
}

fn render_tasks(frame: &mut Frame, app: &App, area: Rect) {
    // Compact spans every project, so each row shows its @project tag.
    let items: Vec<ListItem> = app
        .tasks
        .iter()
        .map(|t| {
            let project = if app.compact {
                app.project_name(t.project_id)
            } else {
                None
            };
            task_to_item(t, project)
        })
        .collect();
    let scope = if app.compact {
        "to do"
    } else {
        app.current_project().map(|p| p.name.as_str()).unwrap_or("all")
    };
    let title = format!(" {scope} · {} ", app.tasks.len());
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(pane_border(app.focus == Focus::Tasks))
                .title(title),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    if !app.tasks.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

/// Render the grouped overview: project headers with their tasks beneath.
fn render_overview(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.overview_rows.iter().map(overview_row_to_item).collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" overview · all projects "),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default();
    if !app.overview_rows.is_empty() {
        state.select(Some(app.overview_selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn overview_row_to_item(row: &OverviewRow) -> ListItem<'_> {
    match row {
        OverviewRow::Header { name, count } => ListItem::new(Line::from(Span::styled(
            format!("@{name} ({count})"),
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        ))),
        OverviewRow::Task(task) => task_to_item(task, None),
    }
}

/// Render one task row with a checkbox and priority color. When `project`
/// is Some (compact all-projects view), prepend a dimmed `@project` tag.
fn task_to_item<'a>(task: &'a Task, project: Option<&'a str>) -> ListItem<'a> {
    let mark = match task.status {
        Status::Open => "[ ]",
        Status::Done => "[x]",
    };
    let title_style = match task.status {
        Status::Done => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::CROSSED_OUT),
        Status::Open => Style::default(),
    };
    let mut spans = vec![
        Span::styled(format!("{mark} "), priority_style(task.priority)),
        Span::styled(format!("{:<6} ", task.priority), priority_style(task.priority)),
    ];
    if let Some(name) = project {
        spans.push(Span::styled(
            format!("@{name} "),
            Style::default().fg(Color::Blue),
        ));
    }
    spans.push(Span::styled(task.title.clone(), title_style));
    if let Some(span) = due_span(task) {
        spans.push(span);
    }
    ListItem::new(Line::from(spans))
}

/// A `due M-D` span, red when overdue and still open. None if no deadline.
fn due_span(task: &Task) -> Option<Span<'static>> {
    let due = task.due?;
    let overdue = task.status == Status::Open && due < Local::now().date_naive();
    let color = if overdue { Color::Red } else { Color::DarkGray };
    Some(Span::styled(
        format!("  due {}", due.format("%m-%d")),
        Style::default().fg(color),
    ))
}

fn priority_style(priority: Priority) -> Style {
    let color = match priority {
        Priority::High => Color::Red,
        Priority::Medium => Color::Yellow,
        Priority::Low => Color::Green,
    };
    Style::default().fg(color)
}

/// Footer shows the active input box, otherwise the status hint.
fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let (title, body) = match &app.mode {
        Mode::AddingTask(buf) => (" new task (enter=save esc=cancel) ", format!("{buf}▏")),
        Mode::AddingProject(buf) => {
            (" new project (enter=save esc=cancel) ", format!("{buf}▏"))
        }
        Mode::Normal => (" status ", app.status.clone()),
    };
    let footer = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(footer, area);
}
