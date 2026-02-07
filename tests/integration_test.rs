//! Integration test for Schema-Forge
//!
//! Tests the actual functionality of connecting to databases and processing queries.

use schema_forge::cli::commands::{Command, CommandType};

#[tokio::test]
async fn test_command_parsing() {
    // Test /connect parsing
    let cmd = Command::parse("/connect sqlite://test.db").unwrap();
    assert!(matches!(cmd.command_type, CommandType::Connect { .. }));

    // Test /index parsing
    let cmd = Command::parse("/index").unwrap();
    assert_eq!(cmd.command_type, CommandType::Index);

    // Test /config parsing
    let cmd = Command::parse("/config anthropic test-key").unwrap();
    assert!(matches!(cmd.command_type, CommandType::Config { .. }));

    // Test natural language query parsing
    let cmd = Command::parse("show all users").unwrap();
    assert!(matches!(cmd.command_type, CommandType::Query { .. }));
}

#[tokio::test]
async fn test_database_components() {
    // Test database backend detection
    use schema_forge::database::connection::DatabaseBackend;

    let postgres_backend = DatabaseBackend::from_url("postgresql://localhost/test").unwrap();
    assert_eq!(postgres_backend, DatabaseBackend::PostgreSQL);

    let mysql_backend = DatabaseBackend::from_url("mysql://localhost/test").unwrap();
    assert_eq!(mysql_backend, DatabaseBackend::MySQL);

    let sqlite_backend = DatabaseBackend::from_url("sqlite://test.db").unwrap();
    assert_eq!(sqlite_backend, DatabaseBackend::SQLite);

    // Test schema data structures
    use schema_forge::database::schema::{Column, ColumnType, SchemaIndex, Table};

    let mut schema = SchemaIndex::new();

    let mut table = Table::new("users");
    table.columns = vec![
        Column {
            name: "id".to_string(),
            column_type: ColumnType {
                base_type: "INTEGER".to_string(),
                length: None,
                scale: None,
                array_dimensions: None,
            },
            nullable: false,
            default_value: None,
            is_primary_key: true,
            is_foreign_key: false,
            references: None,
            is_unique: true,
            comment: None,
        },
        Column {
            name: "name".to_string(),
            column_type: ColumnType {
                base_type: "TEXT".to_string(),
                length: None,
                scale: None,
                array_dimensions: None,
            },
            nullable: false,
            default_value: None,
            is_primary_key: false,
            is_foreign_key: false,
            references: None,
            is_unique: false,
            comment: None,
        },
    ];
    table.primary_keys = vec!["id".to_string()];

    schema.tables.insert("users".to_string(), table);

    // Test LLM formatting
    let context = schema.format_for_llm();
    assert!(context.contains("users"));
    assert!(context.contains("id"));
    assert!(context.contains("name"));

    // Test application state
    use schema_forge::config::create_shared_state;

    let state = create_shared_state();
    {
        let mut state_guard = state.write().await;
        state_guard.set_api_key("anthropic".to_string(), "sk-ant-test".to_string());
        assert_eq!(state_guard.get_current_provider(), Some(&"anthropic".to_string()));
        assert_eq!(state_guard.get_api_key("anthropic"), Some(&"sk-ant-test".to_string()));
    }
}

#[tokio::test]
async fn test_llm_provider_creation() {
    use schema_forge::llm::provider::LLMProvider;

    // Test that we can create providers
    let anthropic = schema_forge::llm::providers::anthropic::AnthropicProvider::new(
        "test-key",
        None,
    );
    assert_eq!(anthropic.provider_name(), "Anthropic");
    assert!(anthropic.has_api_key());

    let openai = schema_forge::llm::providers::openai::OpenAIProvider::new(
        "test-key",
        None,
    );
    assert_eq!(openai.provider_name(), "OpenAI");
    assert!(openai.has_api_key());
}
