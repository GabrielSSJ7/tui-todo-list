//! Core domain types for tasks. No I/O, no third-party storage concerns.

use chrono::{DateTime, Utc};
use std::fmt;
use std::str::FromStr;

/// Lifecycle state of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Open,
    Done,
}

impl Status {
    /// SQLite stores status as a short text tag.
    pub fn as_tag(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Done => "done",
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_tag())
    }
}

impl FromStr for Status {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Status::Open),
            "done" => Ok(Status::Done),
            other => Err(format!("invalid status tag {other:?}, expected \"open\" or \"done\"")),
        }
    }
}

/// Priority drives ordering and color in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    pub fn as_tag(self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
        }
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_tag())
    }
}

impl FromStr for Priority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Priority::Low),
            "medium" | "med" => Ok(Priority::Medium),
            "high" => Ok(Priority::High),
            other => Err(format!(
                "invalid priority {other:?}, expected \"low\", \"medium\", or \"high\""
            )),
        }
    }
}

/// A single todo item. `id` is None until persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: Option<i64>,
    pub title: String,
    pub status: Status,
    pub priority: Priority,
    pub created_at: DateTime<Utc>,
}

/// Fields a caller supplies to create a task. Keeps `Task` construction
/// honest: created_at/status/id are owned by the store, not the caller.
#[derive(Debug, Clone)]
pub struct NewTask {
    pub title: String,
    pub priority: Priority,
}

impl NewTask {
    /// Reject blank titles early — the store should never see empty rows.
    pub fn new(title: impl Into<String>, priority: Priority) -> Result<Self, String> {
        let title = title.into().trim().to_string();
        if title.is_empty() {
            return Err("task title is empty, expected non-blank text".to_string());
        }
        Ok(NewTask { title, priority })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrips_through_tag() {
        for s in [Status::Open, Status::Done] {
            assert_eq!(Status::from_str(s.as_tag()), Ok(s));
        }
    }

    #[test]
    fn priority_parses_med_alias() {
        assert_eq!(Priority::from_str("med"), Ok(Priority::Medium));
    }

    #[test]
    fn priority_rejects_garbage_with_value_in_message() {
        let err = Priority::from_str("urgent").unwrap_err();
        assert!(err.contains("urgent"), "message should quote bad value: {err}");
    }

    #[test]
    fn new_task_trims_and_rejects_blank() {
        assert_eq!(NewTask::new("  hi  ", Priority::Low).unwrap().title, "hi");
        assert!(NewTask::new("   ", Priority::Low).is_err());
    }
}
