use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dirs;
use libc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use toml::Value;
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color as TuiColor, Style},
    widgets::{Block, Borders, List, ListItem, ListState},
    Terminal,
};

// SIGINT Handler (Ctrl+C)
static CTRLC: AtomicBool = AtomicBool::new(false);

extern "C" fn callback(_signum: i32) {
    CTRLC.store(true, Ordering::SeqCst);
}

fn init_signal_handler() {
    unsafe {
        libc::signal(libc::SIGINT, callback as usize);
    }
}

fn poll_signal() -> bool {
    CTRLC.load(Ordering::SeqCst)
}

#[derive(Serialize, Deserialize, Clone)]
struct Todo {
    description: String,
    completed: bool,
}

#[derive(Default)]
struct DirectoryCache {
    entries: HashMap<PathBuf, (Vec<String>, std::time::SystemTime)>,
}

impl DirectoryCache {
    fn get_entries(&mut self, path: &Path, show_hidden: bool) -> io::Result<&Vec<String>> {
        let metadata = fs::metadata(path)?;
        let modified = metadata.modified()?;

        if let Some((entries, last_modified)) = self.entries.get_mut(path) {
            if &modified > last_modified {
                *entries = list_files(path, show_hidden)?;
                *last_modified = modified;
            }
        } else {
            let entries = list_files(path, show_hidden)?;
            self.entries.insert(path.to_path_buf(), (entries, modified));
        }

        Ok(&self.entries[path].0)
    }
}

fn load_todos() -> Vec<Todo> {
    let home = match dirs::home_dir() {
        Some(path) => path,
        None => return vec![],
    };
    let todo_path = home.join(".termfm_todo.json");

    if todo_path.exists() {
        if let Ok(file_content) = fs::read_to_string(todo_path) {
            if let Ok(todos) = serde_json::from_str(&file_content) {
                return todos;
            }
        }
    }
    vec![]
}

fn save_todos(todos: &Vec<Todo>) {
    let home = match dirs::home_dir() {
        Some(path) => path,
        None => return,
    };
    let todo_path = home.join(".termfm_todo.json");
    if let Ok(serialized_todos) = serde_json::to_string(&todos) {
        let _ = fs::write(todo_path, serialized_todos);
    }
}

fn add_todo() -> Option<Todo> {
    // Save current terminal state
    let mut stdout = io::stdout();
    let _ = disable_raw_mode();
    let _ = execute!(stdout, LeaveAlternateScreen, Show);

    println!("Enter new task: ");
    let _ = stdout.flush();

    let mut new_task = String::new();
    let mut stdin = io::stdin();
    if stdin.read_line(&mut new_task).is_err() {
        // Restore terminal state on error
        let _ = enable_raw_mode();
        let _ = execute!(stdout, EnterAlternateScreen);
        return None;
    }

    // Restore terminal state
    let _ = enable_raw_mode();
    let _ = execute!(stdout, EnterAlternateScreen);

    let trimmed_task = new_task.trim();
    if !trimmed_task.is_empty() {
        Some(Todo {
            description: trimmed_task.to_string(),
            completed: false,
        })
    } else {
        None
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_signal_handler();

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

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut cwd_file: Option<PathBuf> = None;
    for arg in env::args().skip(1) {
        if arg.starts_with("--cwd-file=") {
            cwd_file = Some(PathBuf::from(arg.trim_start_matches("--cwd-file=")));
        }
    }

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
    let mut dir_cache = DirectoryCache::default();
    let mut files = dir_cache.get_entries(&current_dir, show_hidden)?.clone();
    let mut cursor_position: usize = 0;
    let mut preview_cache: Option<(PathBuf, Vec<String>)> = None;
    let mut last_selected_file_path: Option<PathBuf> = None;
    let mut search_query = String::new();
    let mut todos = load_todos();
    let mut todo_list_state = ListState::default();
    if !todos.is_empty() {
        todo_list_state.select(Some(0));
    }
    let mut quit = false;

    while !quit && !poll_signal() {
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
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                .split(f.size());

            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(7), Constraint::Percentage(93)].as_ref())
                .split(chunks[0]);

            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Percentage(7),
                        Constraint::Percentage(63),
                        Constraint::Percentage(30),
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
                .highlight_symbol(">> ");

            let mut state = tui::widgets::ListState::default();
            state.select(Some(cursor_position));
            f.render_stateful_widget(list, left_chunks[1], &mut state);

            // Right Panel
            let upper_right_panel = List::new(vec![ListItem::new("To be updated")])
                .block(Block::default().borders(Borders::ALL).title("New Panel"));
            f.render_widget(upper_right_panel, right_chunks[0]);

            let middle_right_panel = match &selected_file {
                Some(file) => {
                    let full_path = current_dir.join(file);
                    if full_path.is_dir() {
                        let items = list_files(&full_path, show_hidden)
                            .unwrap_or_else(|_| vec!["<Empty>".to_string()]);

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

            let bottom_right_panel: Vec<ListItem> = todos
                .iter()
                .map(|todo| {
                    let status = if todo.completed { "✓ " } else { "☐ " };
                    ListItem::new(format!("{} {}", status, todo.description))
                })
                .collect();

            let todo_list = List::new(bottom_right_panel)
                .block(Block::default().borders(Borders::ALL).title("To-Do List"))
                .highlight_style(Style::default().fg(TuiColor::Yellow));

            f.render_stateful_widget(todo_list, right_chunks[2], &mut todo_list_state);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                match (code, modifiers) {
                    (KeyCode::Char('q'), _) => {
                        save_todos(&todos);
                        quit = true;
                    }
                    (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                        todo!()
                    }
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
                                // files = list_files(&current_dir, show_hidden)?;
                                files = dir_cache.get_entries(&current_dir, show_hidden)?.clone();
                                cursor_position = 0;
                            }
                        }
                    }
                    (KeyCode::Left, _) | (KeyCode::Char('h'), _) => {
                        if let Some(parent) = current_dir.parent() {
                            current_dir = parent.to_path_buf();
                            // files = list_files(&current_dir, show_hidden)?;
                            files = dir_cache.get_entries(&current_dir, show_hidden)?.clone();
                            cursor_position = 0;
                        }
                    }
                    (KeyCode::Enter, _) => {
                        if let Some(selected_file) = files.get(cursor_position) {
                            let full_path = current_dir.join(selected_file);
                            if !full_path.is_dir() {
                                open_file(&full_path, &opener_config);
                            }
                        }
                    }
                    (KeyCode::Char('.'), _) => {
                        show_hidden = !show_hidden;
                        // files = list_files(&current_dir, show_hidden)?;
                        files = dir_cache.get_entries(&current_dir, show_hidden)?.clone();
                        cursor_position = 0;
                    }
                    (KeyCode::Char('/'), _) => {
                        let mut stdout = io::stdout();
                        let _ = disable_raw_mode();
                        let _ = execute!(stdout, LeaveAlternateScreen, Show);

                        print!("Search: ");
                        let _ = stdout.flush();

                        let mut search_input = String::new();
                        let stdin = io::stdin();
                        if stdin.read_line(&mut search_input).is_ok() {
                            search_query = search_input.trim().to_string();

                            if !search_query.is_empty() {
                                files = search_files(&current_dir, &search_query)?
                                    .into_iter()
                                    .map(|path| {
                                        path.file_name().unwrap().to_string_lossy().into_owned()
                                    })
                                    .collect();
                            } else {
                                // Reset to normal listing if search is empty
                                files = dir_cache.get_entries(&current_dir, show_hidden)?.clone();
                            }
                        }

                        let _ = enable_raw_mode();
                        let _ = execute!(stdout, EnterAlternateScreen);
                        cursor_position = 0;
                    }
                    (KeyCode::Char('a'), _) => {
                        if let Some(new_todo) = add_todo() {
                            todos.push(new_todo);
                        }
                    }
                    (KeyCode::Char('d'), _) => {
                        if let Some(selected_index) = todo_list_state.selected() {
                            if selected_index < todos.len() {
                                todos.remove(selected_index);
                                if !todos.is_empty() && selected_index >= todos.len() {
                                    todo_list_state.select(Some(todos.len() - 1));
                                }
                            }
                        }
                    }
                    (KeyCode::Char(' '), _) => {
                        if let Some(selected_index) = todo_list_state.selected() {
                            if let Some(todo) = todos.get_mut(selected_index) {
                                todo.completed = !todo.completed;
                            }
                        }
                    }
                    (KeyCode::Char('+'), _) => {
                        if !todos.is_empty() {
                            let mut selected_index = todo_list_state.selected().unwrap_or(0);
                            if selected_index < todos.len() - 1 {
                                selected_index += 1;
                                todo_list_state.select(Some(selected_index));
                            }
                        }
                    }
                    (KeyCode::Char('-'), _) => {
                        if !todos.is_empty() {
                            let mut selected_index = todo_list_state.selected().unwrap_or(0);
                            if selected_index > 0 {
                                selected_index -= 1;
                                todo_list_state.select(Some(selected_index));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, Show)?;
    if let Some(cwd_file) = cwd_file {
        if let Err(e) = fs::write(&cwd_file, current_dir.to_string_lossy().as_bytes()) {
            eprintln!("Failed to write to cwd file: {}", e);
        }
    }
    writeln!(io::stdout(), "Exiting...").unwrap();
    io::stdout().flush().unwrap();
    Ok(())
}

fn list_files(dir: &Path, show_hidden: bool) -> io::Result<Vec<String>> {
    let mut entries: Vec<String> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_name = entry.file_name().into_string().unwrap_or_default();

        if !show_hidden && file_name.starts_with('.') {
            continue;
        }

        entries.push(file_name);
    }

    entries.sort_by(|a, b| {
        let a_is_dir = dir.join(a).is_dir();
        let b_is_dir = dir.join(b).is_dir();

        if a_is_dir && !b_is_dir {
            std::cmp::Ordering::Less
        } else if !a_is_dir && b_is_dir {
            std::cmp::Ordering::Greater
        } else {
            a.to_lowercase().cmp(&b.to_lowercase())
        }
    });

    Ok(entries)
}

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
                _ => Some(TuiColor::White),
            };
        }
    }
    None
}

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

fn preview_file(file_path: &Path) -> Vec<String> {
    if let Ok(metadata) = fs::metadata(file_path) {
        if metadata.len() > 1_000_000 {
            return vec!["<File too large for preview>".to_string()];
        }
    }
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

    if output.stdout.is_empty() {
        if !file_path.exists() {
            return vec!["<File does not exist>".to_string()];
        }
        if fs::metadata(file_path).map(|m| m.len()).unwrap_or(0) == 0 {
            return vec!["<Empty file>".to_string()];
        }
        return vec!["<Failed to preview file>".to_string()];
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .take(20)
        .map(|line| line.to_string())
        .collect()
}

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
