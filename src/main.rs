use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
// use std::io::{self, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use toml::Value;
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem},
    Terminal,
};
// use chrono::Local; // For date and time

fn main() -> Result<(), io::Error> {
    // Parse arguments
    let project_dir = env::current_dir().unwrap();
    let path_file = project_dir.join("src").join("path.txt");
    if !path_file.exists() {
        eprintln!("Error: path.txt not found in {}", path_file.display());
        return Ok(());
    }
    let opener_config_path = project_dir.join("src").join("opener.toml");
    if !opener_config_path.exists() {
        eprintln!(
            "Error: opener.toml not found in {}",
            opener_config_path.display()
        );
        return Ok(());
    }
    //
    // Load opener configuration
    let opener_config = load_opener_config(&opener_config_path)?;
    println!("Loaded opener.toml configuration");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut cwd_file: Option<PathBuf> = None;
    for arg in env::args().skip(1) {
        if arg.starts_with("--cwd-file=") {
            cwd_file = Some(PathBuf::from(arg.trim_start_matches("--cwd-file=")));
        }
    }

    // File Manager State
    // let mut current_dir = std::env::current_dir()?;
    let mut current_dir = match cwd_file {
        Some(ref path) if path.exists() => {
            match fs::read_to_string(path) {
                Ok(content) => {
                    let dir = PathBuf::from(content.trim());
                    if dir.is_dir() {
                        dir
                    } else {
                        eprintln!("Path in cwd file is not a directory. Falling back to current directory.");
                        std::env::current_dir()?
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Failed to read cwd file: {}. Falling back to current directory.",
                        e
                    );
                    std::env::current_dir()?
                }
            }
        }
        _ => std::env::current_dir()?,
    };
    let mut show_hidden = true;
    let mut files = list_files(&current_dir, show_hidden)?;
    let mut cursor_position: usize = 0;

    // Get current date and time
    // let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    loop {
        // Get the selected file or directory
        let selected_file = files.get(cursor_position).cloned();

        // Draw UI
        terminal.draw(|f| {
            // Split the terminal into two main panels: left (50%) and right (50%)
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(f.size());

            // Split the left panel into upper and lower sections
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(10), Constraint::Percentage(80)].as_ref())
                .split(chunks[0]);

            // Split the right panel into upper and lower sections
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Percentage(10),
                        Constraint::Percentage(70),
                        Constraint::Percentage(10),
                    ]
                    .as_ref(),
                )
                .split(chunks[1]);

            // Top Left Panel
            let upper_left_panel = List::new(vec![ListItem::new("To be updated later")])
                .block(Block::default().borders(Borders::ALL).title("Newer Panel"));
            f.render_widget(upper_left_panel, left_chunks[0]);

            // Bottom Left Panel (File Listing)
            let items: Vec<ListItem> = files
                .iter()
                .map(|file| {
                    let style = match get_file_style(&file, &opener_config) {
                        Some(color) => Style::default().fg(color),
                        None => Style::default().fg(Color::White),
                    };
                    ListItem::new(
                        Path::new(&file)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    )
                    .style(style)
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Files"))
                .highlight_style(Style::default().fg(Color::Yellow))
                .highlight_symbol(" ");

            // Render the bottom  left panel (file list)
            let mut state = tui::widgets::ListState::default();
            state.select(Some(cursor_position));
            f.render_stateful_widget(list, left_chunks[1], &mut state);

            // Middle Right Panel (Directory Contents or File Preview)
            let middle_right_panel = match &selected_file {
                Some(file) => {
                    let full_path = current_dir.join(file);
                    if full_path.is_dir() {
                        let items = list_files(&full_path, show_hidden)
                            .unwrap_or_else(|_| vec!["<Empty>".to_string()]);

                        // Color based on file extension
                        let items_with_color: Vec<ListItem> = items
                            .into_iter()
                            .map(|file| {
                                let style = match get_file_style(&file, &opener_config) {
                                    Some(color) => Style::default().fg(color),
                                    None => Style::default().fg(Color::White),
                                };
                                ListItem::new(file).style(style) // Adding the file name directly here
                            })
                            .collect();

                        List::new(items_with_color).block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title("Directory Contents"),
                        )
                    } else {
                        let preview = preview_file(&full_path);
                        List::new(
                            preview
                                .into_iter()
                                .map(ListItem::new)
                                .collect::<Vec<ListItem>>(),
                        )
                        .block(Block::default().borders(Borders::ALL).title("File Preview"))
                    }
                }
                None => List::new(vec![]),
            };
            f.render_widget(middle_right_panel, right_chunks[1]);

            // Top Right Panel
            let top_right_panel = List::new(vec![ListItem::new("To be updated")])
                .block(Block::default().borders(Borders::ALL).title("New Panel"));
            f.render_widget(top_right_panel, right_chunks[0]);

            // Bottom Right Panel
            let bottom_right_panel = List::new(vec![ListItem::new("To be updated")])
                .block(Block::default().borders(Borders::ALL).title("New Panel"));
            f.render_widget(bottom_right_panel, right_chunks[2]);
        })?;

        // Handle input
        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read()?
        {
            match (code, modifiers) {
                // q to exit
                (KeyCode::Char('q'), _) => break,
                // Trigger redrawing on Ctrl + R
                (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                    terminal.draw(|f| {
                        let chunks = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints(
                                [Constraint::Percentage(50), Constraint::Percentage(50)].as_ref(),
                            )
                            .split(f.size());

                        let items: Vec<ListItem> = files
                            .iter()
                            .map(|file| ListItem::new(file.as_str())) // Use `as_str()` to convert `&String` to `&str`
                            .collect();

                        let list = List::new(items)
                            .block(Block::default().borders(Borders::ALL).title("Files"))
                            .highlight_style(Style::default().fg(Color::Yellow))
                            .highlight_symbol(" ");

                        let mut state = tui::widgets::ListState::default();
                        state.select(Some(cursor_position));
                        f.render_stateful_widget(list, chunks[0], &mut state);
                    })?;
                }
                // Navigation with arrow keys and vim-like keys
                (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                    if cursor_position < files.len().saturating_sub(1) {
                        cursor_position += 1;
                    }
                }
                (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                    if cursor_position > 0 {
                        cursor_position -= 1;
                    }
                }
                (KeyCode::Right, _) | (KeyCode::Char('l'), _) => {
                    if let Some(selected_file) = files.get(cursor_position) {
                        let full_path = current_dir.join(selected_file);
                        if full_path.is_dir() {
                            current_dir = full_path;
                            files = list_files(&current_dir, show_hidden)?;
                            cursor_position = 0;
                        }
                    }
                }
                (KeyCode::Left, _) | (KeyCode::Char('h'), _) => {
                    if let Some(parent) = current_dir.parent() {
                        current_dir = parent.to_path_buf();
                        files = list_files(&current_dir, show_hidden)?;
                        cursor_position = 0;
                    }
                }
                // File Opener
                (KeyCode::Enter, _) => {
                    if let Some(selected_file) = files.get(cursor_position) {
                        let full_path = current_dir.join(selected_file);
                        if !full_path.is_dir() {
                            open_file(&full_path, &opener_config);
                        }
                    }
                }
                // Toggle hidden files
                (KeyCode::Char('.'), _) => {
                    show_hidden = !show_hidden;
                    files = list_files(&current_dir, show_hidden)?;
                    cursor_position = 0;
                }
                _ => {}
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;

    // Write the final directory to the cwd file if specified
    if let Some(cwd_file) = cwd_file {
        if let Err(e) = fs::write(&cwd_file, current_dir.to_string_lossy().as_bytes()) {
            eprintln!("Failed to write to cwd file: {}", e);
        }
    }

    Ok(())
}

// Function to toggle hidden files
fn list_files(dir: &Path, show_hidden: bool) -> io::Result<Vec<String>> {
    let entries = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_name = entry.file_name().into_string().ok()?;
            if !show_hidden && file_name.starts_with('.') {
                None
            } else {
                Some(file_name)
            }
        })
        .collect();
    Ok(entries)
}

// Function to load `opener.toml`
fn load_opener_config(config_path: &Path) -> Result<HashMap<String, (String, String)>, io::Error> {
    let toml_contents = fs::read_to_string(config_path)?;
    let value: Value = toml_contents.parse::<Value>().expect("Invalid TOML format");

    let openers = value
        .get("openers")
        .expect("Missing [openers] section in opener.toml")
        .as_table()
        .expect("Invalid TOML table format")
        .iter()
        .map(|(key, val)| {
            let opener = val
                .get("opener")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let color = val
                .get("color")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            (key.clone(), (opener, color))
        })
        .collect();

    Ok(openers)
}

fn get_file_style(file: &str, opener_config: &HashMap<String, (String, String)>) -> Option<Color> {
    if let Some(extension) = Path::new(file).extension().and_then(|ext| ext.to_str()) {
        if let Some((_, color)) = opener_config.get(extension) {
            return match color.as_str() {
                "green" => Some(Color::Green),
                "blue" => Some(Color::Blue),
                "red" => Some(Color::Red),
                "cyan" => Some(Color::Cyan),
                "magenta" => Some(Color::Magenta),
                "yellow" => Some(Color::Yellow),
                "orange" => Some(Color::Rgb(255, 165, 0)),
                "purple" => Some(Color::Rgb(128, 0, 128)),
                "pink" => Some(Color::Rgb(255, 192, 203)),
                "brown" => Some(Color::Rgb(165, 42, 42)),
                "gray" => Some(Color::Gray),
                "darkgray" => Some(Color::DarkGray),
                "lightblue" => Some(Color::Rgb(173, 216, 230)),
                "lightgreen" => Some(Color::Rgb(144, 238, 144)),
                "lightred" => Some(Color::Rgb(255, 182, 193)),
                "lightyellow" => Some(Color::Rgb(255, 255, 224)),
                "lightcyan" => Some(Color::Rgb(224, 255, 255)),
                "lightmagenta" => Some(Color::Rgb(255, 224, 255)),
                "lightorange" => Some(Color::Rgb(255, 200, 150)),
                _ => Some(Color::White), // Default color for unknown extensions
            };
        }
    }
    None
}

// Function to open a file based on its extension
fn open_file(file_path: &Path, opener_config: &HashMap<String, (String, String)>) {
    if let Some(extension) = file_path.extension().and_then(|ext| ext.to_str()) {
        if let Some((command, _)) = opener_config.get(extension) {
            let _ = Command::new(command)
                .arg(file_path)
                .spawn()
                .expect("Failed to open file");
        } else {
            eprintln!("No opener configured for .{} files", extension);
        }
    } else {
        eprintln!("Could not determine file extension.");
    }
}

// Function to preview a file
fn preview_file(file_path: &Path) -> Vec<String> {
    // Try `batcat` first
    let output = Command::new("batcat")
        .args([
            "-n",
            "--style=plain",
            "--color=always",
            "--paging=never",
            "--wrap=never",
        ])
        .arg(file_path)
        .output()
        .or_else(|_| {
            // Fallback to `cat` with line numbers using `nl`
            Command::new("sh")
                .arg("-c")
                .arg(format!("nl {}", file_path.display()))
                .output()
        })
        .unwrap_or_else(|_| Output {
            stdout: Vec::new(),
            stderr: Vec::new(),
            status: std::process::ExitStatus::from_raw(0),
        });

    // If output is empty, check if file exists and is readable
    if output.stdout.is_empty() {
        if !file_path.exists() {
            return vec!["<File does not exist>".to_string()];
        }
        if fs::metadata(file_path).map(|m| m.len()).unwrap_or(0) == 0 {
            return vec!["<Empty file>".to_string()];
        }
        return vec!["<Failed to preview file>".to_string()];
    }

    // Convert output to lines, truncate to 20 lines, and return
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .take(20)
        .map(|line| line.to_string())
        .collect()
}
