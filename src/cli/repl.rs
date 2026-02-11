//! REPL implementation
//!
//! This module implements the interactive Read-Eval-Print Loop for Schema-Forge.

use crate::cli::commands::{self, format_error, Command};
use crate::config::SharedState;
use crate::error::Result;
use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::Context;
use rustyline::Helper;
use rustyline::{CompletionType, Config, Editor};

/// Schema-Forge command completer
struct SchemaForgeCompleter;

impl Completer for SchemaForgeCompleter {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        _pos: usize,
        _ctx: &Context<'_>,
    ) -> std::result::Result<(usize, Vec<String>), ReadlineError> {
        let commands = vec![
            "/connect",
            "/index",
            "/config",
            "/providers",
            "/use",
            "/model",
            "/clear",
            "/help",
            "/quit",
            "/exit",
        ];

        // If line starts with /, show all commands
        if line.starts_with('/') {
            let matches: Vec<String> = commands
                .into_iter()
                .filter(|cmd| cmd.starts_with(line))
                .map(|s| s.to_string())
                .collect();
            Ok((0, matches))
        } else {
            Ok((0, vec![]))
        }
    }
}

impl Hinter for SchemaForgeCompleter {
    type Hint = String;
}

impl Highlighter for SchemaForgeCompleter {}

impl Validator for SchemaForgeCompleter {}

impl Helper for SchemaForgeCompleter {}

/// Schema-Forge REPL
pub struct Repl {
    /// The rustyline editor
    editor: Editor<SchemaForgeCompleter, DefaultHistory>,
    /// Whether the REPL should continue running
    running: bool,
    /// Shared application state
    state: SharedState,
}

impl Repl {
    /// Create a new REPL instance
    pub fn new(state: SharedState) -> Result<Self> {
        let config = Config::builder()
            .history_ignore_space(true)
            .completion_type(CompletionType::List)
            .auto_add_history(true)
            .build();

        let completer = SchemaForgeCompleter;
        let mut editor = Editor::<SchemaForgeCompleter, DefaultHistory>::with_config(config)
            .map_err(|e| {
                crate::error::SchemaForgeError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to initialize editor: {}", e),
                ))
            })?;

        editor.set_helper(Some(completer));

        // Set history file
        let history_path = dirs::home_dir()
            .map(|p| p.join(".schema-forge").join("history"))
            .unwrap_or_else(|| ".schema-forge-history".into());

        if let Err(e) = editor.load_history(&history_path) {
            // History file doesn't exist or is unreadable, that's fine
            eprintln!("Note: Could not load history: {}", e);
        }

        Ok(Self {
            editor,
            running: true,
            state,
        })
    }

    /// Run the REPL loop
    pub async fn run(&mut self) -> Result<()> {
        self.print_welcome();

        while self.running {
            match self.editor.readline("> ") {
                Ok(line) => {
                    let line = line.trim();

                    if line.is_empty() {
                        continue;
                    }

                    // Check if user typed just "/" - show command menu
                    if line == "/" {
                        match crate::cli::command_menu::show_command_menu() {
                            Ok(crate::cli::command_menu::MenuResult::Command(cmd)) => {
                                // User selected a command from menu.
                                let needs_args = matches!(
                                    cmd.as_str(),
                                    "/connect" | "/config" | "/use" | "/model"
                                );
                                let initial_input =
                                    if needs_args { format!("{} ", cmd) } else { cmd };

                                match self
                                    .editor
                                    .readline_with_initial("> ", (&initial_input, ""))
                                {
                                    Ok(input) => {
                                        let input = input.trim();
                                        if input.is_empty() {
                                            continue;
                                        }

                                        let _ = self.editor.add_history_entry(input);
                                        match Command::parse(input) {
                                            Ok(command) => {
                                                self.handle_command(command).await;
                                            }
                                            Err(e) => {
                                                println!("{}", format_error(&e));
                                            }
                                        }
                                    }
                                    Err(ReadlineError::Interrupted) => {
                                        println!("^C");
                                        continue;
                                    }
                                    Err(ReadlineError::Eof) => {
                                        println!();
                                        self.running = false;
                                    }
                                    Err(err) => {
                                        println!("Error: {:?}", err);
                                        self.running = false;
                                    }
                                }
                            }
                            Ok(crate::cli::command_menu::MenuResult::Cancelled) => {
                                // User cancelled, show prompt again
                                println!();
                                continue;
                            }
                            Ok(crate::cli::command_menu::MenuResult::TextInput) => {
                                // User wants to type, prefill with command prefix.
                                match self.editor.readline_with_initial("> ", ("/", "")) {
                                    Ok(input) => {
                                        let input = input.trim();
                                        if !input.is_empty() {
                                            let _ = self.editor.add_history_entry(input);
                                            match Command::parse(input) {
                                                Ok(command) => {
                                                    self.handle_command(command).await;
                                                }
                                                Err(e) => {
                                                    println!("{}", format_error(&e));
                                                }
                                            }
                                        }
                                    }
                                    Err(ReadlineError::Interrupted) => {
                                        println!("^C");
                                        continue;
                                    }
                                    Err(ReadlineError::Eof) => {
                                        println!();
                                        self.running = false;
                                    }
                                    Err(err) => {
                                        println!("Error: {:?}", err);
                                        self.running = false;
                                    }
                                }
                            }
                            Err(e) => {
                                println!("Error showing menu: {}", e);
                                continue;
                            }
                        }
                        continue;
                    }

                    // Add to history (ignore result as history failure is non-critical)
                    let _ = self.editor.add_history_entry(line);

                    // Parse and handle command
                    match Command::parse(line) {
                        Ok(command) => {
                            self.handle_command(command).await;
                        }
                        Err(e) => {
                            println!("{}", format_error(&e));
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("^C");
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    println!();
                    self.running = false;
                }
                Err(err) => {
                    println!("Error: {:?}", err);
                    self.running = false;
                }
            }
        }

        Ok(())
    }

    /// Print welcome message with ASCII art banner
    fn print_welcome(&self) {
        println!();
        println!(" ██████╗ ██████╗ ███╗   ██╗████████╗    ██╗  ██╗██╗   ██╗██████╗ ");
        println!(" ██╔════╝██╔═══██╗████╗  ██║╚══██╔══╝    ██║  ██║██║   ██║██╔══██╗");
        println!(" ██║     ██║   ██║██╔██╗ ██║   ██║       ███████║██║   ██║██████╔╝");
        println!(" ██║     ██║   ██║██║╚██╗██║   ██║       ██╔══██║██║   ██║██╔══██╗");
        println!(" ╚██████╗╚██████╔╝██║ ╚████║   ██║       ██║  ██║╚██████╔╝██████╔╝");
        println!("  ╚═════╝ ╚═════╝ ╚═╝  ╚═══╝   ╚═╝       ╚═╝  ╚═╝ ╚═════╝ ╚═════╝ ");
        println!();
        println!(" ███╗   ███╗██╗ ██████╗██████╗  ██████╗ ███████╗");
        println!(" ████╗ ████║██║██╔════╝██╔══██╗██╔═══██╗██╔════╝");
        println!(" ██╔████╔██║██║██║     ██████╔╝██║   ██║█████╗  ");
        println!(" ██║╚██╔╝██║██║██║     ██╔══██╗██║   ██║██╔══╝  ");
        println!(" ██║ ╚═╝ ██║██║╚██████╗██████╔╝╚██████╔╝███████╗");
        println!(" ╚═╝     ╚═╝╚═╝ ╚═════╝╚═════╝  ╚═════╝ ╚══════╝");
        println!();
        println!(
            "Intelligent Database Query Agent v{}",
            env!("CARGO_PKG_VERSION")
        );
        println!();
        println!("Type / for available commands, or /help for more information.");
        println!();
    }

    /// Show all available commands (like Claude Code)
    fn show_all_commands(&self) {
        println!();
        println!("Available Commands:");
        println!();
        println!("Database Commands:");
        println!("  /connect <url>     Connect to a database");
        println!("  /index             Index the database schema");
        println!();
        println!("Configuration:");
        println!("  /config <provider> <key>  Set API key for LLM provider");
        println!("  /providers         List all available LLM providers");
        println!("  /use <provider>    Switch to a different LLM provider");
        println!("  /model <provider> <model>  Set model for a provider");
        println!();
        println!("Session:");
        println!("  /clear             Clear chat context");
        println!("  /help              Show detailed help");
        println!("  /quit, /exit       Exit Schema-Forge");
        println!();
        println!("Direct SQL (type directly):");
        println!("  SELECT * FROM users WHERE active = true");
        println!("  INSERT, UPDATE, DELETE, CREATE, DROP, etc.");
        println!();
        println!("Natural Language:");
        println!("  Show me all users in the customers table");
        println!("  What are the top 10 products by revenue?");
        println!();
        println!("Type /help <command> for more information on a specific command.");
        println!();
    }

    /// Handle a command
    async fn handle_command(&mut self, command: Command) {
        match &command.command_type {
            commands::CommandType::Quit => {
                if let Ok(msg) = commands::handle_command(&command, self.state.clone()).await {
                    println!("{}", msg);
                }
                self.running = false;
            }
            _ => match commands::handle_command(&command, self.state.clone()).await {
                Ok(msg) => {
                    println!("{}", msg);
                }
                Err(e) => {
                    println!("{}", format_error(&e));
                }
            },
        }
    }
}

impl Default for Repl {
    fn default() -> Self {
        Self::new(crate::config::create_shared_state()).expect("Failed to create REPL")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_creation() {
        let state = crate::config::create_shared_state();
        let repl = Repl::new(state);
        assert!(repl.is_ok());
        let repl = repl.unwrap();
        assert!(repl.running);
    }
}
