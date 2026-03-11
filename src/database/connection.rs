//! Database connection abstraction
//!
//! This module provides the database backend enum and connection pooling logic
//! to support multiple database types (PostgreSQL, MySQL, SQLite, MSSQL).

use crate::error::{Result, SchemaForgeError};
use oracle_rs::{Config as OracleConfig, Connection as OracleConnection};
use sqlx::{
    mysql::MySqlPool,
    postgres::PgPool,
    sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions},
};
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
    /// Oracle Database
    Oracle,
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
        } else if url_lower.starts_with("sqlite://") || url_lower.starts_with("sqlite:") || url_lower.ends_with(".db") || url_lower.ends_with(".sqlite") || url_lower.ends_with(".sqlite3") {
            Ok(DatabaseBackend::SQLite)
        } else if url_lower.starts_with("oracle://") {
            Ok(DatabaseBackend::Oracle)
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
            DatabaseBackend::Oracle => 1521,
            DatabaseBackend::MSSQL => 1433,
        }
    }

    /// Get the name of this database backend
    pub fn name(&self) -> &str {
        match self {
            DatabaseBackend::PostgreSQL => "PostgreSQL",
            DatabaseBackend::MySQL => "MySQL",
            DatabaseBackend::SQLite => "SQLite",
            DatabaseBackend::Oracle => "Oracle",
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
            DatabaseBackend::Oracle => None,
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

fn sqlite_connection_string(url: &str) -> String {
    if url.starts_with("sqlite://") || url.starts_with("sqlite:") {
        url.to_string()
    } else {
        format!("sqlite://{}", url)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OracleUrlConfig {
    username: String,
    password: String,
    host: String,
    port: u16,
    service_name: String,
}

fn parse_oracle_url(url: &str) -> Result<OracleUrlConfig> {
    let remainder = url.strip_prefix("oracle://").ok_or_else(|| {
        SchemaForgeError::InvalidDatabaseUrl(format!(
            "Oracle URLs must start with oracle://: {}",
            url
        ))
    })?;
    let (credentials, address) = remainder.rsplit_once('@').ok_or_else(|| {
        SchemaForgeError::InvalidDatabaseUrl(format!(
            "Oracle URLs must include credentials and host: {}",
            url
        ))
    })?;
    let (username, password) = credentials.split_once(':').ok_or_else(|| {
        SchemaForgeError::InvalidDatabaseUrl(format!(
            "Oracle URLs must include username and password: {}",
            url
        ))
    })?;
    let (host_port, service_name) = address.split_once('/').ok_or_else(|| {
        SchemaForgeError::InvalidDatabaseUrl(format!(
            "Oracle URLs must include a service name path: {}",
            url
        ))
    })?;

    if service_name.trim().is_empty() {
        return Err(SchemaForgeError::InvalidDatabaseUrl(format!(
            "Oracle service name cannot be empty: {}",
            url
        )));
    }

    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) => {
            let parsed_port = port.parse::<u16>().map_err(|_| {
                SchemaForgeError::InvalidDatabaseUrl(format!(
                    "Invalid Oracle port in URL: {}",
                    url
                ))
            })?;
            (host, parsed_port)
        }
        None => (host_port, DatabaseBackend::Oracle.default_port()),
    };

    if host.trim().is_empty() || username.trim().is_empty() {
        return Err(SchemaForgeError::InvalidDatabaseUrl(format!(
            "Oracle host and username cannot be empty: {}",
            url
        )));
    }

    Ok(OracleUrlConfig {
        username: username.to_string(),
        password: password.to_string(),
        host: host.to_string(),
        port,
        service_name: service_name.to_string(),
    })
}

impl FromStr for DatabaseBackend {
    type Err = SchemaForgeError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "postgresql" | "postgres" | "pg" => Ok(DatabaseBackend::PostgreSQL),
            "mysql" | "mariadb" => Ok(DatabaseBackend::MySQL),
            "sqlite" | "sqlite3" => Ok(DatabaseBackend::SQLite),
            "oracle" => Ok(DatabaseBackend::Oracle),
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
/// This enum holds the actual database pool for the connected backend.
pub enum DatabasePool {
    /// SQLite pool
    Sqlite(SqlitePool),
    /// PostgreSQL pool
    Postgres(PgPool),
    /// MySQL pool
    MySql(MySqlPool),
    /// Oracle connection
    Oracle(OracleConnection),
}

impl DatabasePool {
    /// Get the database backend for this pool
    pub fn backend(&self) -> DatabaseBackend {
        match self {
            DatabasePool::Sqlite(_) => DatabaseBackend::SQLite,
            DatabasePool::Postgres(_) => DatabaseBackend::PostgreSQL,
            DatabasePool::MySql(_) => DatabaseBackend::MySQL,
            DatabasePool::Oracle(_) => DatabaseBackend::Oracle,
        }
    }

    /// Create a new database pool from connection URL
    /// Create a new database pool from connection URL
    pub async fn from_url(url: &str) -> Result<Self> {
        let backend = DatabaseBackend::from_url(url)?;

        match backend {
            DatabaseBackend::SQLite => {
                let options = SqliteConnectOptions::from_str(&sqlite_connection_string(url))
                    .map_err(|e| SchemaForgeError::Config(format!("Invalid SQLite URL '{}': {}", url, e)))?
                    .create_if_missing(true);
                let pool = SqlitePool::connect_with(options).await
                    .map_err(|e| SchemaForgeError::db_connection(url.to_string(), e))?;
                Ok(DatabasePool::Sqlite(pool))
            }
            DatabaseBackend::PostgreSQL => {
                let pool = PgPool::connect(url).await
                    .map_err(|e| SchemaForgeError::db_connection(url.to_string(), e))?;
                Ok(DatabasePool::Postgres(pool))
            }
            DatabaseBackend::MySQL => {
                let pool = MySqlPool::connect(url).await
                    .map_err(|e| SchemaForgeError::db_connection(url.to_string(), e))?;
                Ok(DatabasePool::MySql(pool))
            }
            DatabaseBackend::Oracle => {
                let config = parse_oracle_url(url)?;
                let connection = OracleConnection::connect_with_config(OracleConfig::new(
                    config.host,
                    config.port,
                    config.service_name,
                    config.username,
                    config.password,
                ))
                .await
                .map_err(|e| SchemaForgeError::db_connection_message(url, e.to_string()))?;
                Ok(DatabasePool::Oracle(connection))
            }
            DatabaseBackend::MSSQL => {
                // MSSQL support requires tiberius client - not yet implemented
                Err(SchemaForgeError::UnsupportedDatabaseType(
                    "MSSQL support not yet fully implemented".to_string()
                ))
            }
        }
    }

    /// Create a new database pool with custom options
    pub async fn from_url_with_options(url: &str, max_connections: u32) -> Result<Self> {
        let backend = DatabaseBackend::from_url(url)?;

        match backend {
            DatabaseBackend::SQLite => {
                let options = SqliteConnectOptions::from_str(&sqlite_connection_string(url))
                    .map_err(|e| SchemaForgeError::Config(format!("Invalid SQLite URL '{}': {}", url, e)))?
                    .create_if_missing(true);
                let pool = SqlitePoolOptions::new()
                    .max_connections(max_connections)
                    .connect_with(options)
                    .await
                    .map_err(|e| SchemaForgeError::db_connection(url.to_string(), e))?;
                Ok(DatabasePool::Sqlite(pool))
            }
            DatabaseBackend::PostgreSQL => {
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(max_connections)
                    .connect(url)
                    .await
                    .map_err(|e| SchemaForgeError::db_connection(url.to_string(), e))?;
                Ok(DatabasePool::Postgres(pool))
            }
            DatabaseBackend::MySQL => {
                let pool = sqlx::mysql::MySqlPoolOptions::new()
                    .max_connections(max_connections)
                    .connect(url)
                    .await
                    .map_err(|e| SchemaForgeError::db_connection(url.to_string(), e))?;
                Ok(DatabasePool::MySql(pool))
            }
            DatabaseBackend::Oracle => {
                let _ = max_connections;
                Self::from_url(url).await
            }
            DatabaseBackend::MSSQL => {
                Err(SchemaForgeError::UnsupportedDatabaseType(
                    "MSSQL support not yet fully implemented".to_string()
                ))
            }
        }
    }

    /// Test the connection
    pub async fn test_connection(&self) -> Result<()> {
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query("SELECT 1")
                    .fetch_one(pool)
                    .await
                    .map_err(|e| SchemaForgeError::db_connection("test connection".to_string(), e))?;
                Ok(())
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query("SELECT 1")
                    .fetch_one(pool)
                    .await
                    .map_err(|e| SchemaForgeError::db_connection("test connection".to_string(), e))?;
                Ok(())
            }
            DatabasePool::MySql(pool) => {
                sqlx::query("SELECT 1")
                    .fetch_one(pool)
                    .await
                    .map_err(|e| SchemaForgeError::db_connection("test connection".to_string(), e))?;
                Ok(())
            }
            DatabasePool::Oracle(connection) => {
                connection
                    .query("SELECT 1 FROM DUAL", &[])
                    .await
                    .map_err(|e| {
                        SchemaForgeError::db_connection_message(
                            "test connection",
                            e.to_string(),
                        )
                    })?;
                Ok(())
            }
        }
    }
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
            DatabaseBackend::from_url("oracle://scott:tiger@localhost:1521/FREEPDB1").unwrap(),
            DatabaseBackend::Oracle
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
        assert_eq!(DatabaseBackend::Oracle.default_port(), 1521);
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
            "oracle".parse::<DatabaseBackend>().unwrap(),
            DatabaseBackend::Oracle
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
        assert_eq!(DatabaseBackend::Oracle.to_string(), "Oracle");
        assert_eq!(DatabaseBackend::MSSQL.to_string(), "Microsoft SQL Server");
    }

    #[test]
    fn test_sqlite_connection_string_preserves_absolute_paths() {
        assert_eq!(
            sqlite_connection_string("sqlite:///tmp/schema-forge.db"),
            "sqlite:///tmp/schema-forge.db"
        );
        assert_eq!(sqlite_connection_string("test.db"), "sqlite://test.db");
    }

    #[test]
    fn test_parse_oracle_url() {
        let config = parse_oracle_url("oracle://scott:tiger@localhost:1521/FREEPDB1").unwrap();
        assert_eq!(
            config,
            OracleUrlConfig {
                username: "scott".to_string(),
                password: "tiger".to_string(),
                host: "localhost".to_string(),
                port: 1521,
                service_name: "FREEPDB1".to_string(),
            }
        );
    }
}
