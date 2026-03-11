//! Inline slash command palette for the persistent TUI.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandItem {
    pub name: &'static str,
    pub description: &'static str,
    pub example: &'static str,
    pub requires_arguments: bool,
}

pub fn command_items() -> Vec<CommandItem> {
    vec![
        CommandItem {
            name: "/connect",
            description: "Connect to a database",
            example: "/connect sqlite://demo.db",
            requires_arguments: true,
        },
        CommandItem {
            name: "/index",
            description: "Index the database schema",
            example: "/index",
            requires_arguments: false,
        },
        CommandItem {
            name: "/config",
            description: "Configure a hosted LLM or local Ollama",
            example: "/config ollama",
            requires_arguments: true,
        },
        CommandItem {
            name: "/providers",
            description: "List configured and available providers",
            example: "/providers",
            requires_arguments: false,
        },
        CommandItem {
            name: "/use",
            description: "Switch to a configured provider",
            example: "/use groq",
            requires_arguments: true,
        },
        CommandItem {
            name: "/model",
            description: "Set the model for a provider",
            example: "/model openai gpt-4o",
            requires_arguments: true,
        },
        CommandItem {
            name: "/clear",
            description: "Clear the current transcript",
            example: "/clear",
            requires_arguments: false,
        },
        CommandItem {
            name: "/help",
            description: "Show command help",
            example: "/help",
            requires_arguments: false,
        },
        CommandItem {
            name: "/quit",
            description: "Exit Schema-Forge",
            example: "/quit",
            requires_arguments: false,
        },
    ]
}

pub fn filtered_commands(input: &str) -> Vec<CommandItem> {
    let trimmed = input.trim();
    let needle = trimmed.strip_prefix('/').unwrap_or(trimmed).to_lowercase();

    command_items()
        .into_iter()
        .filter(|command| {
            needle.is_empty()
                || command
                    .name
                    .trim_start_matches('/')
                    .starts_with(needle.as_str())
                || command.description.to_lowercase().contains(needle.as_str())
        })
        .collect()
}

pub fn apply_command(command: &CommandItem) -> String {
    if command.requires_arguments {
        format!("{} ", command.name)
    } else {
        command.name.to_string()
    }
}

pub fn render_command_palette(
    frame: &mut Frame,
    area: Rect,
    commands: &[CommandItem],
    state: &mut ListState,
) {
    let popup_area = centered_rect(78, 68, area);
    frame.render_widget(Clear, popup_area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
        ])
        .split(popup_area);

    let header = Paragraph::new(Line::from(vec![
        Span::styled("/", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" Slash Commands"),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    )
    .alignment(Alignment::Center);
    frame.render_widget(header, sections[0]);

    let items = if commands.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No matching commands.",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        commands
            .iter()
            .map(|command| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:<12}", command.name),
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(command.description),
                ]))
            })
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Commands "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, sections[1], state);

    let example = state
        .selected()
        .and_then(|selected| commands.get(selected))
        .map(|command| command.example)
        .unwrap_or("Type to filter commands");
    let footer = Paragraph::new(Line::from(example))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Example "),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, sections[2]);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_root_returns_commands() {
        let commands = filtered_commands("/");
        assert!(!commands.is_empty());
        assert_eq!(commands[0].name, "/connect");
    }

    #[test]
    fn test_filter_matches_prefix() {
        let commands = filtered_commands("/mod");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "/model");
    }

    #[test]
    fn test_apply_command_adds_trailing_space_for_args() {
        let command = filtered_commands("/connect").remove(0);
        assert_eq!(apply_command(&command), "/connect ");
    }
}
