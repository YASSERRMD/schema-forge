//! Error types for Schema-Forge
//!
//! This module defines comprehensive error types used throughout the application.

use thiserror::Error;

/// Result type alias for Schema-Forge
pub type Result<T> = std::result::Result<T, SchemaForgeError>;

/// Main error type for Schema-Forge
#[derive(Error, Debug)]
pub enum SchemaForgeError {
    /// Database-related errors
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Database connection errors
    #[error("Failed to connect to database: {url}")]
    DatabaseConnection {
        /// Connection URL that failed
        url: String,
        /// Underlying error
        #[source]
        source: sqlx::Error,
    },

    /// Database query errors
    #[error("Database query failed: {query}")]
    DatabaseQuery {
        /// The query that failed
        query: String,
        /// Underlying error
        #[source]
        source: sqlx::Error,
    },

    /// Schema indexing errors
    #[error("Failed to index schema: {0}")]
    SchemaIndexing(String),

    /// Table not found
    #[error("Table '{0}' not found in database")]
    TableNotFound(String),

    /// Column not found
    #[error("Column '{column}' not found in table '{table}'")]
    ColumnNotFound {
        /// Column name
        column: String,
        /// Table name
        table: String,
    },

    /// Invalid database URL
    #[error("Invalid database URL: {0}")]
    InvalidDatabaseUrl(String),

    /// Unsupported database type
    #[error("Unsupported database type: {0}")]
    UnsupportedDatabaseType(String),

    /// IO-related errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// HTTP-related errors
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// HTTP status errors
    #[error("HTTP request failed with status {status}: {url}")]
    HttpStatus {
        /// HTTP status code
        status: u16,
        /// Request URL
        url: String,
        /// Response body
        body: String,
    },

    /// Invalid header value
    #[error("Invalid HTTP header: {0}")]
    InvalidHeader(String),

    /// Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Missing configuration
    #[error("Missing configuration: {0}")]
    MissingConfig(String),

    /// Invalid configuration value
    #[error("Invalid configuration value for '{key}': {value}")]
    InvalidConfig {
        /// Configuration key
        key: String,
        /// Value that was invalid
        value: String,
        /// Expected format description
        expected: String,
    },

    /// LLM provider errors
    #[error("LLM provider error: {provider} - {message}")]
    LLMProvider {
        /// Provider name
        provider: String,
        /// Error message
        message: String,
    },

    /// LLM API key missing
    #[error("API key not found for provider '{0}'. Set it using /config {0} <key>")]
    LLMApiKeyMissing(String),

    /// LLM API error
    #[error("LLM API error ({provider}): {message} (status: {status})")]
    LLMApiError {
        /// Provider name
        provider: String,
        /// Error message
        message: String,
        /// HTTP status code
        status: u16,
    },

    /// LLM rate limit exceeded
    #[error("Rate limit exceeded for provider '{0}'. Try again later.")]
    LLMRateLimitExceeded(String),

    /// Command parsing errors
    #[error("Command parsing error: {0}")]
    CommandParse(String),

    /// Unknown command
    #[error("Unknown command: '{0}'. Type /help for available commands.")]
    UnknownCommand(String),

    /// Invalid command syntax
    #[error("Invalid command syntax for '{command}'. Expected: {expected}")]
    InvalidCommandSyntax {
        /// Command name
        command: String,
        /// Expected syntax
        expected: String,
    },

    /// Not found errors
    #[error("Not found: {0}")]
    NotFound(String),

    /// Connection pool errors
    #[error("Connection pool error: {0}")]
    ConnectionPool(String),

    /// Timeout errors
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// Permission errors
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Authentication errors
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Cache errors
    #[error("Cache error: {0}")]
    Cache(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Anyhow error wrapper
    #[error("Error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

impl SchemaForgeError {
    /// Create a database connection error
    pub fn db_connection(url: impl Into<String>, source: sqlx::Error) -> Self {
        Self::DatabaseConnection {
            url: url.into(),
            source,
        }
    }

    /// Create a database query error
    pub fn db_query(query: impl Into<String>, source: sqlx::Error) -> Self {
        Self::DatabaseQuery {
            query: query.into(),
            source,
        }
    }

    /// Create a configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a missing configuration error
    pub fn missing_config(key: impl Into<String>) -> Self {
        Self::MissingConfig(key.into())
    }

    /// Create an LLM provider error
    pub fn llm_provider(provider: impl Into<String>, message: impl Into<String>) -> Self {
        Self::LLMProvider {
            provider: provider.into(),
            message: message.into(),
        }
    }

    /// Create an invalid command syntax error
    pub fn invalid_syntax(command: impl Into<String>, expected: impl Into<String>) -> Self {
        Self::InvalidCommandSyntax {
            command: command.into(),
            expected: expected.into(),
        }
    }

    /// Create a table not found error
    pub fn table_not_found(table: impl Into<String>) -> Self {
        Self::TableNotFound(table.into())
    }

    /// Create a column not found error
    pub fn column_not_found(column: impl Into<String>, table: impl Into<String>) -> Self {
        Self::ColumnNotFound {
            column: column.into(),
            table: table.into(),
        }
    }

    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ConnectionPool(_)
                | Self::Timeout(_)
                | Self::Http(_)
                | Self::LLMApiError { .. }
                | Self::LLMRateLimitExceeded(_)
        )
    }

    /// Check if error should be shown to user (vs internal errors)
    pub fn is_user_facing(&self) -> bool {
        matches!(
            self,
            Self::InvalidCommandSyntax { .. }
                | Self::UnknownCommand(_)
                | Self::TableNotFound(_)
                | Self::ColumnNotFound { .. }
                | Self::MissingConfig(_)
                | Self::LLMApiKeyMissing(_)
                | Self::LLMRateLimitExceeded(_)
                | Self::InvalidInput(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SchemaForgeError::TableNotFound("users".to_string());
        assert_eq!(err.to_string(), "Table 'users' not found in database");
    }

    #[test]
    fn test_column_not_found() {
        let err = SchemaForgeError::column_not_found("email", "users");
        assert!(matches!(err, SchemaForgeError::ColumnNotFound { .. }));
    }

    #[test]
    fn test_is_retryable() {
        let timeout_err = SchemaForgeError::Timeout("test".to_string());
        assert!(timeout_err.is_retryable());

        let table_err = SchemaForgeError::TableNotFound("users".to_string());
        assert!(!table_err.is_retryable());
    }

    #[test]
    fn test_is_user_facing() {
        let cmd_err = SchemaForgeError::UnknownCommand("test".to_string());
        assert!(cmd_err.is_user_facing());

        let io_err = SchemaForgeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "test",
        ));
        assert!(!io_err.is_user_facing());
    }

    #[test]
    fn test_helper_methods() {
        let err = SchemaForgeError::config("test message");
        assert_eq!(err.to_string(), "Configuration error: test message");

        let err = SchemaForgeError::missing_config("api_key");
        assert_eq!(err.to_string(), "Missing configuration: api_key");
    }
}
