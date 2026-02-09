//! Database Manager
//!
//! This module implements the DatabaseManager struct which handles
//! database connections, schema indexing, and LLM context generation.

use crate::database::connection::{DatabaseBackend, DatabasePool};
use crate::database::schema::SchemaIndex;
use crate::error::Result;
use sqlx::AnyPool;
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
            connection_url: url.to_string(),
        };

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
            connection_url: url.to_string(),
        };

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
    pub fn get_context_for_llm(&self) -> String {
        // Note: This is a synchronous method that reads from the RwLock
        // In async context, we'd use try_read() or block on read()
        // For now, we'll clone the Arc and use a blocking read
        let index = self.schema_index.clone();
        let index_guard = index.blocking_read();
        index_guard.format_for_llm()
    }

    /// Returns a concise schema summary for LLM prompts
    ///
    /// Provides a more compact view focusing on table names and
    /// their relationships, useful when token count is limited.
    pub fn get_summary_context_for_llm(&self) -> String {
        let index = self.schema_index.clone();
        let index_guard = index.blocking_read();
        index_guard.format_summary_for_llm()
    }

    /// Get the current schema index
    ///
    /// Returns a clone of the current schema index
    pub async fn get_schema_index(&self) -> SchemaIndex {
        let index_guard = self.schema_index.read().await;
        index_guard.clone()
    }

    /// Get the database backend type
    pub fn backend(&self) -> DatabaseBackend {
        self.backend
    }

    /// Get the connection pool
    pub fn pool(&self) -> &DatabasePool {
        &self.pool
    }

    /// Get the underlying AnyPool
    pub fn pool_any(&self) -> &AnyPool {
        self.pool.as_any()
    }

    /// Get the connection URL
    pub fn connection_url(&self) -> &str {
        &self.connection_url
    }

    /// Check if the manager is connected to a database
    pub async fn is_connected(&self) -> bool {
        self.pool.test_connection().await.is_ok()
    }

    // Private indexing methods for each database type

    /// Index PostgreSQL database schema
    async fn index_postgresql(&self) -> Result<SchemaIndex> {
        let pool = self.pool_any();
        crate::database::indexer::index_postgresql(pool).await
    }

    /// Index MySQL database schema
    async fn index_mysql(&self) -> Result<SchemaIndex> {
        let pool = self.pool_any();
        crate::database::indexer::index_mysql(pool).await
    }

    /// Index SQLite database schema
    async fn index_sqlite(&self) -> Result<SchemaIndex> {
        let pool = self.pool_any();
        crate::database::indexer::index_sqlite(pool).await
    }

    /// Index MSSQL database schema
    async fn index_mssql(&self) -> Result<SchemaIndex> {
        let pool = self.pool_any();
        crate::database::indexer::index_mssql(pool).await
    }
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
