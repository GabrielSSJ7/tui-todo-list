//! TUI entry point: owns the terminal lifecycle and the event loop.

pub mod app;
pub mod ui;

use crossterm::event::{self, Event};

use crate::error::Result;
use crate::store::TaskStore;
use app::App;

/// Take over the terminal, run the event loop, and always restore on exit.
pub fn run(store: &mut dyn TaskStore) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, store);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, store: &mut dyn TaskStore) -> Result<()> {
    let mut app = App::new(store)?;
    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, &app))?;
        // Only key *press* events drive state; ignore release/repeat noise.
        if let Event::Key(key) = event::read()? {
            if key.kind == event::KeyEventKind::Press {
                app.on_key(key)?;
            }
        }
    }
    Ok(())
}
