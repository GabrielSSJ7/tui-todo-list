//! Resolves the on-disk database location via XDG data dir.

use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

use crate::error::{Result, TodoError};

/// Path to the SQLite file, creating the parent data dir if needed.
/// e.g. ~/.local/share/todo/tasks.db on Linux.
pub fn database_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "todo").ok_or(TodoError::NoDataDir)?;
    let data_dir = dirs.data_dir();
    fs::create_dir_all(data_dir)?;
    Ok(data_dir.join("tasks.db"))
}
