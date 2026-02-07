//! Error types for Schema-Forge
//!
//! This module defines the error types used throughout the application.

use thiserror::Error;

/// Result type alias for Schema-Forge
pub type Result<T> = std::result::Result<T, SchemaForgeError>;

/// Main error type for Schema-Forge
#[derive(Error, Debug)]
pub enum SchemaForgeError {
    /// Database-related errors
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// IO-related errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// HTTP-related errors
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// LLM provider errors
    #[error("LLM provider error: {0}")]
    LLMProvider(String),

    /// Command parsing errors
    #[error("Command parsing error: {0}")]
    CommandParse(String),

    /// Not found errors
    #[error("Not found: {0}")]
    NotFound(String),
}

// TODO: Add more error variants as needed in Phase 2+
