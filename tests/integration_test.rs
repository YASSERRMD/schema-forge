//! Integration test for Schema-Forge
//!
//! Tests the actual functionality of connecting to databases and processing queries.

use schema_forge::cli::commands::{self, Command, CommandType};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

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

    let ollama = schema_forge::llm::providers::ollama::OllamaProvider::new(
        "ollama",
        None,
    );
    assert_eq!(ollama.provider_name(), "Ollama");
    assert!(ollama.has_api_key());
}

#[tokio::test]
async fn test_sqlite_command_flow() {
    use schema_forge::config::create_shared_state;

    let database = TestSqliteDatabase::new("command-flow").await;
    let state = create_shared_state();

    let connect = Command::parse(&format!("/connect {}", database.url)).unwrap();
    let connect_output = commands::handle_command(&connect, state.clone()).await.unwrap();
    assert!(connect_output.contains("Connected to database"));

    let index = Command::parse("/index").unwrap();
    let index_output = commands::handle_command(&index, state.clone()).await.unwrap();
    assert!(index_output.contains("1 tables"));
    assert!(index_output.contains("3 columns"));

    let sql = Command::parse("SELECT name, active FROM users ORDER BY id").unwrap();
    let sql_output = commands::handle_command(&sql, state.clone()).await.unwrap();
    assert!(sql_output.contains("Alice"));
    assert!(sql_output.contains("Bob"));
    assert!(sql_output.contains("Charlie"));
}

#[tokio::test]
async fn test_index_updates_cached_schema() {
    use schema_forge::config::create_shared_state;

    let database = TestSqliteDatabase::new("schema-cache").await;
    let state = create_shared_state();

    let connect = Command::parse(&format!("/connect {}", database.url)).unwrap();
    commands::handle_command(&connect, state.clone()).await.unwrap();

    let index = Command::parse("/index").unwrap();
    commands::handle_command(&index, state.clone()).await.unwrap();

    let state_guard = state.read().await;
    let db_manager = state_guard.database_manager.as_ref().unwrap();
    let schema_index = db_manager.get_schema_index().await;

    assert_eq!(schema_index.table_names(), vec!["users"]);
}

#[tokio::test]
async fn test_greeting_query_returns_conversational_response() {
    use schema_forge::config::create_shared_state;

    let state = create_shared_state();
    let greeting = Command::parse("hi").unwrap();
    let output = commands::handle_command(&greeting, state).await.unwrap();

    assert!(output.contains("Hello."));
    assert!(output.contains("/connect <url>"));
}

#[tokio::test]
async fn test_list_tables_query_uses_sqlite_schema_without_llm() {
    use schema_forge::config::create_shared_state;

    let database = TestSqliteDatabase::new("list-tables").await;
    let state = create_shared_state();

    let connect = Command::parse(&format!("/connect {}", database.url)).unwrap();
    commands::handle_command(&connect, state.clone()).await.unwrap();

    let list_tables = Command::parse("list all tables").unwrap();
    let output = commands::handle_command(&list_tables, state).await.unwrap();

    assert!(output.contains("SQLite schema:"));
    assert!(output.contains("users"));
}

struct TestSqliteDatabase {
    path: PathBuf,
    url: String,
}

impl TestSqliteDatabase {
    async fn new(name: &str) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "schema-forge-{name}-{}-{timestamp}.db",
            std::process::id()
        ));
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }

        let url = format!("sqlite://{}", path.display());
        let pool = sqlx::sqlite::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::from_str(&url)
                .unwrap()
                .create_if_missing(true),
        )
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                active INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO users (name, active) VALUES ('Alice', 1), ('Bob', 0), ('Charlie', 1)")
            .execute(&pool)
            .await
            .unwrap();

        pool.close().await;

        Self { path, url }
    }
}

impl Drop for TestSqliteDatabase {
    fn drop(&mut self) {
        if Path::new(&self.path).exists() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
