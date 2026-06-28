//! Entry point: parse args, open the store, dispatch to a handler.

mod cli;
mod commands;
mod error;
mod hypr;
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
    let command = cli.command.unwrap_or(Command::Tui { compact: false });
    dispatch(command, &mut store)
}

fn dispatch(command: Command, store: &mut SqliteStore) -> Result<()> {
    match command {
        Command::Tui { compact } => tui::run(store, compact),
        Command::Status => print_line(tmux::status_line(store)?),
        Command::TmuxConfig => print_line(tmux::config_snippet("todo")),
        Command::HyprConfig => print_line(hypr::config_snippet("todo", "kitty")),
        other => run_text_command(other, store),
    }
}

/// Handlers that produce a single text blob to print.
fn run_text_command(command: Command, store: &mut dyn TaskStore) -> Result<()> {
    let output = match command {
        Command::Add { title, priority, project } => {
            commands::add(store, &title.join(" "), priority.into(), project.as_deref())?
        }
        Command::List { all, project } => commands::list(store, all, project.as_deref())?,
        Command::Done { id } => commands::done(store, id)?,
        Command::Move { id, project } => commands::move_task(store, id, &project)?,
        Command::Project { action } => run_project_action(action, store)?,
        Command::Priority { id, priority } => {
            commands::set_priority(store, id, priority.into())?
        }
        Command::Reopen { id } => commands::reopen(store, id)?,
        Command::Rm { id } => commands::remove(store, id)?,
        // Interactive/printing commands are handled in dispatch.
        Command::Tui { .. } | Command::Status | Command::TmuxConfig | Command::HyprConfig => {
            unreachable!()
        }
    };
    print_line(output)
}

fn run_project_action(action: cli::ProjectAction, store: &mut dyn TaskStore) -> Result<String> {
    use cli::ProjectAction;
    match action {
        ProjectAction::Add { name } => commands::add_project(store, &name.join(" ")),
        ProjectAction::List => commands::list_projects(store),
        ProjectAction::Rm { id } => commands::remove_project(store, id),
    }
}

fn print_line(text: String) -> Result<()> {
    println!("{text}");
    Ok(())
}
