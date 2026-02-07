//! Database module
//!
//! This module provides database connection management,
//! schema indexing, and query execution capabilities.

pub mod connection;
pub mod indexer;
pub mod manager;
pub mod schema;

// Re-exports
pub use connection::{DatabaseBackend, DatabasePool};
pub use schema::{Column, ColumnType, ForeignKeyReference, SchemaIndex, Table, TableRelationship};
