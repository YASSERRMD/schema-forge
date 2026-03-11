//! Docked slash command menu for the persistent TUI.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
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

pub fn render_command_dock(
    frame: &mut Frame,
    area: Rect,
    commands: &[CommandItem],
    state: &mut ListState,
) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);

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
                .title(" Slash Commands "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, sections[0], state);

    let example = state
        .selected()
        .and_then(|selected| commands.get(selected))
        .map(|command| command.example)
        .unwrap_or("Type to filter commands");
    let footer = Paragraph::new(Line::from(example))
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, sections[1]);
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
