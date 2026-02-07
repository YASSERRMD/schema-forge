//! Database module
//!
//! This module provides database connection management,
//! schema indexing, and query execution capabilities.

pub mod cache;
pub mod connection;
pub mod indexer;
pub mod manager;
pub mod schema;

// Re-exports
pub use cache::{SchemaCache, CacheStats};
pub use connection::{DatabaseBackend, DatabasePool};
pub use manager::DatabaseManager;
pub use schema::{Column, ColumnType, ForeignKeyReference, SchemaIndex, Table, TableRelationship};
