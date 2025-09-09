// use crossterm::style::SetForegroundColor;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen},
};
use std::collections::HashMap;
use std::env;
use std::fs;
// use std::fs::File, OpenOptions};
// use std::fs::{self, DirEntry, File};
use std::io::{self, BufRead, Write};
// use std::io::{self, stdin, BufRead, ErrorKind, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use toml::Value;
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color as TuiColor, Style},
    widgets::{Block, Borders, List, ListItem},
    Terminal,
};
// use chrono::Local; // For date and time
use libc;
use std::sync::atomic::{AtomicBool, Ordering};
// use ncurses::*;
// use std::cmp;
// use std::ops::{Add, Mul};
// use std::process;
use termion::event::Key;
use termion::input::TermRead;

// SIGINT Handler (Ctrl+C)
static CTRLC: AtomicBool = AtomicBool::new(false);

extern "C" fn callback(_signum: i32) {
    CTRLC.store(true, Ordering::SeqCst);
    // CTRLC.store(true, Ordering::Relaxed);
}

fn init_signal_handler() {
    unsafe {
        libc::signal(libc::SIGINT, callback as usize);
    }
}

fn poll_signal() -> bool {
    CTRLC.load(Ordering::SeqCst)
    // CTRLC.swap(false, Ordering::Relaxed)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize signal handler for SIGINT (Ctrl+C)
    init_signal_handler();
    let stdin = io::stdin();
    let mut keys = stdin.lock().keys();
    let mut stdout = io::stdout();
    let mut search_query = String::new();

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
    let opener_config = match load_opener_config(&opener_config_path) {
        Ok(config) => {
            println!("Loaded opener.toml configuration");
            config
        }
        Err(e) => {
            eprintln!("Failed to load opener.toml: {}", e);
            return Ok(());
        }
    };
    println!("Loaded opener.toml configuration");

    // Setup terminal
    enable_raw_mode()?;
    // let mut stdout = io::stdout();
    execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
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
    let mut show_hidden = false;
    let mut files = list_files(&current_dir, show_hidden)?;
    let mut cursor_position: usize = 0;
    let mut preview_cache: Option<(PathBuf, Vec<String>)> = None;
    let mut last_selected_file_path: Option<PathBuf> = None;

    // Get current date and time
    // let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut quit = false;

    while !quit && !poll_signal() {
        // loop {
        // Get the selected file or directory
        let selected_file = files.get(cursor_position).cloned();

        if let Some(file_name) = &selected_file {
            let full_path = current_dir.join(file_name);
            if full_path.is_file() && last_selected_file_path.as_ref() != Some(&full_path) {
                preview_cache = Some((full_path.clone(), preview_file(&full_path)));
                last_selected_file_path = Some(full_path);
            }
        }

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
                .constraints([Constraint::Percentage(7), Constraint::Percentage(83)].as_ref())
                .split(chunks[0]);

            // Split the right panel into top, middle and bottom sections
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Percentage(7),
                        Constraint::Percentage(73),
                        Constraint::Percentage(10),
                    ]
                    .as_ref(),
                )
                .split(chunks[1]);

            // Upper Left Panel: Display the current working directory (pwd)
            let current_dir_display = current_dir.to_string_lossy().into_owned();
            let upper_left_panel = List::new(vec![ListItem::new(current_dir_display)]).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Current Directory"),
            );
            f.render_widget(upper_left_panel, left_chunks[0]);

            // Bottom Left Panel (File Listing)
            let items: Vec<ListItem> = files
                .iter()
                .map(|file| {
                    let style = match get_file_style(&file, &opener_config) {
                        Some(color) => Style::default().fg(color),
                        None => Style::default().fg(TuiColor::White),
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
                .highlight_style(Style::default().fg(TuiColor::Yellow))
                .highlight_symbol(" ");

            let mut state = tui::widgets::ListState::default();
            state.select(Some(cursor_position));
            f.render_stateful_widget(list, left_chunks[1], &mut state);

            // Top Right Panel
            let top_right_panel = List::new(vec![ListItem::new("To be updated")])
                .block(Block::default().borders(Borders::ALL).title("New Panel"));
            f.render_widget(top_right_panel, right_chunks[0]);

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
                                    None => Style::default().fg(TuiColor::White),
                                };
                                ListItem::new(file).style(style)
                            })
                            .collect();

                        List::new(items_with_color).block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title("Directory Contents"),
                        )
                    } else {
                        // Use cached preview
                        if let Some((cached_path, cached_preview)) = &preview_cache {
                            if cached_path == &full_path {
                                List::new(
                                    cached_preview
                                        .iter()
                                        .map(|line| ListItem::new(line.as_str()))
                                        .collect::<Vec<ListItem>>(),
                                )
                                .block(Block::default().borders(Borders::ALL).title("File Preview"))
                            } else {
                                List::new(vec![ListItem::new("<Loading preview...>".to_string())])
                                    .block(
                                        Block::default()
                                            .borders(Borders::ALL)
                                            .title("File Preview"),
                                    )
                            }
                        } else {
                            List::new(vec![ListItem::new("<Loading preview...>".to_string())])
                                .block(Block::default().borders(Borders::ALL).title("File Preview"))
                        }
                    }
                }
                None => List::new(vec![]),
            };
            f.render_widget(middle_right_panel, right_chunks[1]);

            // Bottom right panel (to-do list)
            let bottom_right_panel = List::new(vec![ListItem::new("To be updated")])
                .block(Block::default().borders(Borders::ALL).title("New Panel"));
            // let todo_items = get_todo_items(&todos);
            // let bottom_right_panel = List::new(todo_items)
            //     .block(Block::default().borders(Borders::ALL).title("To-Do List"));
            f.render_widget(bottom_right_panel, right_chunks[2]);
        })?;

        // Handle input
        //// q to quit
        if let Some(Ok(Key::Char('q'))) = keys.next() {
            quit = true;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                match (code, modifiers) {
                    // q to quit
                    (KeyCode::Char('q'), _) => break,
                    // Trigger redrawing on Ctrl + R
                    (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                        terminal.draw(|f| {
                            let chunks = Layout::default()
                                .direction(Direction::Horizontal)
                                .constraints(
                                    [Constraint::Percentage(50), Constraint::Percentage(50)]
                                        .as_ref(),
                                )
                                .split(f.size());

                            let items: Vec<ListItem> = files
                                .iter()
                                .map(|file| ListItem::new(file.as_str()))
                                .collect();

                            let list = List::new(items)
                                .block(Block::default().borders(Borders::ALL).title("Files"))
                                .highlight_style(Style::default().fg(TuiColor::Yellow))
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
                    // Search file
                    (KeyCode::Char('/'), _) => {
                        execute!(&mut stdout, EnterAlternateScreen)?;
                        print!("Search: ");
                        stdout.flush()?;

                        let mut search_stdin = io::stdin().lock();
                        BufRead::read_line(&mut search_stdin, &mut search_query)?;
                        search_query = search_query.trim().to_string();

                        if !search_query.is_empty() {
                            files = search_files(&current_dir, &search_query)?
                                .into_iter()
                                .map(|path| path.to_string_lossy().into_owned())
                                .collect();
                        }
                        cursor_position = 0;
                    }
                    // TODO: Add todolist keymaps
                    _ => {}
                }
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

    // quit function
    writeln!(stdout, "Exiting...").unwrap();
    stdout.flush().unwrap();

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
    let value: Value = match toml_contents.parse::<Value>() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing opener.toml: {}", e);
            return Ok(HashMap::new());
        }
    };

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

fn get_file_style(
    file: &str,
    opener_config: &HashMap<String, (String, String)>,
) -> Option<TuiColor> {
    if let Some(extension) = Path::new(file).extension().and_then(|ext| ext.to_str()) {
        if let Some((_, color)) = opener_config.get(extension) {
            return match color.as_str() {
                "green" => Some(TuiColor::Green),
                "blue" => Some(TuiColor::Blue),
                "red" => Some(TuiColor::Red),
                "cyan" => Some(TuiColor::Cyan),
                "magenta" => Some(TuiColor::Magenta),
                "yellow" => Some(TuiColor::Yellow),
                "orange" => Some(TuiColor::Rgb(255, 165, 0)),
                "purple" => Some(TuiColor::Rgb(128, 0, 128)),
                "pink" => Some(TuiColor::Rgb(255, 192, 203)),
                "brown" => Some(TuiColor::Rgb(165, 42, 42)),
                "gray" => Some(TuiColor::Gray),
                "darkgray" => Some(TuiColor::DarkGray),
                "lightblue" => Some(TuiColor::Rgb(173, 216, 230)),
                "lightgreen" => Some(TuiColor::Rgb(144, 238, 144)),
                "lightred" => Some(TuiColor::Rgb(255, 182, 193)),
                "lightyellow" => Some(TuiColor::Rgb(255, 255, 224)),
                "lightcyan" => Some(TuiColor::Rgb(224, 255, 255)),
                "lightmagenta" => Some(TuiColor::Rgb(255, 224, 255)),
                "lightorange" => Some(TuiColor::Rgb(255, 200, 150)),
                _ => Some(TuiColor::White), // Default color for unknown extensions
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

// File search
fn search_files(dir: &Path, keyword: &str) -> io::Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.contains(keyword) {
                results.push(path);
            }
        }
    }
    Ok(results)
}
