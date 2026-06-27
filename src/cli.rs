//! Command-line surface. Pure declaration — dispatch lives in commands.rs.

use clap::{Parser, Subcommand, ValueEnum};

use crate::model::Priority;

#[derive(Parser)]
#[command(name = "todo", version, about = "htop-style todo list with a TUI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Add a new task.
    Add {
        /// Task title (one or more words).
        title: Vec<String>,
        /// Priority: low | medium | high.
        #[arg(short, long, value_enum, default_value_t = PriorityArg::Medium)]
        priority: PriorityArg,
    },
    /// List tasks.
    List {
        /// Include completed tasks too.
        #[arg(short, long)]
        all: bool,
    },
    /// Mark a task done by id.
    Done { id: i64 },
    /// Reopen a completed task by id.
    Reopen { id: i64 },
    /// Delete a task by id.
    Rm { id: i64 },
    /// Print a one-line summary for the tmux status bar.
    Status,
    /// Print a tmux.conf snippet (popup keybind + status-bar line).
    TmuxConfig,
    /// Launch the interactive TUI (default when no subcommand given).
    Tui,
}

/// Clap-facing mirror of `Priority`. Keeps clap derive out of the domain type.
#[derive(Copy, Clone, ValueEnum)]
pub enum PriorityArg {
    Low,
    Medium,
    High,
}

impl From<PriorityArg> for Priority {
    fn from(p: PriorityArg) -> Self {
        match p {
            PriorityArg::Low => Priority::Low,
            PriorityArg::Medium => Priority::Medium,
            PriorityArg::High => Priority::High,
        }
    }
}
