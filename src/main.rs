use std::{fs, io};

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

fn main() -> Result<(), io::Error> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // File Manager State
    let mut current_dir = std::env::current_dir()?;
    let mut show_hidden = true;
    let mut files = list_files(&current_dir, show_hidden)?;
    let mut cursor_position: usize = 0;

    loop {
        // Get the selected file or directory
        let selected_file = files.get(cursor_position).cloned();
        let right_panel_contents = match &selected_file {
            Some(file) => {
                let full_path = current_dir.join(file);
                if full_path.is_dir() {
                    list_files(&full_path, show_hidden)
                        .unwrap_or_else(|_| vec!["<Empty>".to_string()])
                } else {
                    vec!["<Not a directory>".to_string()]
                }
            }
            None => vec!["<No Selection>".to_string()],
        };

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
            let right_items: Vec<ListItem> = right_panel_contents
                .iter()
                .map(|item| ListItem::new(item.clone()))
                .collect();
            let right_panel = List::new(right_items)
                .block(Block::default().borders(Borders::ALL).title("Contents"));
            f.render_widget(right_panel, chunks[1]);
        })?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break,
                // Navigation with arrow keys and vim-like keys
                KeyCode::Down | KeyCode::Char('j') => {
                    if cursor_position < files.len() - 1 {
                        cursor_position += 1;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if cursor_position > 0 {
                        cursor_position -= 1;
                    }
                }
                // Navigate into a directory (right or 'l')
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
                // Navigate back (left or 'h')
                KeyCode::Left | KeyCode::Char('h') => {
                    if let Some(parent) = current_dir.parent() {
                        current_dir = parent.to_path_buf();
                        files = list_files(&current_dir, show_hidden)?;
                        cursor_position = 0;
                    }
                }
                // Toggle hidden files
                KeyCode::Char('.') => {
                    show_hidden = !show_hidden;
                    files = list_files(&current_dir, show_hidden)?;
                    cursor_position = 0; // Reset cursor position after toggle
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
    Ok(())
}

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
