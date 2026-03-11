//! Persistent terminal UI for Schema-Forge.
//!
//! Keeps a pinned top header, a chat-first transcript, and a fixed composer
//! so the interface behaves like an agent shell instead of a scrolling REPL.

use crate::cli::command_menu;
use crate::cli::commands::{self, Command, CommandType, format_error};
use crate::config::SharedState;
use crate::error::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, ListState, Paragraph, Wrap},
};
use std::{io, time::Duration};

const HEADER_LOGO: [&str; 5] = [
    "    ╭────╮",
    "  ╭─┤ ◉  ├─╮",
    "╭─┴─┤╭──╮├─┴─╮",
    "│   │╰──╯│   │",
    "╰───┴────┴───╯",
];
const TRANSCRIPT_BOTTOM_PADDING: usize = 2;

#[derive(Clone, Copy)]
enum TranscriptKind {
    Assistant,
    User,
    System,
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

    fn accent(&self) -> Color {
        match self.kind {
            TranscriptKind::Assistant => Color::Cyan,
            TranscriptKind::User => Color::Green,
            TranscriptKind::System => Color::DarkGray,
            TranscriptKind::Error => Color::Red,
        }
    }
}

#[derive(Default)]
struct StatusSnapshot {
    connected: bool,
    database_backend: Option<String>,
    database_version: Option<String>,
    indexed_tables: usize,
    current_provider: Option<String>,
    current_model: Option<String>,
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
        Self {
            state,
            input: String::new(),
            cursor: 0,
            transcript: Self::welcome_transcript(None),
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

            let should_submit = match event::read()? {
                Event::Key(key) => self.handle_key_event(key),
                Event::Mouse(mouse) => {
                    self.handle_mouse_event(mouse);
                    false
                }
                _ => false,
            };
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
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.scroll_up(2);
                } else if self.should_show_command_palette() {
                    self.select_previous_command();
                } else {
                    self.history_previous();
                }
                false
            }
            KeyCode::Down => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.scroll_down(2);
                } else if self.should_show_command_palette() {
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
            KeyCode::Home if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.jump_to_top();
                false
            }
            KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.jump_to_bottom();
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
                self.scroll_up(4);
                false
            }
            KeyCode::PageDown => {
                self.scroll_down(4);
                false
            }
            _ => false,
        }
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_up(3),
            MouseEventKind::ScrollDown => self.scroll_down(3),
            _ => {}
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
            self.transcript = Self::welcome_transcript(Some(
                "Session reset. The pinned header stays in place and the conversation starts fresh.",
            ));
            self.follow_output = true;
            return Ok(());
        }

        self.push_entry(TranscriptKind::User, "You", submitted.clone());

        match Command::parse(&submitted) {
            Ok(command) => {
                let is_quit = matches!(command.command_type, CommandType::Quit);

                match commands::handle_command(&command, self.state.clone()).await {
                    Ok(message) => {
                        self.push_entry(TranscriptKind::Assistant, "Schema-Forge", message);
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
        self.status.current_model = self
            .status
            .current_provider
            .as_ref()
            .and_then(|provider| state.get_model(provider));
        self.status.configured_providers = state.list_providers().len();

        if let Some(db_manager) = state.database_manager.as_ref() {
            self.status.database_backend = Some(db_manager.backend().to_string());
            self.status.database_version = db_manager.database_version().await;
            self.status.indexed_tables = db_manager.get_schema_index().await.tables.len();
        } else {
            self.status.database_backend = None;
            self.status.database_version = None;
            self.status.indexed_tables = 0;
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let composer_height = if self.should_show_command_palette() { 9 } else { 4 };
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),
                Constraint::Min(10),
                Constraint::Length(composer_height),
            ])
            .split(area);

        self.render_header(frame, sections[0]);
        self.render_body(frame, sections[1]);

        if self.should_show_command_palette() {
            let composer_sections = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(5), Constraint::Min(4)])
                .split(sections[2]);
            let commands = command_menu::filtered_commands(&self.input);
            self.sync_command_selection(commands.len());
            command_menu::render_command_dock(
                frame,
                composer_sections[0],
                &commands,
                &mut self.command_state,
            );
            self.render_input(frame, composer_sections[1]);
        } else {
            self.render_input(frame, sections[2]);
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let header_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Gray))
            .title(" Agent Shell ");
        frame.render_widget(header_block, area);

        let inner = area.inner(Margin {
            vertical: 1,
            horizontal: 1,
        });
        let sections = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(18), Constraint::Min(30)])
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
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
        frame.render_widget(logo, sections[0]);

        let mut detail_lines = vec![
            Line::from(vec![
                Span::styled(
                    "Schema-Forge",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", Style::default()),
                Span::styled(
                    "chat-first SQL agent",
                    Style::default().fg(Color::Gray),
                ),
            ]),
            self.status_pills(),
            Line::from(Span::styled(
                self.header_summary(),
                Style::default().fg(Color::Gray),
            )),
        ];

        if self.busy {
            detail_lines.push(Line::from(Span::styled(
                "Thinking through the next step...",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        let summary = Paragraph::new(detail_lines).wrap(Wrap { trim: false });
        frame.render_widget(summary, sections[1]);
    }

    fn render_body(&mut self, frame: &mut Frame, area: Rect) {
        if area.width >= 110 {
            let sections = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(56), Constraint::Length(32)])
                .split(area);
            self.render_transcript(frame, sections[0]);
            self.render_sidebar(frame, sections[1]);
        } else {
            self.render_transcript(frame, area);
        }
    }

    fn render_transcript(&mut self, frame: &mut Frame, area: Rect) {
        let lines = self.transcript_display_lines();
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
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Conversation "),
            )
            .scroll((self.scroll, 0))
            .wrap(Wrap { trim: false });

        frame.render_widget(transcript, area);
    }

    fn render_sidebar(&self, frame: &mut Frame, area: Rect) {
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(8), Constraint::Min(10)])
            .split(area);

        let context = Paragraph::new(self.context_lines())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Context "),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(context, sections[0]);

        let prompts = Paragraph::new(self.example_lines())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Quick Start "),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(prompts, sections[1]);
    }

    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let prompt = "› ";
        let input_text = if self.input.is_empty() {
            Text::from(Line::from(vec![
                Span::styled(prompt, Style::default().fg(Color::Green)),
                Span::styled(
                    "Ask about data, run SQL, or type / for commands",
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
                    .title(" Ask ")
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
        let mut lines = Vec::new();

        for entry in &self.transcript {
            lines.extend(transcript_entry_lines(entry));
        }

        if self.busy {
            lines.push(Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "Schema-Forge",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    "Working through the request...",
                    Style::default().fg(Color::Gray),
                ),
            ]));
            lines.push(Line::from(""));
        }

        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "No activity yet.",
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines
    }

    fn transcript_display_lines(&self) -> Vec<Line<'static>> {
        let mut lines = self.transcript_lines();
        for _ in 0..TRANSCRIPT_BOTTOM_PADDING {
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
            "Enter send  |  Up/Down history  |  Ctrl+Up/Down scroll  |  PgUp/PgDn or wheel"
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

    fn scroll_up(&mut self, amount: u16) {
        self.follow_output = false;
        self.scroll = self.scroll.saturating_sub(amount);
    }

    fn scroll_down(&mut self, amount: u16) {
        self.follow_output = false;
        self.scroll = self.scroll.saturating_add(amount);
    }

    fn jump_to_top(&mut self) {
        self.follow_output = false;
        self.scroll = 0;
    }

    fn jump_to_bottom(&mut self) {
        self.follow_output = true;
    }

    fn header_summary(&self) -> String {
        if self.status.connected {
            match (
                self.status.database_backend.as_deref(),
                self.status.database_version.as_deref(),
            ) {
                (Some(backend), Some(version)) => format!(
                    "Connected to {backend} ({version}). {} indexed tables cached.",
                    self.status.indexed_tables
                ),
                (Some(backend), None) => format!(
                    "Connected to {backend}. {} indexed tables cached.",
                    self.status.indexed_tables
                ),
                _ => "Connected to a database. Schema indexing runs automatically on connect."
                    .to_string(),
            }
        } else {
            "Connect a database, index the schema immediately, and then ask in plain English."
                .to_string()
        }
    }

    fn status_pills(&self) -> Line<'static> {
        let mut spans = vec![status_pill(
            if self.status.connected {
                "database ready"
            } else {
                "awaiting database"
            },
            if self.status.connected {
                Color::Green
            } else {
                Color::Yellow
            },
        )];

        spans.push(Span::raw(" "));
        spans.push(status_pill(
            self.status
                .database_backend
                .as_deref()
                .unwrap_or("no dialect"),
            Color::Cyan,
        ));

        spans.push(Span::raw(" "));
        spans.push(status_pill(
            self.status
                .current_provider
                .as_deref()
                .unwrap_or("no provider"),
            if self.status.current_provider.is_some() {
                Color::Green
            } else {
                Color::Yellow
            },
        ));

        if let Some(model) = self.status.current_model.as_deref() {
            spans.push(Span::raw(" "));
            spans.push(status_pill(model, Color::White));
        }

        if self.busy {
            spans.push(Span::raw(" "));
            spans.push(status_pill("thinking", Color::Yellow));
        }

        Line::from(spans)
    }

    fn context_lines(&self) -> Vec<Line<'static>> {
        let database = if self.status.connected {
            self.status
                .database_backend
                .clone()
                .unwrap_or_else(|| "connected".to_string())
        } else {
            "disconnected".to_string()
        };
        let version = self
            .status
            .database_version
            .clone()
            .unwrap_or_else(|| "not detected".to_string());
        let provider = self
            .status
            .current_provider
            .clone()
            .unwrap_or_else(|| "not configured".to_string());
        let model = self
            .status
            .current_model
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let index_status = if self.status.connected {
            format!("{} tables cached", self.status.indexed_tables)
        } else {
            "runs on /connect".to_string()
        };

        vec![
            info_line("Database", &database),
            info_line("Version", &version),
            info_line("Provider", &provider),
            info_line("Model", &model),
            info_line("Indexing", &index_status),
            info_line(
                "Providers",
                &self.status.configured_providers.to_string(),
            ),
        ]
    }

    fn example_lines(&self) -> Vec<Line<'static>> {
        if !self.status.connected {
            return vec![
                example_line("/connect sqlite:///Users/.../schema_forge_demo.db"),
                example_line("/connect oracle://user:password@host:1521/SERVICE"),
                example_line("/config ollama"),
                example_line("Ask: show me all tables"),
            ];
        }

        if self.status.current_provider.is_none() {
            return vec![
                example_line("/config ollama"),
                example_line("/model ollama llama3.2"),
                example_line("Ask: list all tables"),
                example_line("Ask: show the newest 10 rows"),
            ];
        }

        vec![
            example_line("Ask: show top customers by revenue"),
            example_line("Ask: which tables store payments"),
            example_line("Ask: find failed orders from today"),
            example_line("/index to refresh the schema cache"),
        ]
    }

    fn welcome_transcript(note: Option<&str>) -> Vec<TranscriptEntry> {
        let mut transcript = vec![TranscriptEntry::new(
            TranscriptKind::Assistant,
            "Schema-Forge",
            "Hello. I work like a database agent: connect a live database, index it immediately, and then ask in plain English or run SQL directly.",
        )];

        if let Some(note) = note {
            transcript.push(TranscriptEntry::new(
                TranscriptKind::System,
                "Session",
                note,
            ));
        }

        transcript.push(TranscriptEntry::new(
            TranscriptKind::System,
            "Quick start",
            "/connect sqlite:///... or /connect oracle://user:password@host:1521/SERVICE_NAME\n/config ollama\nAsk: show me active users or type raw SQL directly",
        ));

        transcript
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct TranscriptBodySections {
    lead: String,
    sql: Option<String>,
    results: Option<String>,
}

fn status_pill(label: &str, color: Color) -> Span<'static> {
    Span::styled(
        format!("[{}]", label),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}

fn info_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<9}"),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(value.to_string(), Style::default().fg(Color::White)),
    ])
}

fn example_line(text: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("• ", Style::default().fg(Color::Cyan)),
        Span::raw(text.to_string()),
    ])
}

fn transcript_entry_lines(entry: &TranscriptEntry) -> Vec<Line<'static>> {
    let accent = entry.accent();
    let body_style = match entry.kind {
        TranscriptKind::Assistant | TranscriptKind::User => Style::default().fg(Color::White),
        TranscriptKind::System => Style::default().fg(Color::Gray),
        TranscriptKind::Error => Style::default().fg(Color::Red),
    };

    let mut lines = vec![Line::from(vec![
        Span::styled(
            "● ",
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            entry.title,
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
    ])];

    let sections = split_transcript_body(&entry.body);
    if !sections.lead.is_empty() {
        for line in sections.lead.lines() {
            lines.push(prefixed_line("│ ", accent, line, body_style));
        }
    }

    if let Some(sql) = sections.sql {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(section_header_line("SQL", Color::Cyan));
        for line in sql.lines() {
            lines.push(prefixed_line("│   ", Color::Cyan, line, Style::default().fg(Color::White)));
        }
    }

    if let Some(results) = sections.results {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(section_header_line("Results", Color::Green));
        for line in results.lines() {
            lines.push(prefixed_line(
                "│   ",
                Color::Green,
                line,
                Style::default().fg(Color::White),
            ));
        }
    }

    lines.push(Line::from(""));
    lines
}

fn prefixed_line(prefix: &str, prefix_color: Color, text: &str, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(prefix.to_string(), Style::default().fg(prefix_color)),
        Span::styled(text.to_string(), style),
    ])
}

fn section_header_line(label: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled("├─ ", Style::default().fg(color)),
        Span::styled(
            label.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn split_transcript_body(body: &str) -> TranscriptBodySections {
    let body = body.trim();
    if body.is_empty() {
        return TranscriptBodySections::default();
    }

    let sql_marker = "SQL:\n";
    let results_marker = "Results:\n";

    if let Some(sql_start) = marker_position(body, sql_marker) {
        let lead = body[..sql_start].trim().to_string();
        let after_sql = &body[sql_start + sql_marker.len()..];
        let (sql, results) = if let Some(results_start) = marker_position(after_sql, results_marker) {
            (
                Some(after_sql[..results_start].trim().to_string()),
                Some(after_sql[results_start + results_marker.len()..].trim().to_string()),
            )
        } else {
            (Some(after_sql.trim().to_string()), None)
        };

        return TranscriptBodySections {
            lead,
            sql: sql.filter(|value| !value.is_empty()),
            results: results.filter(|value| !value.is_empty()),
        };
    }

    if let Some(results_start) = marker_position(body, results_marker) {
        return TranscriptBodySections {
            lead: body[..results_start].trim().to_string(),
            sql: None,
            results: Some(body[results_start + results_marker.len()..].trim().to_string())
                .filter(|value| !value.is_empty()),
        };
    }

    TranscriptBodySections {
        lead: body.to_string(),
        sql: None,
        results: None,
    }
}

fn marker_position(body: &str, marker: &str) -> Option<usize> {
    if body.starts_with(marker) {
        Some(0)
    } else {
        body.find(&format!("\n\n{marker}")).map(|index| index + 2)
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_transcript_body_with_sql_and_results() {
        let body = "Found 2 users.\n\nSQL:\nSELECT * FROM users\n\nResults:\n+----+\n| id |\n+----+";
        let sections = split_transcript_body(body);

        assert_eq!(sections.lead, "Found 2 users.");
        assert_eq!(sections.sql.as_deref(), Some("SELECT * FROM users"));
        assert_eq!(
            sections.results.as_deref(),
            Some("+----+\n| id |\n+----+")
        );
    }

    #[test]
    fn test_split_transcript_body_with_sql_only() {
        let body = "SQL:\nSELECT 1";
        let sections = split_transcript_body(body);

        assert_eq!(sections.lead, "");
        assert_eq!(sections.sql.as_deref(), Some("SELECT 1"));
        assert!(sections.results.is_none());
    }

    #[test]
    fn test_split_transcript_body_plain_text() {
        let sections = split_transcript_body("Hello there.");

        assert_eq!(sections.lead, "Hello there.");
        assert!(sections.sql.is_none());
        assert!(sections.results.is_none());
    }

    #[test]
    fn test_scroll_helpers_update_follow_output() {
        let state = crate::config::create_shared_state();
        let mut app = TuiApp::new(state);

        app.scroll_down(6);
        assert_eq!(app.scroll, 6);
        assert!(!app.follow_output);

        app.scroll_up(2);
        assert_eq!(app.scroll, 4);
        assert!(!app.follow_output);

        app.jump_to_bottom();
        assert!(app.follow_output);
    }

    #[test]
    fn test_transcript_display_lines_add_bottom_padding() {
        let state = crate::config::create_shared_state();
        let app = TuiApp::new(state);

        let display_lines = app.transcript_display_lines();
        let content_lines = app.transcript_lines();

        assert_eq!(
            display_lines.len(),
            content_lines.len() + TRANSCRIPT_BOTTOM_PADDING
        );
        assert_eq!(
            display_lines[display_lines.len() - 1],
            Line::from("")
        );
        assert_eq!(
            display_lines[display_lines.len() - 2],
            Line::from("")
        );
    }
}
