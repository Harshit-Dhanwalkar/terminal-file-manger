use std::env;
use std::fs;

use std::io;
// use std::io::{self, Write};

use std::path::{Path, PathBuf};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};

use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem},
    Terminal,
};

use std::collections::HashMap;
use std::process::Command;
use toml::Value;

fn main() -> Result<(), io::Error> {
    // Parse arguments
    let project_dir = env::current_dir().unwrap();
    // println!("Project directory: {}", project_dir.display());
    let path_file = project_dir.join("src").join("path.txt");
    // println!("Looking for file at: {}", path_file.display());
    if !path_file.exists() {
        eprintln!("Error: path.txt not found in {}", path_file.display());
        return Ok(());
    }
    // let path_contents = fs::read_to_string(path_file)?;
    // println!("path.txt contents: {}", path_contents);
    let opener_config_path = project_dir.join("src").join("opener.toml");
    if !opener_config_path.exists() {
        eprintln!(
            "Error: opener.toml not found in {}",
            opener_config_path.display()
        );
        return Ok(());
    }
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

    loop {
        // Get the selected file or directory
        let selected_file = files.get(cursor_position).cloned();

        // Draw UI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(f.size());

            // Left Panel (File Listing)
            let items: Vec<ListItem> = files
                .iter()
                .map(|file| ListItem::new(file.clone()))
                .collect();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Files"))
                .highlight_style(tui::style::Style::default().fg(tui::style::Color::Yellow))
                .highlight_symbol(" ");

            // Create ListState and set cursor position manually
            let mut state = tui::widgets::ListState::default();
            state.select(Some(cursor_position));
            f.render_stateful_widget(list, chunks[0], &mut state);

            // Right Panel (Directory Contents)
            let right_panel = match &selected_file {
                Some(file) => {
                    let full_path = current_dir.join(file);
                    if full_path.is_dir() {
                        let items = list_files(&full_path, show_hidden)
                            .unwrap_or_else(|_| vec!["<Empty>".to_string()]);
                        let list = List::new(
                            items
                                .into_iter()
                                .map(ListItem::new)
                                .collect::<Vec<ListItem>>(),
                        )
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title("Directory Contents"),
                        );
                        list
                    } else {
                        let preview = preview_file(&full_path);
                        let list = List::new(
                            preview
                                .into_iter()
                                .map(ListItem::new)
                                .collect::<Vec<ListItem>>(),
                        )
                        .block(Block::default().borders(Borders::ALL).title("File Preview"));
                        list
                    }
                }
                None => List::new(vec![]),
            };
            f.render_widget(right_panel, chunks[1]);
        })?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break,
                // Navigation with arrow keys and vim-like keys
                KeyCode::Down | KeyCode::Char('j') => {
                    if cursor_position < files.len().saturating_sub(1) {
                        cursor_position += 1;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if cursor_position > 0 {
                        cursor_position -= 1;
                    }
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    if let Some(selected_file) = files.get(cursor_position) {
                        let full_path = current_dir.join(selected_file);
                        if full_path.is_dir() {
                            current_dir = full_path;
                            files = list_files(&current_dir, show_hidden)?;
                            cursor_position = 0;
                        }
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    if let Some(parent) = current_dir.parent() {
                        current_dir = parent.to_path_buf();
                        files = list_files(&current_dir, show_hidden)?;
                        cursor_position = 0;
                    }
                }
                // file Opener
                KeyCode::Enter => {
                    if let Some(selected_file) = files.get(cursor_position) {
                        let full_path = current_dir.join(selected_file);
                        if !full_path.is_dir() {
                            open_file(&full_path, &opener_config);
                        }
                    }
                }
                // Toggle hidden files
                KeyCode::Char('.') => {
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

// Function to preview a file
fn preview_file(file_path: &Path) -> Vec<String> {
    // TODO: Use batcat to preview the file
    let output = Command::new("cat").arg(file_path).output();
    match output {
        Ok(output) if !output.stdout.is_empty() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .take(20)
            .map(|line| line.to_string())
            .collect(),
        _ => vec!["<Failed to preview file>".to_string()],
    }
}

// Function to toggle hidden files
fn list_files(dir: &std::path::Path, show_hidden: bool) -> io::Result<Vec<String>> {
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
fn load_opener_config(config_path: &Path) -> Result<HashMap<String, String>, io::Error> {
    let toml_contents = fs::read_to_string(config_path)?;
    let value: Value = toml_contents.parse::<Value>().expect("Invalid TOML format");
    let openers = value
        .get("openers")
        .expect("Missing [openers] section in opener.toml")
        .as_table()
        .expect("Invalid TOML table format")
        .iter()
        .map(|(key, val)| {
            (
                key.clone(),
                val.as_str()
                    .expect("Values in [openers] must be strings")
                    .to_string(),
            )
        })
        .collect();
    Ok(openers)
}

// Function to open a file based on its extension
fn open_file(file_path: &Path, opener_config: &HashMap<String, String>) {
    if let Some(ext) = file_path.extension().and_then(|s| s.to_str()) {
        if let Some(opener) = opener_config.get(ext) {
            let _ = Command::new(opener)
                .arg(file_path)
                .spawn()
                .expect("Failed to open file with specified program");
        } else {
            eprintln!("No opener configured for extension: {}", ext);
        }
    } else {
        eprintln!("File has no extension: {}", file_path.display());
    }
}
