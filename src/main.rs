use std::env;
use std::fs;
use std::io;
// use std::io::{self, Write};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
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
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(f.size());

            // Divide the right panel into two sections (3:4 ratio)
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(75), Constraint::Percentage(25)].as_ref())
                .split(chunks[1]);

            // Left Panel (File Listing)
            let items: Vec<ListItem> = files
                .iter()
                .map(|file| {
                    let style = match get_file_style(file, &opener_config) {
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
                .highlight_style(tui::style::Style::default().fg(tui::style::Color::Yellow))
                .highlight_symbol(" ");

            // Create ListState and set cursor position manually
            let mut state = tui::widgets::ListState::default();
            state.select(Some(cursor_position));
            f.render_stateful_widget(list, chunks[0], &mut state);

            // Upper Right Panel (Directory Contents)
            let upper_right_panel = match &selected_file {
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
            f.render_widget(upper_right_panel, chunks[1]);

            // TODO:
            // Lower Right Panel
            // let lower_right_panel =
            //     List::new(vec![ListItem::new(format!("Time {}", current_time))])
            //         .block(Block::default().borders(Borders::ALL).title("Extra Panel"));
            // f.render_widget(lower_right_panel, right_chunks[1]);
            let lower_right_panel = List::new(vec![ListItem::new("To be updated")])
                .block(Block::default().borders(Borders::ALL).title("New Panel"));
            f.render_widget(lower_right_panel, right_chunks[1]);
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
                _ => Some(Color::White),
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
