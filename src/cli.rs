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
        /// Project to add it to (defaults to Inbox).
        #[arg(short = 'P', long)]
        project: Option<String>,
        /// Deadline as YYYY-MM-DD.
        #[arg(short, long)]
        due: Option<String>,
    },
    /// List tasks.
    List {
        /// Include completed tasks too.
        #[arg(short, long)]
        all: bool,
        /// Only show tasks in this project.
        #[arg(short = 'P', long)]
        project: Option<String>,
    },
    /// Show all projects with their tasks grouped beneath each.
    #[command(visible_alias = "tree")]
    Overview {
        /// Include completed tasks too.
        #[arg(short, long)]
        all: bool,
    },
    /// Mark a task done by id.
    Done { id: i64 },
    /// Move a task to another project.
    Move {
        id: i64,
        /// Destination project name.
        project: String,
    },
    /// Create, list, or delete projects.
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Change a task's priority.
    #[command(visible_alias = "pri")]
    Priority {
        id: i64,
        #[arg(value_enum)]
        priority: PriorityArg,
    },
    /// Set a task's deadline (YYYY-MM-DD), or `--clear` to remove it.
    Due {
        id: i64,
        /// Deadline as YYYY-MM-DD (omit with --clear to remove).
        date: Option<String>,
        /// Clear the deadline instead of setting one.
        #[arg(long, conflicts_with = "date")]
        clear: bool,
    },
    /// Reopen a completed task by id.
    Reopen { id: i64 },
    /// Delete a task by id.
    Rm { id: i64 },
    /// Print a one-line summary for the tmux status bar.
    Status,
    /// Print a tmux.conf snippet (popup keybind + status-bar line).
    TmuxConfig,
    /// Print a Hyprland snippet (floating window + SUPER+Shift+T keybind).
    HyprConfig,
    /// Launch the interactive TUI (default when no subcommand given).
    Tui {
        /// Compact view: hide the project sidebar, show only open tasks.
        /// Suited to a small floating window.
        #[arg(short, long)]
        compact: bool,
    },
}

/// Project management subcommands.
#[derive(Subcommand)]
pub enum ProjectAction {
    /// Create a project.
    Add {
        /// Project name (one or more words).
        name: Vec<String>,
    },
    /// List projects with open-task counts.
    #[command(visible_alias = "ls")]
    List,
    /// Delete a project by id; its tasks return to Inbox.
    Rm { id: i64 },
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
