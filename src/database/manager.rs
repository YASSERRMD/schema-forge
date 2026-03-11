//! Database Manager
//!
//! This module implements the DatabaseManager struct which handles
//! database connections, schema indexing, and LLM context generation.

use crate::database::connection::{DatabaseBackend, DatabasePool};
use crate::database::schema::SchemaIndex;
use crate::error::{Result, SchemaForgeError};
use comfy_table::Table;
use sqlx::Column;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Database Manager
///
/// The main struct for managing database connections and schema information.
/// It handles connection pooling, schema indexing, and provides methods
/// for generating LLM-friendly context from the database schema.
pub struct DatabaseManager {
    /// Database connection pool
    pool: DatabasePool,
    /// Database backend type
    backend: DatabaseBackend,
    /// Schema index (cached database metadata)
    schema_index: Arc<RwLock<SchemaIndex>>,
    /// Detected database version, if available
    database_version: Arc<RwLock<Option<String>>>,
    /// Connection URL (for reconnection if needed)
    connection_url: String,
}

impl DatabaseManager {
    /// Creates a new DatabaseManager and connects to the database
    ///
    /// # Arguments
    /// * `url` - Database connection URL (e.g., "postgresql://localhost/mydb")
    ///
    /// # Returns
    /// A connected DatabaseManager instance
    ///
    /// # Example
    /// ```no_run
    /// use schema_forge::database::manager::DatabaseManager;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let manager = DatabaseManager::connect("postgresql://localhost/mydb").await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect(url: &str) -> Result<Self> {
        let backend = DatabaseBackend::from_url(url)?;
        let pool = DatabasePool::from_url(url).await?;

        // Test the connection
        pool.test_connection().await?;

        let manager = Self {
            pool,
            backend,
            schema_index: Arc::new(RwLock::new(SchemaIndex::new())),
            database_version: Arc::new(RwLock::new(None)),
            connection_url: url.to_string(),
        };
        let _ = manager.refresh_database_version().await;

        Ok(manager)
    }

    /// Creates a new DatabaseManager with custom pool options
    ///
    /// # Arguments
    /// * `url` - Database connection URL
    /// * `max_connections` - Maximum number of connections in the pool
    pub async fn connect_with_options(url: &str, max_connections: u32) -> Result<Self> {
        let backend = DatabaseBackend::from_url(url)?;
        let pool = DatabasePool::from_url_with_options(url, max_connections).await?;

        // Test the connection
        pool.test_connection().await?;

        let manager = Self {
            pool,
            backend,
            schema_index: Arc::new(RwLock::new(SchemaIndex::new())),
            database_version: Arc::new(RwLock::new(None)),
            connection_url: url.to_string(),
        };
        let _ = manager.refresh_database_version().await;

        Ok(manager)
    }

    /// Main indexing function - queries information_schema and builds the schema index
    ///
    /// This method introspects the database by querying system catalogs
    /// and builds a comprehensive SchemaIndex containing all tables, columns,
    /// and their relationships.
    ///
    /// # Returns
    /// The indexed schema information
    pub async fn index_database(&self) -> Result<SchemaIndex> {
        match self.backend {
            DatabaseBackend::PostgreSQL => self.index_postgresql().await,
            DatabaseBackend::MySQL => self.index_mysql().await,
            DatabaseBackend::SQLite => self.index_sqlite().await,
            DatabaseBackend::Oracle => self.index_oracle().await,
            DatabaseBackend::MSSQL => self.index_mssql().await,
        }
    }

    /// Re-scans the database and updates the in-memory schema index
    ///
    /// This is equivalent to calling `index_database()` and updates
    /// the internal cache.
    pub async fn reindex(&self) -> Result<()> {
        let new_index = self.index_database().await?;

        // Update the schema index
        let mut index_guard = self.schema_index.write().await;
        *index_guard = new_index;

        Ok(())
    }

    /// Returns formatted schema context for LLM prompts
    ///
    /// This method provides a comprehensive, structured representation
    /// of the database schema suitable for inclusion in LLM prompts.
    ///
    /// # Returns
    /// A formatted string containing the complete database schema
    pub async fn get_context_for_llm(&self) -> String {
        let index_guard = self.schema_index.read().await;
        index_guard.format_for_llm()
    }

    /// Returns a concise schema summary for LLM prompts
    ///
    /// Provides a more compact view focusing on table names and
    /// their relationships, useful when token count is limited.
    pub async fn get_summary_context_for_llm(&self) -> String {
        let index_guard = self.schema_index.read().await;
        index_guard.format_summary_for_llm()
    }

    /// Get the current schema index
    ///
    /// Returns a clone of the current schema index
    pub async fn get_schema_index(&self) -> SchemaIndex {
        let index_guard = self.schema_index.read().await;
        index_guard.clone()
    }

    /// Get the detected database version, if available
    pub async fn database_version(&self) -> Option<String> {
        let version_guard = self.database_version.read().await;
        version_guard.clone()
    }

    /// Get the database backend type
    pub fn backend(&self) -> DatabaseBackend {
        self.backend
    }

    /// Get the connection pool
    pub fn pool(&self) -> &DatabasePool {
        &self.pool
    }

    /// Get the connection URL
    pub fn connection_url(&self) -> &str {
        &self.connection_url
    }

    /// Check if the manager is connected to a database
    pub async fn is_connected(&self) -> bool {
        self.pool.test_connection().await.is_ok()
    }

    /// Refresh the cached database version information
    pub async fn refresh_database_version(&self) -> Result<Option<String>> {
        let version = self.detect_database_version().await?;
        let mut version_guard = self.database_version.write().await;
        *version_guard = Some(version.clone());
        Ok(Some(version))
    }

    /// Execute a SQL query on the database and return formatted results
    pub async fn execute_query(&self, sql: &str) -> Result<Vec<String>> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let rows = sqlx::query(sql).fetch_all(pool).await
                    .map_err(|e| SchemaForgeError::db_query(sql, e))?;
                Ok(vec![format!("Query executed successfully, {} rows returned", rows.len())])
            }
            DatabasePool::Postgres(pool) => {
                let rows = sqlx::query(sql).fetch_all(pool).await
                    .map_err(|e| SchemaForgeError::db_query(sql, e))?;
                Ok(vec![format!("Query executed successfully, {} rows returned", rows.len())])
            }
            DatabasePool::MySql(pool) => {
                let rows = sqlx::query(sql).fetch_all(pool).await
                    .map_err(|e| SchemaForgeError::db_query(sql, e))?;
                Ok(vec![format!("Query executed successfully, {} rows returned", rows.len())])
            }
            DatabasePool::Oracle(connection) => {
                if oracle_query_returns_rows(sql) {
                    let result = connection
                        .query(sql, &[])
                        .await
                        .map_err(|e| SchemaForgeError::db_query_message(sql, e.to_string()))?;
                    Ok(vec![format!(
                        "Query executed successfully, {} rows returned",
                        result.row_count()
                    )])
                } else {
                    let result = connection
                        .execute(sql, &[])
                        .await
                        .map_err(|e| SchemaForgeError::db_query_message(sql, e.to_string()))?;
                    connection
                        .commit()
                        .await
                        .map_err(|e| SchemaForgeError::db_query_message("COMMIT", e.to_string()))?;
                    Ok(vec![format!(
                        "Query executed successfully, {} rows affected",
                        result.rows_affected
                    )])
                }
            }
        }
    }

    /// Execute a SQL query and return actual results as a formatted table
    pub async fn execute_query_with_results(&self, sql: &str) -> Result<String> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                self.execute_sqlite_with_results(pool, sql).await
            }
            DatabasePool::Postgres(pool) => {
                self.execute_postgres_with_results(pool, sql).await
            }
            DatabasePool::MySql(pool) => {
                self.execute_mysql_with_results(pool, sql).await
            }
            DatabasePool::Oracle(connection) => self.execute_oracle_with_results(connection, sql).await,
        }
    }

    /// Execute SQLite query and format results as table
    async fn execute_sqlite_with_results(&self, pool: &sqlx::SqlitePool, sql: &str) -> Result<String> {
        use sqlx::Row;

        let rows = sqlx::query(sql).fetch_all(pool).await
            .map_err(|e| SchemaForgeError::db_query(sql, e))?;

        if rows.is_empty() {
            return Ok("No results found.".to_string());
        }

        // Get column names from first row
        let mut table = Table::new();
        if let Some(first_row) = rows.first() {
            let columns: Vec<String> = first_row.columns()
                .iter()
                .map(|c| c.name().to_string())
                .collect();
            table.set_header(&columns);
        }

        // Add rows
        for row in &rows {
            let mut row_values = Vec::new();
            for i in 0..row.columns().len() {
                let value: Option<String> = row.try_get(i).ok();
                row_values.push(value.unwrap_or_else(|| "NULL".to_string()));
            }
            table.add_row(row_values);
        }

        Ok(format!("{}", table))
    }

    /// Execute PostgreSQL query and format results as table
    async fn execute_postgres_with_results(&self, pool: &sqlx::PgPool, sql: &str) -> Result<String> {
        use sqlx::Row;

        let rows = sqlx::query(sql).fetch_all(pool).await
            .map_err(|e| SchemaForgeError::db_query(sql, e))?;

        if rows.is_empty() {
            return Ok("No results found.".to_string());
        }

        // Get column names from first row
        let mut table = Table::new();
        if let Some(first_row) = rows.first() {
            let columns: Vec<String> = first_row.columns()
                .iter()
                .map(|c| c.name().to_string())
                .collect();
            table.set_header(&columns);
        }

        // Add rows
        for row in &rows {
            let mut row_values = Vec::new();
            for i in 0..row.columns().len() {
                let value: Option<String> = row.try_get(i).ok();
                row_values.push(value.unwrap_or_else(|| "NULL".to_string()));
            }
            table.add_row(row_values);
        }

        Ok(format!("{}", table))
    }

    /// Execute MySQL query and format results as table
    async fn execute_mysql_with_results(&self, pool: &sqlx::MySqlPool, sql: &str) -> Result<String> {
        use sqlx::Row;

        let rows = sqlx::query(sql).fetch_all(pool).await
            .map_err(|e| SchemaForgeError::db_query(sql, e))?;

        if rows.is_empty() {
            return Ok("No results found.".to_string());
        }

        // Get column names from first row
        let mut table = Table::new();
        if let Some(first_row) = rows.first() {
            let columns: Vec<String> = first_row.columns()
                .iter()
                .map(|c| c.name().to_string())
                .collect();
            table.set_header(&columns);
        }

        // Add rows
        for row in &rows {
            let mut row_values = Vec::new();
            for i in 0..row.columns().len() {
                let value: Option<String> = row.try_get(i).ok();
                row_values.push(value.unwrap_or_else(|| "NULL".to_string()));
            }
            table.add_row(row_values);
        }

        Ok(format!("{}", table))
    }

    /// Execute Oracle query and format results as table
    async fn execute_oracle_with_results(
        &self,
        connection: &oracle_rs::Connection,
        sql: &str,
    ) -> Result<String> {
        if oracle_query_returns_rows(sql) {
            let result = connection
                .query(sql, &[])
                .await
                .map_err(|e| SchemaForgeError::db_query_message(sql, e.to_string()))?;

            if result.rows.is_empty() {
                return Ok("No results found.".to_string());
            }

            let mut table = Table::new();
            let columns: Vec<String> = result.columns.iter().map(|column| column.name.clone()).collect();
            table.set_header(&columns);

            for row in &result.rows {
                let row_values = row
                    .values()
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>();
                table.add_row(row_values);
            }

            Ok(format!("{}", table))
        } else {
            let result = connection
                .execute(sql, &[])
                .await
                .map_err(|e| SchemaForgeError::db_query_message(sql, e.to_string()))?;
            connection
                .commit()
                .await
                .map_err(|e| SchemaForgeError::db_query_message("COMMIT", e.to_string()))?;

            Ok(format!(
                "Query executed successfully, {} rows affected",
                result.rows_affected
            ))
        }
    }

    // Private indexing methods for each database type

    /// Index PostgreSQL database schema
    async fn index_postgresql(&self) -> Result<SchemaIndex> {
        if let DatabasePool::Postgres(pool) = &self.pool {
            crate::database::indexer::index_postgresql(pool).await
        } else {
            Err(SchemaForgeError::InvalidInput(
                "Not connected to PostgreSQL database".to_string()
            ))
        }
    }

    /// Index MySQL database schema
    async fn index_mysql(&self) -> Result<SchemaIndex> {
        if let DatabasePool::MySql(pool) = &self.pool {
            crate::database::indexer::index_mysql(pool).await
        } else {
            Err(SchemaForgeError::InvalidInput(
                "Not connected to MySQL database".to_string()
            ))
        }
    }

    /// Index SQLite database schema
    async fn index_sqlite(&self) -> Result<SchemaIndex> {
        if let DatabasePool::Sqlite(pool) = &self.pool {
            crate::database::indexer::index_sqlite(pool).await
        } else {
            Err(SchemaForgeError::InvalidInput(
                "Not connected to SQLite database".to_string()
            ))
        }
    }

    /// Index Oracle database schema
    async fn index_oracle(&self) -> Result<SchemaIndex> {
        if let DatabasePool::Oracle(connection) = &self.pool {
            crate::database::indexer::index_oracle(connection).await
        } else {
            Err(SchemaForgeError::InvalidInput(
                "Not connected to Oracle database".to_string()
            ))
        }
    }

    /// Index MSSQL database schema
    async fn index_mssql(&self) -> Result<SchemaIndex> {
        Err(SchemaForgeError::UnsupportedDatabaseType(
            "MSSQL support not yet implemented".to_string()
        ))
    }

    async fn detect_database_version(&self) -> Result<String> {
        match &self.pool {
            DatabasePool::Sqlite(pool) => {
                let row: (String,) = sqlx::query_as("SELECT sqlite_version()")
                    .fetch_one(pool)
                    .await?;
                Ok(format!("SQLite {}", row.0))
            }
            DatabasePool::Postgres(pool) => {
                let row: (String,) = sqlx::query_as("SHOW server_version")
                    .fetch_one(pool)
                    .await?;
                Ok(format!("PostgreSQL {}", row.0))
            }
            DatabasePool::MySql(pool) => {
                let row: (String,) = sqlx::query_as("SELECT VERSION()")
                    .fetch_one(pool)
                    .await?;
                Ok(format!("MySQL {}", row.0))
            }
            DatabasePool::Oracle(connection) => {
                let server_info = connection.server_info().await;
                if !server_info.version.trim().is_empty() {
                    Ok(format!("Oracle {}", server_info.version.trim()))
                } else if !server_info.banner.trim().is_empty() {
                    Ok(format!("Oracle {}", server_info.banner.trim()))
                } else {
                    Ok("Oracle".to_string())
                }
            }
        }
    }
}

fn oracle_query_returns_rows(sql: &str) -> bool {
    let upper = sql.trim_start().to_uppercase();
    upper.starts_with("SELECT ") || upper.starts_with("WITH ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_detection() {
        // Test backend detection logic
        let backend = DatabaseBackend::from_url("postgresql://localhost/test").unwrap();
        assert_eq!(backend, DatabaseBackend::PostgreSQL);
    }

    #[test]
    fn test_invalid_url() {
        assert!(DatabaseBackend::from_url("invalid://url").is_err());
    }

    // Note: Full integration tests with actual database connections
    // require proper database setup. These can be run manually
    // or with docker-compose for testing.
    //
    // The core functionality is verified through unit tests in:
    // - database/schema: Schema structure tests
    // - database/connection: Backend detection tests
    // - database/indexer: Indexing logic tests
    // - database/cache: Cache functionality tests
}
