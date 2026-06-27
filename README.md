# todo

htop-style todo list for the terminal. Rust + ratatui TUI, SQLite storage,
tmux integration (status bar + popup).

## Build

```sh
cargo build --release
# binary at target/release/todo
```

## CLI

```sh
todo add deploy the site -p high   # add (priority: low|medium|high)
todo list                          # open tasks only
todo list --all                    # include completed
todo done 1                        # mark done
todo reopen 1                      # reopen
todo rm 1                          # delete
todo status                        # one-line summary for tmux
todo                               # launch the TUI (no subcommand)
```

## TUI keys

| key        | action               |
|------------|----------------------|
| `a`        | add task             |
| `space`/`enter` | toggle done     |
| `d`        | delete selected      |
| `j`/`k`    | move cursor          |
| `h`        | hide/show completed  |
| `q`/`esc`  | quit                 |

## tmux

```sh
todo tmux-config >> ~/.tmux.conf   # or source tmux/todo.conf
```

Gives `prefix + T` to open the TUI in a popup, and an open-task count in
the status bar.

## Storage

SQLite at the XDG data dir (`~/.local/share/todo/tasks.db` on Linux).

## Layout

```
src/
  main.rs        arg parse + dispatch
  cli.rs         clap definitions
  model.rs       Task / Status / Priority
  error.rs       TodoError
  paths.rs       db location
  commands.rs    non-interactive handlers
  store/         TaskStore trait + sqlite impl + test fake
  tui/           ratatui app (state / render / loop)
  tmux.rs        status line + config snippet
```

## Test

```sh
cargo test
```
