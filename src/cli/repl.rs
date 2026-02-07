//! REPL implementation
//!
//! This module implements the interactive Read-Eval-Print Loop for Schema-Forge.

use crate::cli::commands::{self, Command, format_error};
use crate::error::Result;
use rustyline::error::ReadlineError;
use rustyline::{CompletionType, Config, Editor};
use rustyline::history::DefaultHistory;

/// Schema-Forge REPL
pub struct Repl {
    /// The rustyline editor
    editor: Editor<(), DefaultHistory>,
    /// Whether the REPL should continue running
    running: bool,
}

impl Repl {
    /// Create a new REPL instance
    pub fn new() -> Result<Self> {
        let config = Config::builder()
            .history_ignore_space(true)
            .completion_type(CompletionType::List)
            .auto_add_history(true)
            .build();

        let mut editor = Editor::<(), DefaultHistory>::with_config(config).map_err(|e| {
            crate::error::SchemaForgeError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to initialize editor: {}", e),
            ))
        })?;

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

                    // Add to history (ignore result as history failure is non-critical)
                    let _ = self.editor.add_history_entry(line);

                    // Check for "/" command to show all available commands
                    if line == "/" {
                        self.show_all_commands();
                        continue;
                    }

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
        println!(r#"   ____ _           _                  _       "#);
        println!(r#"  / ___| |__   __ _| | ___ _ __   __ _| |      "#);
        println!(r#" | |   | '_ \ / _` | |/ _ \ '_ \ / _` | |      "#);
        println!(r#" | |___| | | | (_| | |  __/ | | | (_| | |      "#);
        println!(r#"  \____|_| |_|\__,_|_|\___|_| |_|\__,_|_|      "#);
        println!();
        println!("Intelligent Database Query Agent v{}", env!("CARGO_PKG_VERSION"));
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
        println!("  /model <provider> <model>  Set model for a provider");
        println!();
        println!("Session:");
        println!("  /clear             Clear chat context");
        println!("  /help              Show detailed help");
        println!("  /quit, /exit       Exit Schema-Forge");
        println!();
        println!("Type /help <command> for more information on a specific command.");
        println!();
    }

    /// Handle a command
    async fn handle_command(&mut self, command: Command) {
        match &command.command_type {
            commands::CommandType::Quit => {
                if let Ok(msg) = commands::handle_command(&command).await {
                    println!("{}", msg);
                }
                self.running = false;
            }
            _ => {
                match commands::handle_command(&command).await {
                    Ok(msg) => {
                        println!("{}", msg);
                    }
                    Err(e) => {
                        println!("{}", format_error(&e));
                    }
                }
            }
        }
    }
}

impl Default for Repl {
    fn default() -> Self {
        Self::new().expect("Failed to create REPL")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_creation() {
        let repl = Repl::new();
        assert!(repl.is_ok());
        let repl = repl.unwrap();
        assert!(repl.running);
    }
}
