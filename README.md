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
| Key            | Action                                     |
|----------------|--------------------------------------------|
| `q`            | Quit the file manager                      |
| `↓` or `j`     | Move down in the file list                 |
| `↑` or `k`     | Move up in the file list                   |
| `→` or `l`     | Enter the selected directory               |
| `←` or `h`     | Navigate back to the parent directory      |
| `.`            | Toggle visibility of hidden files          |

## To-Do List
- [x] Implement file preview for text files.
- [ ] Use colors for directories and files.
- [ ] Add support for file operations (copy, move, delete).
- [ ] Add search functionality.
- [ ] Improve error handling and logging.
- [ ] Customize themes and colors.
