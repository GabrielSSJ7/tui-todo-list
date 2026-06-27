//! Entry point: parse args, open the store, dispatch to a handler.

mod cli;
mod commands;
mod error;
mod model;
mod paths;
mod store;
mod tmux;
mod tui;

use clap::Parser;
use std::process::ExitCode;

use cli::{Cli, Command};
use error::Result;
use store::sqlite::SqliteStore;
use store::TaskStore;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("todo: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let mut store = SqliteStore::open(&paths::database_path()?)?;
    // No subcommand → launch the TUI, matching htop's bare-invocation feel.
    let command = cli.command.unwrap_or(Command::Tui);
    dispatch(command, &mut store)
}

fn dispatch(command: Command, store: &mut SqliteStore) -> Result<()> {
    match command {
        Command::Tui => tui::run(store),
        Command::Status => print_line(tmux::status_line(store)?),
        Command::TmuxConfig => print_line(tmux::config_snippet("todo")),
        other => run_text_command(other, store),
    }
}

/// Handlers that produce a single text blob to print.
fn run_text_command(command: Command, store: &mut dyn TaskStore) -> Result<()> {
    let output = match command {
        Command::Add { title, priority } => {
            commands::add(store, &title.join(" "), priority.into())?
        }
        Command::List { all } => commands::list(store, all)?,
        Command::Done { id } => commands::done(store, id)?,
        Command::Reopen { id } => commands::reopen(store, id)?,
        Command::Rm { id } => commands::remove(store, id)?,
        // TUI/Status/TmuxConfig are handled in dispatch and never reach here.
        Command::Tui | Command::Status | Command::TmuxConfig => unreachable!(),
    };
    print_line(output)
}

fn print_line(text: String) -> Result<()> {
    println!("{text}");
    Ok(())
}
