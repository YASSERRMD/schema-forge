//! Command Menu (TUI popup)
//!
//! Displays a visual command menu like Claude Code when user types "/"

use ratatui::{
    crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
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
    let mut selected = 0;

    // Setup terminal
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(io::stdout(), EnableMouseCapture)?;

    let stdout = io::stdout();
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let result = run_menu(&mut terminal, &commands, &mut selected);

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(io::stdout(), DisableMouseCapture)?;

    result
}

fn run_menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    commands: &[CommandItem],
    selected: &mut usize,
) -> io::Result<MenuResult> {
    loop {
        terminal.draw(|f| ui(f, commands, selected))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    return Ok(MenuResult::Cancelled);
                }
                KeyCode::Enter => {
                    return Ok(MenuResult::Command(commands[*selected].name.clone()));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if *selected < commands.len() - 1 {
                        *selected += 1;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if *selected > 0 {
                        *selected -= 1;
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

fn ui(f: &mut Frame, commands: &[CommandItem], selected: &usize) {
    let size = f.area();

    // Create layout: main popup in center
    let popup_width = std::cmp::min(80, size.width - 4);
    let popup_height = std::cmp::min(20, size.height - 4);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(popup_height),
            Constraint::Min(0),
        ])
        .split(size);

    // Command list
    let items: Vec<ListItem> = commands
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let style = if i == *selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::White)
            };

            ListItem::new(format!(
                "  {} {:40} - {}",
                cmd.name, "", cmd.description
            ))
            .style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" ⚡ Schema-Forge ")
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .fg(Color::Black)
                .bg(Color::Cyan)
        );

    f.render_widget(list, chunks[0]);

    // Help text at bottom
    let help_text = vec![
        Line::from(" ↑/k: Up  ↓/j: Down  Enter: Select  ESC/q: Cancel  /: Type command "),
    ];

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    let help_area = Rect {
        x: chunks[0].x,
        y: chunks[0].y + chunks[0].height - 3,
        width: chunks[0].width,
        height: 3,
    };

    f.render_widget(help, help_area);
}
