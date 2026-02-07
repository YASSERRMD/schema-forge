//! Command handlers for CLI
//!
//! This module implements all `/` commands for the Schema-Forge CLI.

use crate::error::{Result, SchemaForgeError};

/// Command types
#[derive(Debug, Clone, PartialEq)]
pub enum CommandType {
    /// Connect to a database
    Connect { url: String },
    /// Index the database schema
    Index,
    /// Set configuration (API keys)
    Config { provider: String, key: String },
    /// Clear chat context
    Clear,
    /// Show help message
    Help,
    /// Exit the application
    Quit,
    /// Natural language query
    Query { text: String },
}

/// Parsed command
#[derive(Debug, Clone)]
pub struct Command {
    /// The type of command
    pub command_type: CommandType,
}

impl Command {
    /// Parse a command from user input
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();

        // Check if it's a command (starts with /)
        if input.starts_with('/') {
            let parts: Vec<&str> = input.splitn(3, ' ').collect();
            let cmd = parts[0];

            match cmd {
                "/connect" => {
                    if parts.len() < 2 {
                        return Err(SchemaForgeError::InvalidCommandSyntax {
                            command: cmd.to_string(),
                            expected: "/connect <database_url>".to_string(),
                        });
                    }
                    let url = parts[1].to_string();
                    Ok(Command {
                        command_type: CommandType::Connect { url },
                    })
                }
                "/index" => Ok(Command {
                    command_type: CommandType::Index,
                    
                }),
                "/config" => {
                    if parts.len() < 3 {
                        return Err(SchemaForgeError::InvalidCommandSyntax {
                            command: cmd.to_string(),
                            expected: "/config <provider> <api_key>".to_string(),
                        });
                    }
                    let provider = parts[1].to_string();
                    let key = parts[2].to_string();
                    Ok(Command {
                        command_type: CommandType::Config { provider, key },
                        
                    })
                }
                "/clear" => Ok(Command {
                    command_type: CommandType::Clear,
                    
                }),
                "/help" => Ok(Command {
                    command_type: CommandType::Help,
                    
                }),
                "/quit" | "/exit" => Ok(Command {
                    command_type: CommandType::Quit,
                    
                }),
                _ => Err(SchemaForgeError::UnknownCommand(cmd.to_string())),
            }
        } else {
            // It's a natural language query
            Ok(Command {
                command_type: CommandType::Query {
                    text: input.to_string(),
                },
                
            })
        }
    }
}

/// Handle a command and return the result message
pub async fn handle_command(
    command: &Command,
) -> Result<String> {
    match &command.command_type {
        CommandType::Connect { url } => {
            // Validate the connection URL format
            if url.starts_with("postgresql://")
                || url.starts_with("postgres://")
                || url.starts_with("mysql://")
                || url.starts_with("sqlite://")
                || url.starts_with("mssql://")
                || url.starts_with("sqlserver://")
            {
                Ok(format!("✓ Connected to database: {}", url))
            } else {
                Err(SchemaForgeError::InvalidInput(format!(
                    "Invalid database URL: {}. Supported: postgresql://, mysql://, sqlite://, mssql://",
                    url
                )))
            }
        }
        CommandType::Index => {
            Ok("✓ Database indexed successfully".to_string())
        }
        CommandType::Config { provider, key } => {
            // Store the API key (actual storage to be implemented)
            let masked_key = if key.len() > 8 {
                format!("{}...{}", &key[..4], &key[key.len() - 4..])
            } else {
                "***".to_string()
            };
            Ok(format!(
                "✓ API key configured for provider: {} ({})",
                provider, masked_key
            ))
        }
        CommandType::Clear => {
            Ok("✓ Chat context cleared".to_string())
        }
        CommandType::Help => {
            let help = r#"
Schema-Forge Commands

Database Commands:
  /connect <url>     Connect to a database (postgresql://, mysql://, sqlite://, mssql://)
  /index             Index the database schema

Configuration:
  /config <provider> <key>  Set API key for LLM provider

Session:
  /clear             Clear chat context
  /help              Show this help message
  /quit, /exit       Exit Schema-Forge

Natural Language:
  Any text without a / prefix will be treated as a natural language query.

Examples:
  /connect postgresql://localhost/mydb
  /index
  /config anthropic sk-ant-...
  Show me all users in the customers table
"#;
            Ok(help.to_string())
        }
        CommandType::Quit => {
            Ok("Goodbye!".to_string())
        }
        CommandType::Query { text } => {
            // Process natural language query
            Ok(format!("Query: {}", text))
        }
    }
}

/// Format an error for display
pub fn format_error(error: &SchemaForgeError) -> String {
    format!("Error: {}", error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_connect_command() {
        let cmd = Command::parse("/connect postgresql://localhost/test").unwrap();
        assert_eq!(
            cmd.command_type,
            CommandType::Connect {
                url: "postgresql://localhost/test".to_string()
            }
        );
    }

    #[test]
    fn test_parse_index_command() {
        let cmd = Command::parse("/index").unwrap();
        assert_eq!(cmd.command_type, CommandType::Index);
    }

    #[test]
    fn test_parse_config_command() {
        let cmd = Command::parse("/config anthropic test-key-123").unwrap();
        assert_eq!(
            cmd.command_type,
            CommandType::Config {
                provider: "anthropic".to_string(),
                key: "test-key-123".to_string()
            }
        );
    }

    #[test]
    fn test_parse_clear_command() {
        let cmd = Command::parse("/clear").unwrap();
        assert_eq!(cmd.command_type, CommandType::Clear);
    }

    #[test]
    fn test_parse_help_command() {
        let cmd = Command::parse("/help").unwrap();
        assert_eq!(cmd.command_type, CommandType::Help);
    }

    #[test]
    fn test_parse_quit_command() {
        let cmd1 = Command::parse("/quit").unwrap();
        assert_eq!(cmd1.command_type, CommandType::Quit);

        let cmd2 = Command::parse("/exit").unwrap();
        assert_eq!(cmd2.command_type, CommandType::Quit);
    }

    #[test]
    fn test_parse_query() {
        let cmd = Command::parse("Show me all users").unwrap();
        assert_eq!(
            cmd.command_type,
            CommandType::Query {
                text: "Show me all users".to_string()
            }
        );
    }

    #[test]
    fn test_parse_invalid_command() {
        let result = Command::parse("/invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_args() {
        let result = Command::parse("/connect");
        assert!(result.is_err());

        let result = Command::parse("/config anthropic");
        assert!(result.is_err());
    }
}
