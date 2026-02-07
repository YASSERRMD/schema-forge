//! Database connection abstraction
//!
//! This module provides the database backend enum and connection pooling logic
//! to support multiple database types (PostgreSQL, MySQL, SQLite, MSSQL).

use crate::error::{Result, SchemaForgeError};
use sqlx::{any::AnyPoolOptions, AnyPool, Pool, Postgres, MySql, Sqlite, Any};
use std::str::FromStr;

// MSSQL support via tiberius will be added in Phase 2.3
// use tiberius::Client;
// use tokio::net::TcpStream;
// use tokio_util::compat::{TokioAsyncWriteCompatExt, Compat};

/// Supported database backends
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackend {
    /// PostgreSQL
    PostgreSQL,
    /// MySQL/MariaDB
    MySQL,
    /// SQLite
    SQLite,
    /// Microsoft SQL Server
    MSSQL,
}

impl DatabaseBackend {
    /// Parse database URL to determine backend
    pub fn from_url(url: &str) -> Result<Self> {
        let url_lower = url.to_lowercase();

        if url_lower.starts_with("postgres://") || url_lower.starts_with("postgresql://") {
            Ok(DatabaseBackend::PostgreSQL)
        } else if url_lower.starts_with("mysql://") || url_lower.starts_with("mariadb://") {
            Ok(DatabaseBackend::MySQL)
        } else if url_lower.starts_with("sqlite://") || url_lower.ends_with(".db") || url_lower.ends_with(".sqlite") || url_lower.ends_with(".sqlite3") {
            Ok(DatabaseBackend::SQLite)
        } else if url_lower.starts_with("mssql://") || url_lower.starts_with("sqlserver://") {
            Ok(DatabaseBackend::MSSQL)
        } else {
            Err(SchemaForgeError::InvalidDatabaseUrl(format!(
                "Unable to determine database type from URL: {}",
                url
            )))
        }
    }

    /// Get the default port for this database
    pub fn default_port(&self) -> u16 {
        match self {
            DatabaseBackend::PostgreSQL => 5432,
            DatabaseBackend::MySQL => 3306,
            DatabaseBackend::SQLite => 0, // No port for file-based DB
            DatabaseBackend::MSSQL => 1433,
        }
    }

    /// Get the name of this database backend
    pub fn name(&self) -> &str {
        match self {
            DatabaseBackend::PostgreSQL => "PostgreSQL",
            DatabaseBackend::MySQL => "MySQL",
            DatabaseBackend::SQLite => "SQLite",
            DatabaseBackend::MSSQL => "Microsoft SQL Server",
        }
    }

    /// Check if this backend supports information_schema
    pub fn supports_information_schema(&self) -> bool {
        matches!(
            self,
            DatabaseBackend::PostgreSQL | DatabaseBackend::MySQL | DatabaseBackend::MSSQL
        )
    }

    /// Get the default schema name for this backend
    pub fn default_schema(&self) -> Option<&str> {
        match self {
            DatabaseBackend::PostgreSQL => Some("public"),
            DatabaseBackend::MySQL => Some(database_name_default_schema()),
            DatabaseBackend::SQLite => Some("main"),
            DatabaseBackend::MSSQL => Some("dbo"),
        }
    }
}

// Helper function for MySQL default schema
const fn database_name_default_schema() -> &'static str {
    // MySQL typically uses the database name as the schema
    // This will be replaced at runtime with the actual database name
    ""
}

impl FromStr for DatabaseBackend {
    type Err = SchemaForgeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "postgresql" | "postgres" | "pg" => Ok(DatabaseBackend::PostgreSQL),
            "mysql" | "mariadb" => Ok(DatabaseBackend::MySQL),
            "sqlite" | "sqlite3" => Ok(DatabaseBackend::SQLite),
            "mssql" | "sqlserver" | "microsoft sql server" => Ok(DatabaseBackend::MSSQL),
            _ => Err(SchemaForgeError::UnsupportedDatabaseType(s.to_string())),
        }
    }
}

impl std::fmt::Display for DatabaseBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Database connection pool wrapper
///
/// This enum abstracts over different database pool types to provide
/// a unified interface.
#[derive(Clone)]
pub enum DatabasePool {
    /// Any database pool (sqlx::Any)
    Any(AnyPool),
    // Future: Add specific pool types if needed for type-specific operations
    // PostgreSQL(Pool<Postgres>),
    // MySQL(Pool<MySql>),
    // SQLite(Pool<Sqlite>),
}

impl DatabasePool {
    /// Create a new database pool from connection URL
    pub async fn from_url(url: &str) -> Result<Self> {
        let backend = DatabaseBackend::from_url(url)?;

        match backend {
            DatabaseBackend::PostgreSQL | DatabaseBackend::MySQL | DatabaseBackend::SQLite => {
                // Use sqlx::Any for these databases
                let pool = AnyPool::connect(url).await.map_err(|e| {
                    SchemaForgeError::db_connection(url.to_string(), e)
                })?;
                Ok(DatabasePool::Any(pool))
            }
            DatabaseBackend::MSSQL => {
                // MSSQL uses tiberius directly
                // For now, we'll use sqlx::Any with a converted URL
                // Note: Full MSSQL support via tiberius will be added in Phase 2.3
                let mssql_url = convert_mssql_url_for_sqlx(url)?;
                let pool = AnyPool::connect(&mssql_url).await.map_err(|e| {
                    SchemaForgeError::db_connection(url.to_string(), e)
                })?;
                Ok(DatabasePool::Any(pool))
            }
        }
    }

    /// Get the underlying AnyPool
    pub fn as_any(&self) -> Option<&AnyPool> {
        match self {
            DatabasePool::Any(pool) => Some(pool),
        }
    }

    /// Create a new database pool with custom options
    pub async fn from_url_with_options(url: &str, max_connections: u32) -> Result<Self> {
        let backend = DatabaseBackend::from_url(url)?;

        match backend {
            DatabaseBackend::PostgreSQL | DatabaseBackend::MySQL | DatabaseBackend::SQLite => {
                let pool = AnyPoolOptions::new()
                    .max_connections(max_connections)
                    .connect(url)
                    .await
                    .map_err(|e| SchemaForgeError::db_connection(url.to_string(), e))?;
                Ok(DatabasePool::Any(pool))
            }
            DatabaseBackend::MSSQL => {
                let mssql_url = convert_mssql_url_for_sqlx(url)?;
                let pool = AnyPoolOptions::new()
                    .max_connections(max_connections)
                    .connect(&mssql_url)
                    .await
                    .map_err(|e| SchemaForgeError::db_connection(url.to_string(), e))?;
                Ok(DatabasePool::Any(pool))
            }
        }
    }

    /// Test the connection
    pub async fn test_connection(&self) -> Result<()> {
        match self {
            DatabasePool::Any(pool) => {
                sqlx::query("SELECT 1")
                    .fetch_one(pool)
                    .await
                    .map_err(|e| SchemaForgeError::db_connection("test connection".to_string(), e))?;
                Ok(())
            }
        }
    }
}

/// Convert MSSQL URL format to sqlx-compatible format
///
/// tiberius uses: `mssql://user:pass@host:port/database`
/// sqlx expects: `mssql://user:pass@host:port/database` (similar format)
fn convert_mssql_url_for_sqlx(url: &str) -> Result<String> {
    // For now, return as-is. If sqlx doesn't support MSSQL directly,
    // we'll need to use tiberius's native client
    // This will be implemented in Phase 2.3
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_from_url() {
        assert_eq!(
            DatabaseBackend::from_url("postgresql://localhost/test").unwrap(),
            DatabaseBackend::PostgreSQL
        );
        assert_eq!(
            DatabaseBackend::from_url("postgres://localhost/test").unwrap(),
            DatabaseBackend::PostgreSQL
        );
        assert_eq!(
            DatabaseBackend::from_url("mysql://localhost/test").unwrap(),
            DatabaseBackend::MySQL
        );
        assert_eq!(
            DatabaseBackend::from_url("sqlite://test.db").unwrap(),
            DatabaseBackend::SQLite
        );
        assert_eq!(
            DatabaseBackend::from_url("test.db").unwrap(),
            DatabaseBackend::SQLite
        );
    }

    #[test]
    fn test_backend_default_port() {
        assert_eq!(DatabaseBackend::PostgreSQL.default_port(), 5432);
        assert_eq!(DatabaseBackend::MySQL.default_port(), 3306);
        assert_eq!(DatabaseBackend::SQLite.default_port(), 0);
        assert_eq!(DatabaseBackend::MSSQL.default_port(), 1433);
    }

    #[test]
    fn test_backend_from_str() {
        assert_eq!(
            "postgres".parse::<DatabaseBackend>().unwrap(),
            DatabaseBackend::PostgreSQL
        );
        assert_eq!(
            "mysql".parse::<DatabaseBackend>().unwrap(),
            DatabaseBackend::MySQL
        );
        assert_eq!(
            "sqlite".parse::<DatabaseBackend>().unwrap(),
            DatabaseBackend::SQLite
        );
        assert_eq!(
            "mssql".parse::<DatabaseBackend>().unwrap(),
            DatabaseBackend::MSSQL
        );
    }

    #[test]
    fn test_invalid_url() {
        assert!(DatabaseBackend::from_url("invalid://url").is_err());
    }

    #[test]
    fn test_backend_display() {
        assert_eq!(DatabaseBackend::PostgreSQL.to_string(), "PostgreSQL");
        assert_eq!(DatabaseBackend::MySQL.to_string(), "MySQL");
        assert_eq!(DatabaseBackend::SQLite.to_string(), "SQLite");
        assert_eq!(DatabaseBackend::MSSQL.to_string(), "Microsoft SQL Server");
    }
}
