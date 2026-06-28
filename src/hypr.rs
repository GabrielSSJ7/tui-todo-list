//! Hyprland integration: a floating "quick todo" window bound to a key.
//!
//! The window is a terminal launched with a distinct window class so a
//! Hyprland rule can float, size, and center only that window.

/// Window class used by both the spawn command and the float rule.
pub const FLOAT_CLASS: &str = "todo-float";

/// Hyprland config snippet: float rules for the quick-todo window plus a
/// SUPER+Shift+T keybind that spawns it. Printed by `todo hypr-config`.
///
/// `terminal` is the emulator used to host the TUI (e.g. "kitty").
pub fn config_snippet(binary: &str, terminal: &str) -> String {
    let spawn = spawn_command(binary, terminal);
    format!(
        "# todo — Hyprland floating quick-window\n\
         windowrule = float, class:^({FLOAT_CLASS})$\n\
         windowrule = size 40% 50%, class:^({FLOAT_CLASS})$\n\
         windowrule = center, class:^({FLOAT_CLASS})$\n\
         # SUPER + Shift + T → open the floating todo window\n\
         bind = SUPER SHIFT, T, exec, {spawn}\n"
    )
}

/// The command Hyprland runs to open the floating window. kitty/alacritty
/// both accept `--class`; we host the compact TUI inside it.
pub fn spawn_command(binary: &str, terminal: &str) -> String {
    match terminal {
        "alacritty" => format!("alacritty --class {FLOAT_CLASS} -e {binary} tui --compact"),
        // kitty and most others use the same flag shape.
        _ => format!("{terminal} --class {FLOAT_CLASS} -e {binary} tui --compact"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_has_keybind_and_class() {
        let snip = config_snippet("todo", "kitty");
        assert!(snip.contains("SUPER SHIFT, T"));
        assert!(snip.contains(FLOAT_CLASS));
        assert!(snip.contains("todo tui --compact"));
    }

    #[test]
    fn alacritty_uses_class_flag() {
        let cmd = spawn_command("todo", "alacritty");
        assert!(cmd.starts_with("alacritty --class todo-float"));
        assert!(cmd.ends_with("todo tui --compact"));
    }
}
