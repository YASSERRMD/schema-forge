//! Command Menu (TUI popup)
//!
//! Displays a visual command menu like Claude Code when user types "/"

use ratatui::{
    crossterm::{
        cursor,
        event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
        terminal::{EnterAlternateScreen, LeaveAlternateScreen},
    },
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
    /// Whether the command needs additional user input
    pub requires_args: bool,
}

/// All available commands
fn get_commands() -> Vec<CommandItem> {
    vec![
        CommandItem {
            name: "/connect".to_string(),
            description: "Connect to a database".to_string(),
            example: "/connect postgresql://localhost/mydb".to_string(),
            requires_args: true,
        },
        CommandItem {
            name: "/index".to_string(),
            description: "Index the database schema".to_string(),
            example: "/index".to_string(),
            requires_args: false,
        },
        CommandItem {
            name: "/config".to_string(),
            description: "Set API key for LLM provider".to_string(),
            example: "/config anthropic sk-ant-...".to_string(),
            requires_args: true,
        },
        CommandItem {
            name: "/providers".to_string(),
            description: "List all available LLM providers".to_string(),
            example: "/providers".to_string(),
            requires_args: false,
        },
        CommandItem {
            name: "/use".to_string(),
            description: "Switch to a different LLM provider".to_string(),
            example: "/use groq".to_string(),
            requires_args: true,
        },
        CommandItem {
            name: "/model".to_string(),
            description: "Set model for a provider".to_string(),
            example: "/model openai gpt-4".to_string(),
            requires_args: true,
        },
        CommandItem {
            name: "/clear".to_string(),
            description: "Clear chat context".to_string(),
            example: "/clear".to_string(),
            requires_args: false,
        },
        CommandItem {
            name: "/help".to_string(),
            description: "Show detailed help".to_string(),
            example: "/help".to_string(),
            requires_args: false,
        },
        CommandItem {
            name: "/quit".to_string(),
            description: "Exit Schema-Forge".to_string(),
            example: "/quit".to_string(),
            requires_args: false,
        },
    ]
}

/// Result of running the command menu
pub enum MenuResult {
    /// User selected a command
    Command { initial_input: String },
    /// User cancelled (ESC)
    Cancelled,
    /// User wants to type their own input
    TextInput,
}

/// Terminal mode guard to guarantee terminal restoration.
struct TerminalGuard;

impl TerminalGuard {
    fn activate() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableMouseCapture,
            cursor::Hide
        )?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            io::stdout(),
            DisableMouseCapture,
            LeaveAlternateScreen,
            cursor::Show
        );
    }
}

/// Display the command menu and return selected command
pub fn show_command_menu() -> io::Result<MenuResult> {
    let commands = get_commands();
    let mut state = ListState::default();
    state.select(Some(0));
    let mut filter = String::new();

    let _terminal_guard = TerminalGuard::activate()?;

    let stdout = io::stdout();
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    run_menu(&mut terminal, &commands, &mut state, &mut filter)
}

fn filtered_indices(commands: &[CommandItem], filter: &str) -> Vec<usize> {
    let normalized = filter.to_lowercase();
    commands
        .iter()
        .enumerate()
        .filter(|(_, cmd)| {
            normalized.is_empty()
                || cmd.name.to_lowercase().contains(&normalized)
                || cmd.description.to_lowercase().contains(&normalized)
        })
        .map(|(index, _)| index)
        .collect()
}

fn keep_selection_valid(state: &mut ListState, filtered_len: usize) {
    match (filtered_len, state.selected()) {
        (0, _) => state.select(None),
        (_, None) => state.select(Some(0)),
        (len, Some(selected)) if selected >= len => state.select(Some(len - 1)),
        _ => {}
    }
}

fn run_menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    commands: &[CommandItem],
    state: &mut ListState,
    filter: &mut String,
) -> io::Result<MenuResult> {
    loop {
        let visible = filtered_indices(commands, filter);
        keep_selection_valid(state, visible.len());

        terminal.draw(|f| ui(f, commands, state, &visible, filter))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    return Ok(MenuResult::Cancelled);
                }
                KeyCode::Enter => {
                    if let Some(selected_visible_index) = state.selected() {
                        let selected_index = visible[selected_visible_index];
                        let command = &commands[selected_index];
                        let initial_input = if command.requires_args {
                            format!("{} ", command.name)
                        } else {
                            command.name.clone()
                        };

                        return Ok(MenuResult::Command { initial_input });
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let selected = state.selected().unwrap_or(0);
                    if selected + 1 < visible.len() {
                        state.select(Some(selected + 1));
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let selected = state.selected().unwrap_or(0);
                    if selected > 0 {
                        state.select(Some(selected - 1));
                    }
                }
                KeyCode::Backspace => {
                    filter.pop();
                }
                KeyCode::Char('/') => {
                    if filter.is_empty() {
                        return Ok(MenuResult::TextInput);
                    }
                    filter.push('/');
                }
                KeyCode::Char(c) => {
                    if !c.is_control() {
                        filter.push(c);
                    }
                }
                _ => {}
            }
        }
    }
}

fn ui(
    f: &mut Frame,
    commands: &[CommandItem],
    state: &mut ListState,
    visible: &[usize],
    filter: &str,
) {
    let size = f.area();

    // Create layout: main popup in center with header and scrollable list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(9), // Fixed header height for ASCII art
            Constraint::Length(3), // Search box
            Constraint::Min(5),    // Scrollable command list
            Constraint::Length(4), // Command detail
            Constraint::Length(3), // Fixed help text
        ])
        .split(size);

    // Header block (fixed, doesn't scroll) - with ASCII art
    let header = Paragraph::new(vec![
        Line::from(
            "████████╗███████╗██████╗ ██████╗ ██████╗ ███████╗    █████╗ ██╗   ██╗████████╗",
        ),
        Line::from(
            "╚══██╔══╝██╔════╝██╔══██╗██╔═══██╗██╔══██╗██╔════╝   ██╔══██╗██║   ██║╚══██╔══╝",
        ),
        Line::from(
            "   ██║   █████╗  ██████╔╝██║   ██║██████╔╝█████╗     ███████║██║   ██║   ██║   ",
        ),
        Line::from(
            "   ██║   ██╔══╝  ██╔══██╗██║   ██║██╔══██╗██╔══╝     ██╔══██║██║   ██║   ██║   ",
        ),
        Line::from(
            "   ██║   ███████╗██║  ██║╚██████╔╝██║  ██║███████╗   ██║  ██║╚██████╔╝   ██║   ",
        ),
        Line::from(
            "   ╚═╝   ╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝╚══════╝   ╚═╝  ╚═╝ ╚═════╝    ╚═╝   ",
        ),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    )
    .alignment(Alignment::Center);

    f.render_widget(header, chunks[0]);

    let search = Paragraph::new(format!("  /{}", filter))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Filter commands ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    f.render_widget(search, chunks[1]);

    // Command list (scrollable)
    let items: Vec<ListItem> = if visible.is_empty() {
        vec![ListItem::new("  No matching commands")]
    } else {
        visible
            .iter()
            .map(|idx| {
                let cmd = &commands[*idx];
                ListItem::new(format!("  {:<12} {}", cmd.name, cmd.description))
            })
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Commands ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .fg(Color::Black)
                .bg(Color::Cyan),
        );

    f.render_stateful_widget(list, chunks[2], state);

    let detail_line = if let Some(selected_visible_index) = state.selected() {
        let selected_index = visible[selected_visible_index];
        format!("Example: {}", commands[selected_index].example)
    } else {
        "Example: (no command selected)".to_string()
    };

    let details = Paragraph::new(vec![
        Line::from(detail_line).style(Style::default().fg(Color::Gray))
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Selected Command ")
            .border_style(Style::default().fg(Color::Cyan)),
    )
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: true });

    f.render_widget(details, chunks[3]);

    // Help text at bottom (fixed, doesn't scroll)
    let help_text = vec![Line::from(
        " type to filter • ↑/k ↓/j move • Enter select • Backspace edit filter • ESC/q cancel ",
    )
    .style(Style::default().fg(Color::Gray))];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(help, chunks[4]);
}
