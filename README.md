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
todo add ship it -P Work           # add into a project (-P/--project)
todo list                          # open tasks only
todo list --all                    # include completed
todo list -P Work                  # only tasks in a project
todo done 1                        # mark done
todo priority 1 high               # change priority (alias: pri)
todo move 1 Work                   # move task to another project
todo reopen 1                      # reopen
todo rm 1                          # delete
todo status                        # one-line summary for tmux
todo                               # launch the TUI (no subcommand)
```

## Projects

Tasks are grouped by project. Unassigned tasks live in the always-present
**Inbox**. Deleting a project moves its tasks back to Inbox.

```sh
todo project add Work              # create a project
todo project ls                    # list projects + open counts
todo project rm 2                  # delete (tasks return to Inbox)
```

## TUI keys

Left pane = projects, right pane = tasks of the selected project.

| key        | action                         |
|------------|--------------------------------|
| `tab`      | switch focus (projects/tasks)  |
| `j`/`k`    | move cursor in focused pane    |
| `n`        | new project                    |
| `a`        | add task (to selected project) |
| `space`/`enter` | toggle done               |
| `p`        | cycle priority                 |
| `d`        | delete selected task           |
| `h`        | hide/show completed            |
| `q`/`esc`  | quit                           |

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
