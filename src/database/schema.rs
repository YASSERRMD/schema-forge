//! Schema data structures
//!
//! This module defines the core data structures for representing
//! database schema information, including tables, columns, and their metadata.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Represents the type of a database column
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnType {
    /// The base type (e.g., "varchar", "integer", "timestamp")
    pub base_type: String,
    /// Optional length/precision (e.g., 255 for VARCHAR(255))
    pub length: Option<i64>,
    /// Optional scale for decimal types
    pub scale: Option<i64>,
    /// Array dimensions (e.g., Some(1) for TEXT[], Some(2) for TEXT[][])
    pub array_dimensions: Option<u32>,
}

impl fmt::Display for ColumnType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.base_type)?;
        if let Some(len) = self.length {
            write!(f, "({}", len)?;
            if let Some(scale) = self.scale {
                write!(f, ", {}", scale)?;
            }
            write!(f, ")")?;
        }
        if let Some(dim) = self.array_dimensions {
            for _ in 0..dim {
                write!(f, "[]")?;
            }
        }
        Ok(())
    }
}

/// Represents a column in a database table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    /// Column name
    pub name: String,
    /// Column data type
    pub column_type: ColumnType,
    /// Whether the column is nullable
    pub nullable: bool,
    /// Default value (if any)
    pub default_value: Option<String>,
    /// Whether this column is a primary key
    pub is_primary_key: bool,
    /// Whether this column is a foreign key
    pub is_foreign_key: bool,
    /// Referenced table (if this is a foreign key)
    pub references: Option<ForeignKeyReference>,
    /// Whether this column is unique
    pub is_unique: bool,
    /// Column comment (if any)
    pub comment: Option<String>,
}

impl fmt::Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.column_type)?;

        if self.is_primary_key {
            write!(f, " PRIMARY KEY")?;
        }
        if self.is_foreign_key {
            write!(f, " FOREIGN KEY")?;
        }
        if self.is_unique {
            write!(f, " UNIQUE")?;
        }
        if !self.nullable {
            write!(f, " NOT NULL")?;
        }
        if let Some(ref default) = self.default_value {
            write!(f, " DEFAULT {}", default)?;
        }
        if let Some(ref comment) = self.comment {
            write!(f, " -- {}", comment)?;
        }

        Ok(())
    }
}

/// Foreign key reference information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyReference {
    /// Referenced table name
    pub table: String,
    /// Referenced column name
    pub column: String,
    /// On delete action
    pub on_delete: Option<String>,
    /// On update action
    pub on_update: Option<String>,
}

/// Represents a database table or view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    /// Table or view name
    pub name: String,
    /// Whether this is a view (vs a table)
    pub is_view: bool,
    /// Table columns
    pub columns: Vec<Column>,
    /// Primary key columns (ordered)
    pub primary_keys: Vec<String>,
    /// Foreign key relationships
    pub foreign_keys: Vec<ForeignKeyReference>,
    /// Table comment (if any)
    pub comment: Option<String>,
    /// Estimated row count (if available)
    pub estimated_rows: Option<i64>,
}

impl Table {
    /// Create a new table
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            is_view: false,
            columns: Vec::new(),
            primary_keys: Vec::new(),
            foreign_keys: Vec::new(),
            comment: None,
            estimated_rows: None,
        }
    }

    /// Create a new view
    pub fn new_view(name: impl Into<String>) -> Self {
        let mut table = Self::new(name);
        table.is_view = true;
        table
    }

    /// Add a column to the table
    pub fn add_column(&mut self, column: Column) {
        self.columns.push(column);
    }

    /// Get a column by name
    pub fn get_column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// Format table schema for display
    pub fn format_schema(&self) -> String {
        let prefix = if self.is_view { "View" } else { "Table" };
        let mut result = format!("{}: {}\n", prefix, self.name);

        if let Some(ref comment) = self.comment {
            result.push_str(&format!("  -- {}\n", comment));
        }

        if !self.primary_keys.is_empty() {
            result.push_str(&format!("  Primary Key: {}\n", self.primary_keys.join(", ")));
        }

        if !self.foreign_keys.is_empty() {
            result.push_str("  Foreign Keys:\n");
            for fk in &self.foreign_keys {
                result.push_str(&format!(
                    "    {} -> {} ({})\n",
                    fk.column, fk.table, fk.column
                ));
            }
        }

        result.push_str("  Columns:\n");
        for column in &self.columns {
            result.push_str(&format!("    {}\n", column));
        }

        result
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_schema())
    }
}

/// Complete database schema index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaIndex {
    /// Database name (if available)
    pub database_name: Option<String>,
    /// Schema/namespace name (e.g., "public", "dbo")
    pub schema_name: Option<String>,
    /// Tables and views indexed by name
    pub tables: BTreeMap<String, Table>,
    /// Relationships between tables
    pub relationships: Vec<TableRelationship>,
    /// Index timestamp
    pub indexed_at: chrono::DateTime<chrono::Utc>,
}

impl SchemaIndex {
    /// Create a new schema index
    pub fn new() -> Self {
        Self {
            database_name: None,
            schema_name: None,
            tables: BTreeMap::new(),
            relationships: Vec::new(),
            indexed_at: chrono::Utc::now(),
        }
    }

    /// Add a table to the index
    pub fn add_table(&mut self, table: Table) {
        let name = table.name.clone();
        self.tables.insert(name, table);
    }

    /// Get a table by name
    pub fn get_table(&self, name: &str) -> Option<&Table> {
        self.tables.get(name)
    }

    /// Get all table names
    pub fn table_names(&self) -> Vec<&str> {
        self.tables.keys().map(|k| k.as_str()).collect()
    }

    /// Get all views
    pub fn views(&self) -> Vec<&Table> {
        self.tables
            .values()
            .filter(|t| t.is_view)
            .collect()
    }

    /// Get all tables (excluding views)
    pub fn tables_only(&self) -> Vec<&Table> {
        self.tables
            .values()
            .filter(|t| !t.is_view)
            .collect()
    }

    /// Format the entire schema for LLM context
    ///
    /// This provides a comprehensive, structured representation of the database
    /// schema suitable for inclusion in LLM prompts.
    pub fn format_for_llm(&self) -> String {
        let mut result = String::new();

        // Database header
        if let Some(ref db_name) = self.database_name {
            result.push_str(&format!("Database: {}\n", db_name));
        }
        if let Some(ref schema) = self.schema_name {
            result.push_str(&format!("Schema: {}\n", schema));
        }
        result.push_str(&format!(
            "Indexed at: {}\n\n",
            self.indexed_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));

        // Tables summary
        let table_count = self.tables.values().filter(|t| !t.is_view).count();
        let view_count = self.tables.values().filter(|t| t.is_view).count();

        result.push_str(&format!(
            "Contains {} tables and {} views\n\n",
            table_count, view_count
        ));

        // Detailed table information
        for (_name, table) in &self.tables {
            result.push_str(&table.format_schema());
            result.push_str("\n");
        }

        // Relationships section
        if !self.relationships.is_empty() {
            result.push_str("Relationships:\n");
            for rel in &self.relationships {
                result.push_str(&format!(
                    "  {}.{} -> {}.{} ({})\n",
                    rel.from_table, rel.from_column, rel.to_table, rel.to_column, rel.relationship_type
                ));
            }
        }

        result
    }

    /// Generate a concise schema summary for LLM
    ///
    /// This provides a more compact view focusing on table names and
    /// their relationships, useful when token count is limited.
    pub fn format_summary_for_llm(&self) -> String {
        let mut result = String::new();

        if let Some(ref db_name) = self.database_name {
            result.push_str(&format!("Database: {}\n", db_name));
        }

        result.push_str("\nTables:\n");
        for (name, table) in &self.tables {
            let prefix = if table.is_view { "[VIEW] " } else { "" };
            result.push_str(&format!("  {}{} (", prefix, name));

            // List column names with types
            let column_info: Vec<String> = table
                .columns
                .iter()
                .map(|c| {
                    let mut info = format!("{}: {}", c.name, c.column_type.base_type);
                    if c.is_primary_key {
                        info = format!("[PK] {}", info);
                    }
                    if c.is_foreign_key {
                        info = format!("[FK] {}", info);
                    }
                    info
                })
                .collect();

            result.push_str(&column_info.join(", "));
            result.push_str(")\n");
        }

        if !self.relationships.is_empty() {
            result.push_str("\nRelationships:\n");
            for rel in &self.relationships {
                result.push_str(&format!(
                    "  {}.{} -> {}.{}\n",
                    rel.from_table, rel.from_column, rel.to_table, rel.to_column
                ));
            }
        }

        result
    }

    /// Search tables by column name
    pub fn find_tables_with_column(&self, column_name: &str) -> Vec<&Table> {
        self.tables
            .values()
            .filter(|t| t.columns.iter().any(|c| c.name == column_name))
            .collect()
    }

    /// Search tables by name pattern
    pub fn find_tables_by_pattern(&self, pattern: &str) -> Vec<&Table> {
        let pattern_lower = pattern.to_lowercase();
        self.tables
            .values()
            .filter(|t| t.name.to_lowercase().contains(&pattern_lower))
            .collect()
    }
}

impl Default for SchemaIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SchemaIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_for_llm())
    }
}

/// Represents a relationship between two tables
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRelationship {
    /// Source table
    pub from_table: String,
    /// Source column
    pub from_column: String,
    /// Target table
    pub to_table: String,
    /// Target column
    pub to_column: String,
    /// Relationship type (e.g., "one-to-many", "many-to-many")
    pub relationship_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_type_display() {
        let col_type = ColumnType {
            base_type: "varchar".to_string(),
            length: Some(255),
            scale: None,
            array_dimensions: None,
        };
        assert_eq!(col_type.to_string(), "varchar(255)");

        let col_type = ColumnType {
            base_type: "integer".to_string(),
            length: None,
            scale: None,
            array_dimensions: None,
        };
        assert_eq!(col_type.to_string(), "integer");
    }

    #[test]
    fn test_table_creation() {
        let table = Table::new("users");
        assert_eq!(table.name, "users");
        assert!(!table.is_view);
        assert!(table.columns.is_empty());
    }

    #[test]
    fn test_schema_index() {
        let mut index = SchemaIndex::new();
        index.database_name = Some("test_db".to_string());

        let mut table = Table::new("users");
        table.add_column(Column {
            name: "id".to_string(),
            column_type: ColumnType {
                base_type: "integer".to_string(),
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
        });

        index.add_table(table);
        assert_eq!(index.tables.len(), 1);
        assert!(index.get_table("users").is_some());
    }

    #[test]
    fn test_llm_formatting() {
        let mut index = SchemaIndex::new();
        index.database_name = Some("test_db".to_string());

        let mut table = Table::new("users");
        table.add_column(Column {
            name: "id".to_string(),
            column_type: ColumnType {
                base_type: "integer".to_string(),
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
        });

        index.add_table(table);

        let formatted = index.format_for_llm();
        assert!(formatted.contains("Database: test_db"));
        assert!(formatted.contains("Table: users"));
        assert!(formatted.contains("id: integer PRIMARY KEY"));
    }
}
