//! Command Menu (TUI popup)
//!
//! Displays a visual command menu like Claude Code when user types "/"

use ratatui::{
    crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use std::io;

/// Command menu item
#[derive(Debug, Clone)]
pub struct CommandItem {
    /// Command name
    pub name: String,
    /// Description
    pub description: String,
    /// Example usage
    pub example: String,
}

/// All available commands
fn get_commands() -> Vec<CommandItem> {
    vec![
        CommandItem {
            name: "/connect".to_string(),
            description: "Connect to a database".to_string(),
            example: "/connect postgresql://localhost/mydb".to_string(),
        },
        CommandItem {
            name: "/index".to_string(),
            description: "Index the database schema".to_string(),
            example: "/index".to_string(),
        },
        CommandItem {
            name: "/config".to_string(),
            description: "Set API key for LLM provider".to_string(),
            example: "/config anthropic sk-ant-...".to_string(),
        },
        CommandItem {
            name: "/providers".to_string(),
            description: "List all available LLM providers".to_string(),
            example: "/providers".to_string(),
        },
        CommandItem {
            name: "/use".to_string(),
            description: "Switch to a different LLM provider".to_string(),
            example: "/use groq".to_string(),
        },
        CommandItem {
            name: "/model".to_string(),
            description: "Set model for a provider".to_string(),
            example: "/model openai gpt-4".to_string(),
        },
        CommandItem {
            name: "/clear".to_string(),
            description: "Clear chat context".to_string(),
            example: "/clear".to_string(),
        },
        CommandItem {
            name: "/help".to_string(),
            description: "Show detailed help".to_string(),
            example: "/help".to_string(),
        },
        CommandItem {
            name: "/quit".to_string(),
            description: "Exit Schema-Forge".to_string(),
            example: "/quit".to_string(),
        },
    ]
}

/// Result of running the command menu
pub enum MenuResult {
    /// User selected a command
    Command(String),
    /// User cancelled (ESC)
    Cancelled,
    /// User wants to type their own input
    TextInput,
}

/// Display the command menu and return selected command
pub fn show_command_menu() -> io::Result<MenuResult> {
    let commands = get_commands();
    let mut state = ListState::default();
    state.select(Some(0));

    // Setup terminal
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(io::stdout(), EnableMouseCapture)?;

    let stdout = io::stdout();
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let result = run_menu(&mut terminal, &commands, &mut state);

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(io::stdout(), DisableMouseCapture)?;

    result
}

fn run_menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    commands: &[CommandItem],
    state: &mut ListState,
) -> io::Result<MenuResult> {
    loop {
        terminal.draw(|f| ui(f, commands, state))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    return Ok(MenuResult::Cancelled);
                }
                KeyCode::Enter => {
                    if let Some(selected) = state.selected() {
                        return Ok(MenuResult::Command(commands[selected].name.clone()));
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let selected = state.selected().unwrap_or(0);
                    if selected < commands.len() - 1 {
                        state.select(Some(selected + 1));
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let selected = state.selected().unwrap_or(0);
                    if selected > 0 {
                        state.select(Some(selected - 1));
                    }
                }
                KeyCode::Char('/') => {
                    // User typed / again, switch to text input
                    return Ok(MenuResult::TextInput);
                }
                _ => {}
            }
        }
    }
}

fn ui(f: &mut Frame, commands: &[CommandItem], state: &mut ListState) {
    let size = f.area();

    // Create layout: main popup in center with header and scrollable list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Fixed header height
            Constraint::Min(5),     // Scrollable command list
            Constraint::Length(3),  // Fixed help text
        ])
        .split(size);

    // Header block (fixed, doesn't scroll)
    let header = Paragraph::new(vec![
        Line::from(" ⚡ Schema-Forge ").style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Line::from(""),
    ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
        )
        .alignment(Alignment::Center);

    f.render_widget(header, chunks[0]);

    // Command list (scrollable)
    let items: Vec<ListItem> = commands
        .iter()
        .map(|cmd| {
            ListItem::new(format!(
                "  {} {:40} - {}",
                cmd.name, "", cmd.description
            ))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .fg(Color::Black)
                .bg(Color::Cyan)
        );

    f.render_stateful_widget(list, chunks[1], state);

    // Help text at bottom (fixed, doesn't scroll)
    let help_text = vec![
        Line::from(" ↑/k: Up  ↓/j: Down  Enter: Select  ESC/q: Cancel  /: Type command ")
            .style(Style::default().fg(Color::Gray)),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(help, chunks[2]);
}
