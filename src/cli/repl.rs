//! REPL implementation
//!
//! This module implements the interactive Read-Eval-Print Loop for Schema-Forge.

use crate::cli::commands::{self, Command, format_error};
use crate::error::Result;
use rustyline::error::ReadlineError;
use rustyline::{Cmd, CompletionType, Config, Editor};
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

                    // Add to history
                    self.editor.add_history_entry(line);

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

    /// Print welcome message
    fn print_welcome(&self) {
        println!();
        println!("Schema-Forge v{}", env!("CARGO_PKG_VERSION"));
        println!("Intelligent Database Query Agent");
        println!();
        println!("Type /help for available commands, or /quit to exit.");
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
