//! Schema Cache
//!
//! This module provides caching functionality for schema indexes using SQLite.
//! Caching allows faster startup by avoiding re-indexing on every connection.

use crate::database::schema::SchemaIndex;
use crate::error::{Result, SchemaForgeError};
use sqlx::SqlitePool;
use std::path::PathBuf;

/// Schema cache using SQLite for persistent storage
pub struct SchemaCache {
    pool: SqlitePool,
    cache_dir: PathBuf,
}

impl SchemaCache {
    /// Create or open a schema cache
    ///
    /// # Arguments
    /// * `cache_path` - Path to the cache database file
    pub async fn new(cache_path: PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = cache_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Create connection string
        let connection_string = format!("sqlite:{}", cache_path.display());

        // Create connection pool
        let pool = SqlitePool::connect(&connection_string).await?;

        // Initialize cache schema
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_cache (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                connection_url TEXT NOT NULL UNIQUE,
                database_name TEXT,
                schema_name TEXT,
                schema_data TEXT NOT NULL,
                indexed_at TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_connection_url ON schema_cache(connection_url);
            "#,
        )
        .execute(&pool)
        .await?;

        let cache_dir = cache_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();

        Ok(Self { pool, cache_dir })
    }

    /// Get the default cache directory path
    pub fn default_cache_path() -> Result<PathBuf> {
        let mut cache_dir = dirs::home_dir()
            .ok_or_else(|| SchemaForgeError::Cache("Could not determine home directory".to_string()))?;

        cache_dir.push(".schema-forge");
        cache_dir.push("cache.db");

        Ok(cache_dir)
    }

    /// Create a cache with default path
    pub async fn with_default_path() -> Result<Self> {
        let cache_path = Self::default_cache_path()?;
        Self::new(cache_path).await
    }

    /// Save a schema index to the cache
    ///
    /// # Arguments
    /// * `connection_url` - Database connection URL (used as cache key)
    /// * `schema_index` - The schema index to cache
    pub async fn save(&self, connection_url: &str, schema_index: &SchemaIndex) -> Result<()> {
        // Serialize schema index to JSON
        let schema_json = serde_json::to_string(schema_index)
            .map_err(|e| SchemaForgeError::Serialization(e))?;

        let indexed_at = schema_index.indexed_at.to_rfc3339();

        // Insert or replace cache entry
        sqlx::query(
            r#"
            INSERT INTO schema_cache (connection_url, database_name, schema_name, schema_data, indexed_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT(connection_url) DO UPDATE SET
                database_name = excluded.database_name,
                schema_name = excluded.schema_name,
                schema_data = excluded.schema_data,
                indexed_at = excluded.indexed_at
            "#,
        )
        .bind(connection_url)
        .bind(&schema_index.database_name)
        .bind(&schema_index.schema_name)
        .bind(&schema_json)
        .bind(&indexed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| SchemaForgeError::Cache(format!("Failed to save cache: {}", e)))?;

        Ok(())
    }

    /// Load a schema index from the cache
    ///
    /// # Arguments
    /// * `connection_url` - Database connection URL (cache key)
    ///
    /// # Returns
    /// The cached schema index, or None if not found
    pub async fn load(&self, connection_url: &str) -> Result<Option<SchemaIndex>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT schema_data FROM schema_cache WHERE connection_url = $1 ORDER BY indexed_at DESC LIMIT 1",
        )
        .bind(connection_url)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SchemaForgeError::Cache(format!("Failed to load cache: {}", e)))?;

        if let Some((schema_json,)) = row {
            let schema_index: SchemaIndex = serde_json::from_str(&schema_json)
                .map_err(|e| SchemaForgeError::Serialization(e))?;
            Ok(Some(schema_index))
        } else {
            Ok(None)
        }
    }

    /// Check if a cached entry exists for the given connection URL
    pub async fn exists(&self, connection_url: &str) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) as count FROM schema_cache WHERE connection_url = $1")
            .bind(connection_url)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| SchemaForgeError::Cache(format!("Failed to check cache: {}", e)))?;

        Ok(row.map(|(count,)| count > 0).unwrap_or(false))
    }

    /// Clear all cache entries
    pub async fn clear(&self) -> Result<()> {
        sqlx::query("DELETE FROM schema_cache")
            .execute(&self.pool)
            .await
            .map_err(|e| SchemaForgeError::Cache(format!("Failed to clear cache: {}", e)))?;

        Ok(())
    }

    /// Remove a specific cache entry
    pub async fn remove(&self, connection_url: &str) -> Result<()> {
        sqlx::query("DELETE FROM schema_cache WHERE connection_url = $1")
            .bind(connection_url)
            .execute(&self.pool)
            .await
            .map_err(|e| SchemaForgeError::Cache(format!("Failed to remove cache entry: {}", e)))?;

        Ok(())
    }

    /// Get cache statistics
    pub async fn stats(&self) -> Result<CacheStats> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT COUNT(*) as count FROM schema_cache")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| SchemaForgeError::Cache(format!("Failed to get cache stats: {}", e)))?;

        let entry_count = row.map(|(count,)| count as usize).unwrap_or(0);

        Ok(CacheStats {
            entry_count,
            cache_path: self.cache_dir.clone(),
        })
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cached schemas
    pub entry_count: usize,
    /// Path to the cache directory
    pub cache_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_cache_path() {
        let path = SchemaCache::default_cache_path();
        assert!(path.is_ok());
        let path = path.unwrap();
        assert!(path.to_string_lossy().contains(".schema-forge"));
    }
}
