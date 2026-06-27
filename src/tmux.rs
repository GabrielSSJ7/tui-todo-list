//! tmux integration: a compact status-line string and the popup config snippet.

use crate::error::Result;
use crate::model::Status;
use crate::store::{StatusFilter, TaskStore};

/// One-line summary for the tmux status bar, e.g. `☐ 3  ⚑ deploy site`.
/// Shows open count and the single highest-priority open task's title.
pub fn status_line(store: &dyn TaskStore) -> Result<String> {
    let open = store.open_count()?;
    if open == 0 {
        return Ok("☐ 0".to_string());
    }
    let next = top_open_title(store)?;
    match next {
        Some(title) => Ok(format!("☐ {open}  ⚑ {}", truncate(&title, 24))),
        None => Ok(format!("☐ {open}")),
    }
}

/// Title of the highest-priority, then newest, open task.
fn top_open_title(store: &dyn TaskStore) -> Result<Option<String>> {
    let open = store.list(StatusFilter::Only(Status::Open))?;
    // list() already orders high-priority first; take the head.
    Ok(open.into_iter().next().map(|t| t.title))
}

/// Clip overly long titles so the status bar never wraps. Adds an ellipsis.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let kept: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// tmux.conf snippet wiring a popup launcher and a status-bar refresh.
/// Printed by `todo tmux-config` for the user to paste into ~/.tmux.conf.
pub fn config_snippet(binary: &str) -> String {
    format!(
        "# todo — tmux integration\n\
         # Popup TUI on prefix + T\n\
         bind T display-popup -E -w 80% -h 80% '{binary} tui'\n\
         # Open-task summary in the status bar (refreshes every 30s)\n\
         set -g status-right '#({binary} status) | %H:%M'\n\
         set -g status-interval 30\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NewTask, Priority};
    use crate::store::fake::FakeStore;

    #[test]
    fn empty_store_shows_zero() {
        let s = FakeStore::new();
        assert_eq!(status_line(&s).unwrap(), "☐ 0");
    }

    #[test]
    fn shows_count_and_top_title() {
        let mut s = FakeStore::new();
        s.add(NewTask::new("ship it", Priority::High).unwrap()).unwrap();
        s.add(NewTask::new("later", Priority::Low).unwrap()).unwrap();
        let line = status_line(&s).unwrap();
        assert!(line.starts_with("☐ 2"), "line was {line}");
        assert!(line.contains("ship it"));
    }

    #[test]
    fn truncates_long_titles() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
    }

    #[test]
    fn snippet_embeds_binary_name() {
        let snip = config_snippet("todo");
        assert!(snip.contains("todo tui"));
        assert!(snip.contains("todo status"));
    }
}
