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

    // Project picker takes over the top area while choosing an add target.
    if app.mode == Mode::PickingProject {
        render_project_picker(frame, app, rows[0]);
        render_footer(frame, app, rows[1]);
        return;
    }

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

/// Full-area list to choose which project a new task goes to.
fn render_project_picker(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app.projects.iter().map(project_to_item).collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" add to which project? "),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    if !app.projects.is_empty() {
        state.select(Some(app.pick_selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_tasks(frame: &mut Frame, app: &App, area: Rect) {
    let width = inner_width(area);
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
            task_to_item(t, project, width)
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
    let width = inner_width(area);
    let items: Vec<ListItem> = app
        .overview_rows
        .iter()
        .map(|r| overview_row_to_item(r, width))
        .collect();
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

fn overview_row_to_item(row: &OverviewRow, width: usize) -> ListItem<'_> {
    match row {
        OverviewRow::Header { name, count, .. } => ListItem::new(Line::from(Span::styled(
            format!("@{name} ({count})"),
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        ))),
        OverviewRow::Task(task) => task_to_item(task, None, width),
    }
}

/// Usable text width inside a bordered list: subtract the two borders and a
/// small margin for the selection symbol so wrapped rows never clip.
fn inner_width(area: Rect) -> usize {
    (area.width as usize).saturating_sub(4)
}

/// Render one task row. The title wraps to multiple lines when it exceeds the
/// pane width, with continuation lines indented under the title column.
/// When `project` is Some (compact all-projects view), show a `@project` tag.
fn task_to_item<'a>(task: &'a Task, project: Option<&'a str>, width: usize) -> ListItem<'a> {
    let title_style = match task.status {
        Status::Done => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::CROSSED_OUT),
        Status::Open => Style::default(),
    };
    let prefix = row_prefix_spans(task, project);
    let prefix_width: usize = prefix.iter().map(|s| s.content.chars().count()).sum();
    // Leave room for the prefix; keep a sane minimum so titles always wrap.
    let avail = width.saturating_sub(prefix_width).max(10);
    let chunks = wrap_title(&task.title, avail);

    let indent = " ".repeat(prefix_width);
    let mut lines: Vec<Line> = Vec::with_capacity(chunks.len());
    for (i, chunk) in chunks.iter().enumerate() {
        let mut spans = if i == 0 {
            prefix.clone()
        } else {
            vec![Span::raw(indent.clone())]
        };
        spans.push(Span::styled(chunk.clone(), title_style));
        lines.push(Line::from(spans));
    }
    // Deadline goes on the last line so it stays visible after wrapping.
    if let Some(span) = due_span(task) {
        if let Some(last) = lines.last_mut() {
            last.spans.push(span);
        }
    }
    ListItem::new(lines)
}

/// The fixed leading spans of a task row: checkbox, priority, optional project.
fn row_prefix_spans<'a>(task: &Task, project: Option<&'a str>) -> Vec<Span<'a>> {
    let mark = match task.status {
        Status::Open => "[ ]",
        Status::Done => "[x]",
    };
    let mut spans = vec![
        Span::styled(format!("{mark} "), priority_style(task.priority)),
        Span::styled(format!("{:<6} ", task.priority), priority_style(task.priority)),
    ];
    if let Some(name) = project {
        spans.push(Span::styled(format!("@{name} "), Style::default().fg(Color::Blue)));
    }
    spans
}

/// Greedy word-wrap to `width` columns. Words longer than `width` are hard
/// split so a single long token can't overflow the pane.
fn wrap_title(title: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in title.split_whitespace() {
        if word.chars().count() > width {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            for piece in hard_split(word, width) {
                lines.push(piece);
            }
            continue;
        }
        let extra = if line.is_empty() { 0 } else { 1 };
        if line.chars().count() + extra + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Split an over-long word into `width`-sized chunks (by chars).
fn hard_split(word: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    chars.chunks(width).map(|c| c.iter().collect()).collect()
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
        Mode::PickingProject => {
            (" pick project ", "j/k move · enter select · esc cancel".to_string())
        }
        Mode::Normal => (" status ", app.status.clone()),
    };
    let footer = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(footer, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_keeps_words_whole_within_width() {
        let lines = wrap_title("alpha beta gamma delta", 11);
        assert!(lines.iter().all(|l| l.chars().count() <= 11), "{lines:?}");
        // Nothing is dropped: joining back yields the original words.
        assert_eq!(lines.join(" "), "alpha beta gamma delta");
    }

    #[test]
    fn wrap_hard_splits_overlong_word() {
        let lines = wrap_title("supercalifragilistic", 5);
        assert!(lines.iter().all(|l| l.chars().count() <= 5));
        assert_eq!(lines.concat(), "supercalifragilistic");
    }

    #[test]
    fn wrap_short_title_is_single_line() {
        assert_eq!(wrap_title("hi there", 40), vec!["hi there".to_string()]);
    }

    #[test]
    fn wrap_empty_title_yields_one_empty_line() {
        assert_eq!(wrap_title("", 10), vec![String::new()]);
    }
}
