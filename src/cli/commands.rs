//! Command handlers for CLI
//!
//! This module implements all `/` commands for the Schema-Forge CLI.

use crate::config::SharedState;
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
    /// List all available LLM providers
    Providers,
    /// Switch to a different provider
    Use { provider: String },
    /// Set model for a provider
    Model { provider: String, model: String },
    /// Clear chat context
    Clear,
    /// Show help message
    Help,
    /// Exit the application
    Quit,
    /// Direct SQL query execution
    DirectSql { sql: String },
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
                    if parts.len() < 3 && !(parts.len() == 2 && parts[1].eq_ignore_ascii_case("ollama")) {
                        return Err(SchemaForgeError::InvalidCommandSyntax {
                            command: cmd.to_string(),
                            expected: "/config <provider> <api_key> (or /config ollama)".to_string(),
                        });
                    }
                    let provider = parts[1].to_string();
                    let key = if provider.eq_ignore_ascii_case("ollama") && parts.len() < 3 {
                        "ollama".to_string()
                    } else {
                        parts[2].to_string()
                    };
                    Ok(Command {
                        command_type: CommandType::Config { provider, key },
                    })
                }
                "/providers" => Ok(Command {
                    command_type: CommandType::Providers,
                }),
                "/model" => {
                    if parts.len() < 3 {
                        return Err(SchemaForgeError::InvalidCommandSyntax {
                            command: cmd.to_string(),
                            expected: "/model <provider> <model>".to_string(),
                        });
                    }
                    let provider = parts[1].to_string();
                    let model = parts[2].to_string();
                    Ok(Command {
                        command_type: CommandType::Model { provider, model },
                    })
                }
                "/use" => {
                    if parts.len() < 2 {
                        return Err(SchemaForgeError::InvalidCommandSyntax {
                            command: cmd.to_string(),
                            expected: "/use <provider>".to_string(),
                        });
                    }
                    let provider = parts[1].to_string();
                    Ok(Command {
                        command_type: CommandType::Use { provider },
                    })
                }
                "/clear" => Ok(Command {
                    command_type: CommandType::Clear,
                }),
                "/" | "/help" => Ok(Command {
                    command_type: CommandType::Help,
                }),
                "/quit" | "/exit" => Ok(Command {
                    command_type: CommandType::Quit,
                }),
                _ => Err(SchemaForgeError::UnknownCommand(cmd.to_string())),
            }
        } else {
            // Prefer natural language when input starts with common NL "show" forms.
            let lower_input = input.to_lowercase();
            if lower_input.starts_with("show me ") || lower_input.starts_with("show us ") {
                return Ok(Command {
                    command_type: CommandType::Query {
                        text: input.to_string(),
                    },
                });
            }

            // Check if it's a direct SQL query
            let upper_input = input.to_uppercase();
            let sql_keywords = [
                "SELECT",
                "INSERT",
                "UPDATE",
                "DELETE",
                "CREATE",
                "DROP",
                "ALTER",
                "TRUNCATE",
                "DESCRIBE",
                "DESC",
                "EXPLAIN",
                "WITH",
            ];

            let is_sql_query = sql_keywords.iter().any(|keyword| {
                upper_input == *keyword || upper_input.starts_with(&format!("{} ", keyword))
            }) || is_show_statement(&upper_input);

            if is_sql_query {
                // Direct SQL execution
                Ok(Command {
                    command_type: CommandType::DirectSql {
                        sql: input.to_string(),
                    },
                })
            } else {
                // Natural language query
                Ok(Command {
                    command_type: CommandType::Query {
                        text: input.to_string(),
                    },
                })
            }
        }
    }
}

fn is_show_statement(upper_input: &str) -> bool {
    let show_prefixes = [
        "SHOW TABLE",
        "SHOW TABLES",
        "SHOW DATABASE",
        "SHOW DATABASES",
        "SHOW COLUMN",
        "SHOW COLUMNS",
        "SHOW CREATE",
        "SHOW INDEX",
        "SHOW INDEXES",
        "SHOW STATUS",
        "SHOW VARIABLES",
        "SHOW PROCESSLIST",
    ];

    show_prefixes.iter().any(|prefix| {
        upper_input == *prefix || upper_input.starts_with(&format!("{} ", prefix))
    })
}

/// Handle a command and return the result message
pub async fn handle_command(command: &Command, state: SharedState) -> Result<String> {
    match &command.command_type {
        CommandType::Connect { url } => {
            // Validate the connection URL format
            let url_lower = url.to_lowercase();
            if !url_lower.starts_with("postgresql://")
                && !url_lower.starts_with("postgres://")
                && !url_lower.starts_with("mysql://")
                && !url_lower.starts_with("mariadb://")
                && !url_lower.starts_with("sqlite://")
                && !url_lower.starts_with("sqlite:")
                && !url_lower.starts_with("mssql://")
                && !url_lower.starts_with("sqlserver://")
                && !url_lower.ends_with(".db")
                && !url_lower.ends_with(".sqlite")
                && !url_lower.ends_with(".sqlite3")
            {
                return Err(SchemaForgeError::InvalidInput(format!(
                    "Invalid database URL: {}. Supported: postgresql://, mysql://, sqlite://, sqlite:, mssql://",
                    url
                )));
            }

            // Actually connect to the database
            let manager = crate::database::manager::DatabaseManager::connect(url).await?;

            // Store the database manager in state
            let mut state_guard = state.write().await;
            state_guard.set_database_manager(manager);

            Ok(format!("Connected to database: {}", url))
        }
        CommandType::Index => {
            // Check if database is connected
            let state_guard = state.read().await;
            let db_manager = state_guard.database_manager.as_ref().ok_or_else(|| {
                SchemaForgeError::InvalidInput(
                    "Not connected to any database. Use /connect first.".to_string(),
                )
            })?;

            db_manager.reindex().await?;
            let schema_index = db_manager.get_schema_index().await;

            let table_count = schema_index.tables.len();
            let column_count: usize = schema_index.tables.values().map(|t| t.columns.len()).sum();

            Ok(format!(
                "Database indexed successfully: {} tables, {} columns",
                table_count, column_count
            ))
        }
        CommandType::Config { provider, key } => {
            let mut state_guard = state.write().await;
            state_guard.set_api_key(provider.clone(), key.clone());

            if provider.eq_ignore_ascii_case("ollama") {
                let model = state_guard
                    .get_model("ollama")
                    .unwrap_or_else(|| "llama3.2".to_string());
                Ok(format!(
                    "Ollama configured for local server at http://localhost:11434 using model '{}'. Use /model ollama <model> to switch models.",
                    model
                ))
            } else {
                let masked_key = if key.len() > 8 {
                    format!("{}...{}", &key[..4], &key[key.len() - 4..])
                } else {
                    "***".to_string()
                };

                Ok(format!(
                    "API key configured for provider: {} ({})",
                    provider, masked_key
                ))
            }
        }
        CommandType::Providers => {
            let state_guard = state.read().await;
            let configured = state_guard.list_providers();

            if configured.is_empty() {
                let providers = r#"
Available LLM Providers:

Anthropic:
  Default Model: claude-sonnet-4-20250514
  Config: /config anthropic <api-key>

OpenAI:
  Default Model: gpt-4o
  Config: /config openai <api-key>

Ollama:
  Default Model: llama3.2
  Config: /config ollama
  Notes: local server at http://localhost:11434

Groq:
  Default Model: llama-3.3-70b-versatile
  Config: /config groq <api-key>

Cohere:
  Default Model: command-r-plus
  Config: /config cohere <api-key>

xAI:
  Default Model: grok-2
  Config: /config xai <api-key>

Minimax:
  Default Model: abab6.5s-chat
  Config: /config minimax <api-key>

Qwen:
  Default Model: qwen-max
  Config: /config qwen <api-key>

z.ai:
  Default Model: deepseek-r1
  Config: /config z.ai <api-key>

Set a specific model:
  /model <provider> <model-name>
"#;
                Ok(providers.to_string())
            } else {
                let mut output = String::from("Configured Providers:\n\n");

                for provider in &configured {
                    let model = state_guard
                        .get_model(provider)
                        .unwrap_or_else(|| "default".to_string());
                    let current = state_guard
                        .get_current_provider()
                        .map(|p| if p == provider { " (current)" } else { "" })
                        .unwrap_or("");

                    output.push_str(&format!("  {}{}:\n", provider, current));
                    output.push_str(&format!("    Model: {}\n", model));
                    if provider.eq_ignore_ascii_case("ollama") {
                        output.push_str("    Connection: local Ollama at http://localhost:11434\n\n");
                    } else {
                        output.push_str(&format!("    API Key: {}***\n\n",
                            &state_guard.get_api_key(provider).map(|k| &k[..4.min(k.len())]).unwrap_or("")));
                    }
                }

                output.push_str("\nUse /model <provider> <model> to change models\n");
                Ok(output)
            }
        }
        CommandType::Model { provider, model } => {
            // Store the model preference in state
            let mut state_guard = state.write().await;

            // Validate provider exists
            if !state_guard.api_keys.contains_key(provider) {
                return Err(SchemaForgeError::InvalidInput(format!(
                    "Provider '{}' not configured. Use {} first.",
                    provider, config_hint(provider)
                )));
            }

            // Store model preference
            state_guard.set_model(provider.clone(), model.clone());

            Ok(format!(
                "Model '{}' set for provider '{}' (saved)",
                model, provider
            ))
        }
        CommandType::Use { provider } => {
            // Switch to a different provider
            let mut state_guard = state.write().await;

            // Validate provider exists
            if !state_guard.api_keys.contains_key(provider) {
                return Err(SchemaForgeError::InvalidInput(format!(
                    "Provider '{}' not configured. Use {} first.",
                    provider, config_hint(provider)
                )));
            }

            // Switch to this provider
            state_guard.set_current_provider(provider.clone());

            Ok(format!("Switched to provider: {} (saved)", provider))
        }
        CommandType::Clear => {
            // Clear chat context (to be implemented with message history)
            Ok("Chat context cleared".to_string())
        }
        CommandType::Help => {
            let help = r#"
Schema-Forge Commands

Database Commands:
  /connect <url>     Connect to a database (postgresql://, mysql://, sqlite://, mssql://)
  /index             Index the database schema

Configuration:
  /config <provider> <key>  Set API key for a hosted LLM provider
  /config ollama            Use a local Ollama server at http://localhost:11434
  /providers         List all available LLM providers
  /use <provider>    Switch to a different LLM provider
  /model <provider> <model>  Set model for a provider

Session:
  /clear             Clear chat context
  /help              Show this help message
  /quit, /exit       Exit Schema-Forge

Direct SQL:
  SELECT * FROM users WHERE active = true
  INSERT INTO users (name) VALUES ('John')
  Any SQL statement starting with SELECT, INSERT, UPDATE, DELETE, etc.

Natural Language:
  Any text without a / prefix will be treated as a natural language query.
  Show me all users in the customers table

Examples:
  /connect postgresql://localhost/mydb
  /index
  /config anthropic sk-ant-...
  /config ollama
  /model ollama llama3.2
  /providers
  /model openai gpt-4
  SELECT * FROM users LIMIT 10
"#;
            Ok(help.to_string())
        }
        CommandType::Quit => Ok("Goodbye!".to_string()),
        CommandType::DirectSql { sql } => {
            // Direct SQL execution - no LLM needed
            let state_guard = state.read().await;

            // Check if database is connected
            let db_manager = state_guard.database_manager.as_ref().ok_or_else(|| {
                SchemaForgeError::InvalidInput(
                    "Not connected to any database. Use /connect first.".to_string(),
                )
            })?;

            // Execute the SQL query directly and return formatted results
            let results = db_manager.execute_query_with_results(sql).await?;

            Ok(results)
        }
        CommandType::Query { text } => {
            // This is a natural language query - process it using LLM
            let state_guard = state.read().await;

            if is_greeting_query(text) {
                let backend = state_guard
                    .database_manager
                    .as_ref()
                    .map(|db_manager| db_manager.backend());
                return Ok(greeting_response(backend));
            }

            // Check if database is connected
            let db_manager = state_guard.database_manager.as_ref().ok_or_else(|| {
                SchemaForgeError::InvalidInput(
                    "Not connected to any database. Use /connect first.".to_string(),
                )
            })?;

            let schema_index = ensure_schema_index_loaded(db_manager).await?;

            if is_table_list_request(text) {
                return Ok(format_table_list(&schema_index, db_manager.backend()));
            }

            // Check if an LLM provider is configured
            let current_provider = state_guard.get_current_provider()
                .ok_or_else(|| SchemaForgeError::InvalidInput("No LLM provider configured. Use /config ollama for local Ollama or /config <provider> <api-key> for a hosted model.".to_string()))?
                .clone();

            let api_key = state_guard
                .get_api_key(&current_provider)
                .ok_or_else(|| {
                    SchemaForgeError::InvalidInput(format!(
                        "API key not found for provider '{}'",
                        current_provider
                    ))
                })?
                .clone();

            let schema_context = schema_index.format_for_llm();

            // Get configured model for this provider
            let model = state_guard.get_model(&current_provider);

            // Drop the read guard before we make the async LLM call
            drop(state_guard);

            // Create the appropriate LLM provider with configured model
            let provider = create_llm_provider(&current_provider, &api_key, model)?;

            // Generate SQL from natural language
            let sql_query = provider
                .generate_sql(&schema_context, text)
                .await
                .map_err(|e| SchemaForgeError::LLMApiError {
                    provider: current_provider.clone(),
                    message: format!("Failed to generate SQL: {}", e),
                    status: 0,
                })?;

            // Execute the SQL query
            let state_guard = state.read().await;
            let db_manager = state_guard.database_manager.as_ref().unwrap();
            let results = execute_sql_query(db_manager, &sql_query).await?;

            Ok(format!("SQL:\n{}\n\nResults:\n{}", sql_query, results))
        }
    }
}

async fn ensure_schema_index_loaded(
    db_manager: &crate::database::manager::DatabaseManager,
) -> Result<crate::database::schema::SchemaIndex> {
    let schema_index = db_manager.get_schema_index().await;
    if !schema_index.tables.is_empty() {
        return Ok(schema_index);
    }

    db_manager.reindex().await?;
    Ok(db_manager.get_schema_index().await)
}

fn is_greeting_query(text: &str) -> bool {
    matches!(
        normalize_query_text(text).as_str(),
        "hi"
            | "hello"
            | "hey"
            | "greetings"
            | "good morning"
            | "good afternoon"
            | "good evening"
    )
}

fn greeting_response(
    backend: Option<crate::database::connection::DatabaseBackend>,
) -> String {
    match backend {
        Some(backend) => format!(
            "Hello. You're connected to {}. I can list tables, run SQL directly, or use /config ollama for natural-language queries.",
            backend
        ),
        None => "Hello. Connect a database with /connect <url>, then I can list tables, run SQL directly, or use /config ollama for natural-language queries.".to_string(),
    }
}

fn is_table_list_request(text: &str) -> bool {
    matches!(
        normalize_query_text(text).as_str(),
        "list tables"
            | "list all tables"
            | "list the tables"
            | "show tables"
            | "show all tables"
            | "what tables exist"
            | "which tables exist"
            | "what are the tables"
            | "what tables are there"
            | "what tables do i have"
            | "which tables are in the database"
            | "which tables do i have"
    )
}

fn format_table_list(
    schema_index: &crate::database::schema::SchemaIndex,
    backend: crate::database::connection::DatabaseBackend,
) -> String {
    let tables: Vec<String> = schema_index
        .tables_only()
        .into_iter()
        .map(|table| table.name.clone())
        .collect();
    let views: Vec<String> = schema_index
        .views()
        .into_iter()
        .map(|view| view.name.clone())
        .collect();

    if tables.is_empty() && views.is_empty() {
        return format!("No tables found in the connected {} database.", backend);
    }

    let mut response = format!("{} schema:\n", backend);
    if !tables.is_empty() {
        response.push_str("Tables:\n");
        for table in tables {
            response.push_str(&format!("  - {}\n", table));
        }
    }
    if !views.is_empty() {
        response.push_str("Views:\n");
        for view in views {
            response.push_str(&format!("  - {}\n", view));
        }
    }

    response.trim_end().to_string()
}

fn normalize_query_text(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() || ch.is_whitespace() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Format an error for display
pub fn format_error(error: &SchemaForgeError) -> String {
    format!("Error: {}", error)
}

/// Create an LLM provider instance based on provider name and model
fn create_llm_provider(
    provider: &str,
    api_key: &str,
    model: Option<String>,
) -> Result<Box<dyn crate::llm::provider::LLMProvider>> {
    match provider.to_lowercase().as_str() {
        "anthropic" => {
            Ok(Box::new(crate::llm::providers::anthropic::AnthropicProvider::new(
                api_key,
                model,
            )))
        }
        "openai" => {
            Ok(Box::new(crate::llm::providers::openai::OpenAIProvider::new(
                api_key,
                model,
            )))
        }
        "ollama" => {
            Ok(Box::new(crate::llm::providers::ollama::OllamaProvider::new(
                api_key,
                model,
            )))
        }
        "groq" => {
            Ok(Box::new(crate::llm::providers::groq::GroqProvider::new(
                api_key,
                model,
            )))
        }
        "cohere" => {
            Ok(Box::new(crate::llm::providers::cohere::CohereProvider::new(
                api_key,
                model,
            )))
        }
        "xai" => {
            Ok(Box::new(crate::llm::providers::xai::XAIProvider::new(
                api_key,
                model,
            )))
        }
        "minimax" => {
            Ok(Box::new(crate::llm::providers::minimax::MinimaxProvider::new(
                api_key,
                model,
            )))
        }
        "qwen" => {
            Ok(Box::new(crate::llm::providers::qwen::QwenProvider::new(
                api_key,
                model,
            )))
        }
        "z.ai" | "zai" => {
            Ok(Box::new(crate::llm::providers::zai::ZAIProvider::new(
                api_key,
                model,
            )))
        }
        _ => Err(SchemaForgeError::InvalidInput(format!(
            "Unknown provider: '{}'. Supported: anthropic, openai, ollama, groq, cohere, xai, minimax, qwen, z.ai",
            provider
        ))),
    }
}

/// Execute a SQL query and format results
async fn execute_sql_query(
    db_manager: &crate::database::manager::DatabaseManager,
    sql: &str,
) -> Result<String> {
    // Execute the query and return formatted results as a table
    db_manager.execute_query_with_results(sql).await
}

fn config_hint(provider: &str) -> String {
    if provider.eq_ignore_ascii_case("ollama") {
        "/config ollama".to_string()
    } else {
        format!("/config {} <api-key>", provider)
    }
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
    fn test_parse_ollama_config_command_without_key() {
        let cmd = Command::parse("/config ollama").unwrap();
        assert_eq!(
            cmd.command_type,
            CommandType::Config {
                provider: "ollama".to_string(),
                key: "ollama".to_string()
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
    fn test_parse_show_query_without_me() {
        let cmd = Command::parse("show all users").unwrap();
        assert_eq!(
            cmd.command_type,
            CommandType::Query {
                text: "show all users".to_string()
            }
        );
    }

    #[test]
    fn test_parse_show_tables_as_direct_sql() {
        let cmd = Command::parse("SHOW TABLES").unwrap();
        assert_eq!(
            cmd.command_type,
            CommandType::DirectSql {
                sql: "SHOW TABLES".to_string()
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

        let result = Command::parse("/config ollama");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_providers_command() {
        let cmd = Command::parse("/providers").unwrap();
        assert_eq!(cmd.command_type, CommandType::Providers);
    }

    #[test]
    fn test_parse_model_command() {
        let cmd = Command::parse("/model openai gpt-4").unwrap();
        assert_eq!(
            cmd.command_type,
            CommandType::Model {
                provider: "openai".to_string(),
                model: "gpt-4".to_string()
            }
        );
    }

    #[test]
    fn test_parse_model_missing_args() {
        let result = Command::parse("/model openai");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_hint_for_ollama() {
        assert_eq!(config_hint("ollama"), "/config ollama");
        assert_eq!(config_hint("openai"), "/config openai <api-key>");
    }

    #[test]
    fn test_greeting_query_detection() {
        assert!(is_greeting_query("hi"));
        assert!(is_greeting_query("Good evening!"));
        assert!(!is_greeting_query("show me users"));
    }

    #[test]
    fn test_table_list_request_detection() {
        assert!(is_table_list_request("list all tables"));
        assert!(is_table_list_request("Which tables do I have?"));
        assert!(!is_table_list_request("list all users"));
    }
}
