use std::{fs, io};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};

use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, List, ListItem, ListState},
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
    let current_dir = std::env::current_dir()?;
    let files = list_files(&current_dir)?;
    let mut state = ListState::default();
    state.select(Some(0));

    loop {
        // Draw UI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1)].as_ref())
                .split(f.size());

            let items: Vec<ListItem> = files
                .iter()
                .map(|file| ListItem::new(file.clone()))
                .collect();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Files"))
                .highlight_style(tui::style::Style::default().fg(tui::style::Color::Yellow))
                .highlight_symbol(">> ");
            f.render_stateful_widget(list, chunks[0], &mut state);
        })?;

        // Handle input
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break, // Quit the program
                KeyCode::Down => move_selection(&mut state, 1, files.len()), // Move down
                KeyCode::Up => move_selection(&mut state, -1, files.len()), // Move up
                KeyCode::Char('r') => { /* Add refresh logic here if needed */ }
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

fn list_files(dir: &std::path::Path) -> io::Result<Vec<String>> {
    let entries = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().into_string().unwrap_or_default())
        .collect();
    Ok(entries)
}

fn move_selection(state: &mut ListState, step: isize, max: usize) {
    if let Some(selected) = state.selected() {
        let new_index = (selected as isize + step).rem_euclid(max as isize) as usize;
        state.select(Some(new_index));
    } else {
        state.select(Some(0));
    }
}
