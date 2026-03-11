//! Persistent terminal UI for Schema-Forge.
//!
//! Keeps the logo pinned at the top while command output scrolls in a
//! dedicated transcript area, with a composer fixed to the bottom.

use crate::cli::command_menu;
use crate::cli::commands::{self, Command, CommandType, format_error};
use crate::config::SharedState;
use crate::error::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, ListState, Paragraph, Wrap},
};
use std::{io, time::Duration};

const HEADER_LOGO: [&str; 7] = [
    "      .-.",
    "     ( * )",
    "      `-'",
    "   .--------.",
    " .'  .----.  '.",
    "|   | []  |   |",
    " '-.______.-'",
];

#[derive(Clone, Copy)]
enum TranscriptKind {
    System,
    User,
    Success,
    Error,
}

struct TranscriptEntry {
    kind: TranscriptKind,
    title: &'static str,
    body: String,
}

impl TranscriptEntry {
    fn new(kind: TranscriptKind, title: &'static str, body: impl Into<String>) -> Self {
        Self {
            kind,
            title,
            body: body.into(),
        }
    }

    fn style(&self) -> Style {
        match self.kind {
            TranscriptKind::System => Style::default().fg(Color::Cyan),
            TranscriptKind::User => Style::default().fg(Color::Yellow),
            TranscriptKind::Success => Style::default().fg(Color::Green),
            TranscriptKind::Error => Style::default().fg(Color::Red),
        }
    }
}

#[derive(Default)]
struct StatusSnapshot {
    connected: bool,
    current_provider: Option<String>,
    configured_providers: usize,
}

pub struct TuiApp {
    state: SharedState,
    input: String,
    cursor: usize,
    transcript: Vec<TranscriptEntry>,
    command_state: ListState,
    history: Vec<String>,
    history_index: Option<usize>,
    history_draft: String,
    scroll: u16,
    follow_output: bool,
    should_quit: bool,
    busy: bool,
    status: StatusSnapshot,
}

impl TuiApp {
    pub fn new(state: SharedState) -> Self {
        let transcript = vec![
            TranscriptEntry::new(
                TranscriptKind::System,
                "Schema-Forge",
                format!("Intelligent Database Query Agent v{}", env!("CARGO_PKG_VERSION")),
            ),
            TranscriptEntry::new(
                TranscriptKind::System,
                "Hint",
                "Use slash commands like /connect or /help, or type a natural language query.",
            ),
        ];

        Self {
            state,
            input: String::new(),
            cursor: 0,
            transcript,
            command_state: ListState::default().with_selected(Some(0)),
            history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
            scroll: 0,
            follow_output: true,
            should_quit: false,
            busy: false,
            status: StatusSnapshot::default(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = setup_terminal()?;
        self.refresh_status().await;

        let run_result = self.run_loop(&mut terminal).await;
        let restore_result = restore_terminal(&mut terminal);

        run_result?;
        restore_result?;
        Ok(())
    }

    async fn run_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            if self.should_quit {
                return Ok(());
            }

            if !event::poll(Duration::from_millis(200))? {
                continue;
            }

            let Event::Key(key) = event::read()? else {
                continue;
            };

            let should_submit = self.handle_key_event(key);
            if should_submit {
                self.busy = true;
                terminal.draw(|frame| self.render(frame))?;
                self.submit_input().await?;
                self.busy = false;
                self.refresh_status().await;
            }
        }
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
                false
            }
            KeyCode::Enter => {
                if self.should_show_command_palette() {
                    self.apply_selected_command()
                } else {
                    !self.input.trim().is_empty()
                }
            }
            KeyCode::Esc => {
                self.clear_input();
                false
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.clear_input();
                false
            }
            KeyCode::Up => {
                if self.should_show_command_palette() {
                    self.select_previous_command();
                } else {
                    self.history_previous();
                }
                false
            }
            KeyCode::Down => {
                if self.should_show_command_palette() {
                    self.select_next_command();
                } else {
                    self.history_next();
                }
                false
            }
            KeyCode::Tab => {
                if self.should_show_command_palette() {
                    let _ = self.accept_selected_command(false);
                }
                false
            }
            KeyCode::Char(ch) => {
                self.insert_char(ch);
                false
            }
            KeyCode::Backspace => {
                self.delete_previous_char();
                false
            }
            KeyCode::Delete => {
                self.delete_current_char();
                false
            }
            KeyCode::Left => {
                self.move_cursor_left();
                false
            }
            KeyCode::Right => {
                self.move_cursor_right();
                false
            }
            KeyCode::Home => {
                self.cursor = 0;
                false
            }
            KeyCode::End => {
                self.cursor = self.input.len();
                false
            }
            KeyCode::PageUp => {
                self.follow_output = false;
                self.scroll = self.scroll.saturating_sub(4);
                false
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(4);
                false
            }
            _ => false,
        }
    }

    async fn submit_input(&mut self) -> Result<()> {
        let submitted = self.input.trim().to_string();
        self.clear_input();

        if submitted.is_empty() {
            return Ok(());
        }

        self.record_history(&submitted);

        if submitted == "/clear" {
            self.transcript.clear();
            self.push_entry(
                TranscriptKind::System,
                "Schema-Forge",
                "Transcript cleared. The header stays pinned while new output scrolls below it.",
            );
            return Ok(());
        }

        self.push_entry(TranscriptKind::User, "You", submitted.clone());

        match Command::parse(&submitted) {
            Ok(command) => {
                let is_quit = matches!(command.command_type, CommandType::Quit);

                match commands::handle_command(&command, self.state.clone()).await {
                    Ok(message) => {
                        self.push_entry(TranscriptKind::Success, "Schema-Forge", message);
                        if is_quit {
                            self.should_quit = true;
                        }
                    }
                    Err(error) => {
                        self.push_entry(TranscriptKind::Error, "Error", format_error(&error));
                    }
                }
            }
            Err(error) => {
                self.push_entry(TranscriptKind::Error, "Error", format_error(&error));
            }
        }

        Ok(())
    }

    async fn refresh_status(&mut self) {
        let state = self.state.read().await;
        self.status.connected = state.is_connected();
        self.status.current_provider = state.get_current_provider().cloned();
        self.status.configured_providers = state.list_providers().len();
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10),
                Constraint::Min(8),
                Constraint::Length(4),
            ])
            .split(area);

        self.render_header(frame, sections[0]);
        self.render_transcript(frame, sections[1]);
        self.render_input(frame, sections[2]);

        if self.should_show_command_palette() {
            let commands = command_menu::filtered_commands(&self.input);
            self.sync_command_selection(commands.len());
            command_menu::render_command_palette(
                frame,
                sections[1],
                &commands,
                &mut self.command_state,
            );
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let status_provider = self
            .status
            .current_provider
            .as_deref()
            .unwrap_or("none configured");
        let status_database = if self.status.connected {
            "database connected"
        } else {
            "database not connected"
        };

        let header_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Schema-Forge ");
        frame.render_widget(header_block, area);

        let inner = area.inner(Margin {
            vertical: 1,
            horizontal: 1,
        });
        let sections = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(18), Constraint::Min(24)])
            .split(inner);

        let logo = Paragraph::new(
            HEADER_LOGO
                .iter()
                .map(|line| {
                    Line::from(Span::styled(
                        (*line).to_string(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ))
                })
                .collect::<Vec<_>>(),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: false });
        frame.render_widget(logo, sections[0]);

        let mut info_lines = vec![
            Line::from(Span::styled(
                "Schema-Forge",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "SQL agent for indexed live databases",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("Provider: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    status_provider.to_string(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("State: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    status_database.to_string(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  |  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Providers: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    self.status.configured_providers.to_string(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
            ]),
        ];

        if self.busy {
            info_lines.push(Line::from(Span::styled(
                "Working...",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )));
        }

        let summary = Paragraph::new(info_lines).wrap(Wrap { trim: false });
        frame.render_widget(summary, sections[1]);
    }

    fn render_transcript(&mut self, frame: &mut Frame, area: Rect) {
        let lines = self.transcript_lines();
        let visible_height = area.height.saturating_sub(2) as usize;
        let max_scroll = lines.len().saturating_sub(visible_height) as u16;

        if self.follow_output || self.scroll >= max_scroll {
            self.scroll = max_scroll;
            self.follow_output = true;
        } else {
            self.scroll = self.scroll.min(max_scroll);
        }

        let transcript = Paragraph::new(Text::from(lines))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue))
                    .title(" Activity "),
            )
            .scroll((self.scroll, 0))
            .wrap(Wrap { trim: false });

        frame.render_widget(transcript, area);
    }

    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let prompt = "> ";
        let input_text = if self.input.is_empty() {
            Text::from(Line::from(vec![
                Span::styled(prompt, Style::default().fg(Color::Green)),
                Span::styled(
                    "Type a query or slash command",
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        } else {
            Text::from(Line::from(vec![
                Span::styled(prompt, Style::default().fg(Color::Green)),
                Span::raw(self.input.clone()),
            ]))
        };

        let input = Paragraph::new(input_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green))
                    .title(" Composer ")
                    .title_bottom(Line::from(self.composer_hint())),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(input, area);

        let cursor_area = area.inner(Margin {
            vertical: 1,
            horizontal: 1,
        });
        let cursor_x = cursor_area.x
            + prompt.chars().count() as u16
            + self.input[..self.cursor].chars().count() as u16;
        frame.set_cursor_position((cursor_x, cursor_area.y));
    }

    fn transcript_lines(&self) -> Vec<Line<'static>> {
        if self.transcript.is_empty() {
            return vec![Line::from(Span::styled(
                "No activity yet.",
                Style::default().fg(Color::DarkGray),
            ))];
        }

        let mut lines = Vec::new();
        for entry in &self.transcript {
            lines.push(Line::from(Span::styled(
                format!("{}:", entry.title),
                entry.style().add_modifier(Modifier::BOLD),
            )));

            for line in entry.body.lines() {
                lines.push(Line::from(Span::raw(format!("  {}", line))));
            }

            lines.push(Line::from(""));
        }

        lines
    }

    fn push_entry(&mut self, kind: TranscriptKind, title: &'static str, body: impl Into<String>) {
        self.transcript.push(TranscriptEntry::new(kind, title, body));
        self.follow_output = true;
    }

    fn should_show_command_palette(&self) -> bool {
        let trimmed = self.input.trim_start();
        trimmed.starts_with('/') && !trimmed.contains(' ')
    }

    fn composer_hint(&self) -> &'static str {
        if self.should_show_command_palette() {
            "Enter select  |  Tab insert  |  Up/Down navigate  |  Esc clear"
        } else {
            "Enter submit  |  Up/Down history  |  Ctrl+C quit  |  PgUp/PgDn scroll"
        }
    }

    fn sync_command_selection(&mut self, command_count: usize) {
        if command_count == 0 {
            self.command_state.select(None);
            return;
        }

        let selected = self.command_state.selected().unwrap_or(0).min(command_count - 1);
        self.command_state.select(Some(selected));
    }

    fn apply_selected_command(&mut self) -> bool {
        self.accept_selected_command(true)
    }

    fn accept_selected_command(&mut self, submit_when_complete: bool) -> bool {
        let commands = command_menu::filtered_commands(&self.input);
        let Some(selected) = self.command_state.selected() else {
            return false;
        };
        let Some(command) = commands.get(selected) else {
            return false;
        };

        self.set_input(command_menu::apply_command(command));
        submit_when_complete && !command.requires_arguments
    }

    fn select_previous_command(&mut self) {
        let commands = command_menu::filtered_commands(&self.input);
        if commands.is_empty() {
            self.command_state.select(None);
            return;
        }

        let selected = self.command_state.selected().unwrap_or(0).saturating_sub(1);
        self.command_state.select(Some(selected));
    }

    fn select_next_command(&mut self) {
        let commands = command_menu::filtered_commands(&self.input);
        if commands.is_empty() {
            self.command_state.select(None);
            return;
        }

        let selected = self.command_state.selected().unwrap_or(0);
        let next = (selected + 1).min(commands.len() - 1);
        self.command_state.select(Some(next));
    }

    fn clear_input(&mut self) {
        self.history_index = None;
        self.history_draft.clear();
        self.set_input(String::new());
    }

    fn set_input(&mut self, input: String) {
        self.input = input;
        self.cursor = self.input.len();
        let command_count = command_menu::filtered_commands(&self.input).len();
        self.sync_command_selection(command_count);
    }

    fn history_previous(&mut self) {
        if self.history.is_empty() {
            return;
        }

        if self.history_index.is_none() {
            self.history_draft = self.input.clone();
        }

        let next_index = match self.history_index {
            Some(index) => index.saturating_sub(1),
            None => self.history.len() - 1,
        };

        self.history_index = Some(next_index);
        self.set_input(self.history[next_index].clone());
    }

    fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };

        if index + 1 < self.history.len() {
            let next_index = index + 1;
            self.history_index = Some(next_index);
            self.set_input(self.history[next_index].clone());
        } else {
            self.history_index = None;
            self.set_input(self.history_draft.clone());
        }
    }

    fn record_history(&mut self, submitted: &str) {
        if self.history.last().map(|entry| entry.as_str()) != Some(submitted) {
            self.history.push(submitted.to_string());
        }
        self.history_index = None;
        self.history_draft.clear();
    }

    fn insert_char(&mut self, ch: char) {
        self.input.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.history_index = None;
        let command_count = command_menu::filtered_commands(&self.input).len();
        self.sync_command_selection(command_count);
    }

    fn delete_previous_char(&mut self) {
        if self.cursor == 0 {
            return;
        }

        if let Some(previous) = self.input[..self.cursor].chars().last() {
            let start = self.cursor.saturating_sub(previous.len_utf8());
            self.input.drain(start..self.cursor);
            self.cursor = start;
        }
        self.history_index = None;
        let command_count = command_menu::filtered_commands(&self.input).len();
        self.sync_command_selection(command_count);
    }

    fn delete_current_char(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }

        if let Some(current) = self.input[self.cursor..].chars().next() {
            let end = self.cursor + current.len_utf8();
            self.input.drain(self.cursor..end);
        }
        self.history_index = None;
        let command_count = command_menu::filtered_commands(&self.input).len();
        self.sync_command_selection(command_count);
    }

    fn move_cursor_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        if let Some(previous) = self.input[..self.cursor].chars().last() {
            self.cursor = self.cursor.saturating_sub(previous.len_utf8());
        }
    }

    fn move_cursor_right(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }

        if let Some(current) = self.input[self.cursor..].chars().next() {
            self.cursor += current.len_utf8();
        }
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
