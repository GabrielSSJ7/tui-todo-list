//! Pure rendering: reads `App` state, draws widgets. No state mutation.

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::model::{Priority, Status, Task};
use crate::tui::app::{App, Mode};

/// Top-level draw entry. Splits the screen into list + footer.
pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    render_list(frame, app, chunks[0]);
    render_footer(frame, app, chunks[1]);
}

fn render_list(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = app.tasks.iter().map(task_to_item).collect();
    let title = format!(" todo · {} shown ", app.tasks.len());
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    if !app.tasks.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

/// Render one task row with a checkbox and priority color.
fn task_to_item(task: &Task) -> ListItem<'_> {
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
    let line = Line::from(vec![
        Span::styled(format!("{mark} "), priority_style(task.priority)),
        Span::styled(format!("{:<6} ", task.priority), priority_style(task.priority)),
        Span::styled(task.title.clone(), title_style),
    ]);
    ListItem::new(line)
}

fn priority_style(priority: Priority) -> Style {
    let color = match priority {
        Priority::High => Color::Red,
        Priority::Medium => Color::Yellow,
        Priority::Low => Color::Green,
    };
    Style::default().fg(color)
}

/// Footer shows the input box while adding, otherwise the status hint.
fn render_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let (title, body) = match &app.mode {
        Mode::Adding(buf) => (" new task (enter=save esc=cancel) ", format!("{buf}▏")),
        Mode::Normal => (" status ", app.status.clone()),
    };
    let footer = Paragraph::new(body)
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(footer, area);
}
