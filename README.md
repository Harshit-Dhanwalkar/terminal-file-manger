# Terminal File Manager

A simple terminal-based file manager built with Rust, using the `tui` and `crossterm` libraries. It supports navigation through directories, viewing contents, and toggling hidden files. The current working directory can also be saved to a file using a command-line argument.

## Features

- Navigate through files and directories.
- Toggle display of hidden files.
- Display contents of selected directory and file in a separate panel.
- Save the final working directory to a specified file.

## Usage

Run the file manager using Cargo or the binary:

```bash
cargo run -- --cwd-file=<output_file>
```

Example:

```bash
cargo run -- --cwd-file=path.txt
```

### Key Bindings

| Key        | Action                                |
| ---------- | ------------------------------------- |
| `q`        | Quit the file manager                 |
| `↓` or `j` | Move down in the file list            |
| `↑` or `k` | Move up in the file list              |
| `→` or `l` | Enter the selected directory          |
| `←` or `h` | Navigate back to the parent directory |
| `Enter`    | Opens the file                        |
| `.`        | Toggle visibility of hidden files     |
| `crlt-r`   | Redraw terminal UI                    |

## To-Do List

- [x] Implement file preview for text files.
- [x] Use colors for directories and files.
- [ ] Add file handling to operations.
- [ ] Add support for file operations (copy, move, delete).
- [ ] Add search functionality.
- [ ] FZF integration.
- [ ] Improve error handling and logging.
- [ ] File format Icons
- [ ] Add handles the creation of trash directories (specific to the OS) for deleted files.
- [ ] Create necessary directories and configuration files (like .toml files) for storing settings, hotkeys, and logs.
- [ ] checks whether it's the first time the application is being run, creating an initial setup file if necessary.
- [ ] Implement a command-line interface with flags (using clap or structopt in Rust) to allow users to control various aspects of the file manager (e.g., enabling/disabling features, printing configuration paths).
- [ ] Hotkey Management, user-defined keyboard shortcuts, allowing users to add missing hotkeys.
- [ ] Update Check.
- [ ] Customize themes and colors.
- [ ] Image preview
- [ ] Mouse support.
