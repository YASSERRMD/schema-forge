//! Schema Indexer
//!
//! This module implements schema indexing logic for different database backends.
//! Each database type has its own indexing function that queries the system catalogs
//! and builds a complete SchemaIndex.

use crate::database::schema::{Column, ColumnType, ForeignKeyReference, SchemaIndex, Table, TableRelationship};
use crate::error::{Result, SchemaForgeError};
use sqlx::{postgres::PgPool, mysql::MySqlPool, sqlite::SqlitePool, Row};

/// Index PostgreSQL database schema
pub async fn index_postgresql(pool: &PgPool) -> Result<SchemaIndex> {
    let mut schema_index = SchemaIndex::new();

    // Get database name
    let db_row: Option<(String,)> = sqlx::query_as("SELECT current_database()")
        .fetch_optional(pool)
        .await?;
    if let Some((db_name,)) = db_row {
        schema_index.database_name = Some(db_name);
    }
    schema_index.schema_name = Some("public".to_string());

    // Query all tables and views
    let tables_query = r#"
        SELECT
            table_name,
            table_type,
            obj_description((table_schema||'.'||table_name)::regclass, 'pg_class') as comment
        FROM information_schema.tables
        WHERE table_schema = 'public'
        ORDER BY table_name
    "#;

    let tables_rows = sqlx::query(tables_query)
        .fetch_all(pool)
        .await
        .map_err(|e| SchemaForgeError::db_query(tables_query, e))?;

    for row in tables_rows {
        let table_name: String = row.get("table_name");
        let table_type: String = row.get("table_type");
        let comment: Option<String> = row.get("comment");

        let is_view = table_type == "VIEW";
        let mut table = if is_view {
            Table::new_view(&table_name)
        } else {
            Table::new(&table_name)
        };
        table.comment = comment;

        // Query columns for this table
        let columns_query = r#"
            SELECT
                column_name,
                data_type,
                character_maximum_length,
                numeric_precision,
                numeric_scale,
                is_nullable,
                column_default,
                ordinal_position
            FROM information_schema.columns
            WHERE table_schema = 'public'
                AND table_name = $1
            ORDER BY ordinal_position
        "#;

        let columns_rows = sqlx::query(columns_query)
            .bind(&table_name)
            .fetch_all(pool)
            .await
            .map_err(|e| SchemaForgeError::db_query(columns_query, e))?;

        for col_row in columns_rows {
            let column_name: String = col_row.get("column_name");
            let data_type: String = col_row.get("data_type");
            let max_len: Option<i64> = col_row.get("character_maximum_length");
            let precision: Option<i64> = col_row.get("numeric_precision");
            let scale: Option<i64> = col_row.get("numeric_scale");
            let is_nullable: String = col_row.get("is_nullable");
            let default_val: Option<String> = col_row.get("column_default");

            let column_type = ColumnType {
                base_type: data_type.clone(),
                length: max_len.or(precision),
                scale,
                array_dimensions: if data_type.ends_with("[]") {
                    Some(1)
                } else {
                    None
                },
            };

            let column = Column {
                name: column_name.clone(),
                column_type,
                nullable: is_nullable == "YES",
                default_value: default_val,
                is_primary_key: false, // Will be set below
                is_foreign_key: false, // Will be set below
                references: None,
                is_unique: false,
                comment: None,
            };

            table.add_column(column);
        }

        // Query primary keys
        let pk_query = r#"
            SELECT a.attname as column_name
            FROM pg_index i
            JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
            WHERE i.indrelid = $1::regclass AND i.indisprimary
            ORDER BY a.attnum
        "#;

        let pk_rows = sqlx::query(pk_query)
            .bind(&table_name)
            .fetch_all(pool)
            .await
            .map_err(|e| SchemaForgeError::db_query(pk_query, e))?;

        for pk_row in pk_rows {
            let pk_column: String = pk_row.get("column_name");
            table.primary_keys.push(pk_column.clone());
            if let Some(col) = table.columns.iter_mut().find(|c| c.name == pk_column) {
                col.is_primary_key = true;
            }
        }

        // Query foreign keys
        let fk_query = r#"
            SELECT
                kcu.column_name,
                ccu.table_name AS foreign_table_name,
                ccu.column_name AS foreign_column_name
            FROM information_schema.table_constraints AS tc
            JOIN information_schema.key_column_usage AS kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage AS ccu
                ON ccu.constraint_name = tc.constraint_name
                AND ccu.table_schema = tc.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
                AND tc.table_schema = 'public'
                AND tc.table_name = $1
        "#;

        let fk_rows = sqlx::query(fk_query)
            .bind(&table_name)
            .fetch_all(pool)
            .await
            .map_err(|e| SchemaForgeError::db_query(fk_query, e))?;

        for fk_row in fk_rows {
            let column_name: String = fk_row.get("column_name");
            let foreign_table: String = fk_row.get("foreign_table_name");
            let foreign_column: String = fk_row.get("foreign_column_name");

            let fk_ref = ForeignKeyReference {
                table: foreign_table.clone(),
                column: foreign_column.clone(),
                on_delete: None,
                on_update: None,
            };

            table.foreign_keys.push(fk_ref.clone());
            if let Some(col) = table.columns.iter_mut().find(|c| c.name == column_name) {
                col.is_foreign_key = true;
                col.references = Some(fk_ref);
            }

            // Add relationship
            let relationship = TableRelationship {
                from_table: table_name.clone(),
                from_column: column_name,
                to_table: foreign_table,
                to_column: foreign_column,
                relationship_type: "many-to-one".to_string(),
            };
            schema_index.relationships.push(relationship);
        }

        schema_index.add_table(table);
    }

    Ok(schema_index)
}

/// Index MySQL database schema
pub async fn index_mysql(pool: &MySqlPool) -> Result<SchemaIndex> {
    let mut schema_index = SchemaIndex::new();

    // Get database name
    let db_row: Option<(String,)> = sqlx::query_as("SELECT DATABASE()")
        .fetch_optional(pool)
        .await?;
    if let Some((db_name,)) = db_row {
        schema_index.database_name = Some(db_name);
    }

    // Query all tables and views
    let tables_query = r#"
        SELECT
            TABLE_NAME as table_name,
            TABLE_TYPE as table_type,
            TABLE_COMMENT as comment
        FROM information_schema.TABLES
        WHERE TABLE_SCHEMA = DATABASE()
            AND TABLE_TYPE IN ('BASE TABLE', 'VIEW')
        ORDER BY TABLE_NAME
    "#;

    let tables_rows = sqlx::query(tables_query)
        .fetch_all(pool)
        .await
        .map_err(|e| SchemaForgeError::db_query(tables_query, e))?;

    for row in tables_rows {
        let table_name: String = row.get("table_name");
        let table_type: String = row.get("table_type");
        let comment: Option<String> = row.get("comment");

        let is_view = table_type == "VIEW";
        let mut table = if is_view {
            Table::new_view(&table_name)
        } else {
            Table::new(&table_name)
        };
        table.comment = comment;

        // Query columns
        let columns_query = r#"
            SELECT
                COLUMN_NAME as column_name,
                DATA_TYPE as data_type,
                CHARACTER_MAXIMUM_LENGTH as character_maximum_length,
                NUMERIC_PRECISION as numeric_precision,
                NUMERIC_SCALE as numeric_scale,
                IS_NULLABLE as is_nullable,
                COLUMN_DEFAULT as column_default,
                COLUMN_KEY as column_key,
                ORDINAL_POSITION as ordinal_position
            FROM information_schema.COLUMNS
            WHERE TABLE_SCHEMA = DATABASE()
                AND TABLE_NAME = $1
            ORDER BY ORDINAL_POSITION
        "#;

        let columns_rows = sqlx::query(columns_query)
            .bind(&table_name)
            .fetch_all(pool)
            .await
            .map_err(|e| SchemaForgeError::db_query(columns_query, e))?;

        for col_row in columns_rows {
            let column_name: String = col_row.get("column_name");
            let data_type: String = col_row.get("data_type");
            let max_len: Option<i64> = col_row.get("character_maximum_length");
            let precision: Option<i64> = col_row.get("numeric_precision");
            let scale: Option<i64> = col_row.get("numeric_scale");
            let is_nullable: String = col_row.get("is_nullable");
            let default_val: Option<String> = col_row.get("column_default");
            let column_key: Option<String> = col_row.get("column_key");

            let column_type = ColumnType {
                base_type: data_type,
                length: max_len.or(precision),
                scale,
                array_dimensions: None,
            };

            let is_pk = column_key.as_deref() == Some("PRI");

            let column = Column {
                name: column_name.clone(),
                column_type,
                nullable: is_nullable == "YES",
                default_value: default_val,
                is_primary_key: is_pk,
                is_foreign_key: false,
                references: None,
                is_unique: column_key.as_deref() == Some("UNI"),
                comment: None,
            };

            if is_pk {
                table.primary_keys.push(column_name.clone());
            }

            table.add_column(column);
        }

        // Query foreign keys
        let fk_query = r#"
            SELECT
                COLUMN_NAME as column_name,
                REFERENCED_TABLE_NAME as foreign_table_name,
                REFERENCED_COLUMN_NAME as foreign_column_name
            FROM information_schema.KEY_COLUMN_USAGE
            WHERE TABLE_SCHEMA = DATABASE()
                AND TABLE_NAME = $1
                AND REFERENCED_TABLE_NAME IS NOT NULL
        "#;

        let fk_rows = sqlx::query(fk_query)
            .bind(&table_name)
            .fetch_all(pool)
            .await
            .map_err(|e| SchemaForgeError::db_query(fk_query, e))?;

        for fk_row in fk_rows {
            let column_name: String = fk_row.get("column_name");
            let foreign_table: String = fk_row.get("foreign_table_name");
            let foreign_column: String = fk_row.get("foreign_column_name");

            let fk_ref = ForeignKeyReference {
                table: foreign_table.clone(),
                column: foreign_column.clone(),
                on_delete: None,
                on_update: None,
            };

            table.foreign_keys.push(fk_ref.clone());
            if let Some(col) = table.columns.iter_mut().find(|c| c.name == column_name) {
                col.is_foreign_key = true;
                col.references = Some(fk_ref);
            }

            let relationship = TableRelationship {
                from_table: table_name.clone(),
                from_column: column_name,
                to_table: foreign_table,
                to_column: foreign_column,
                relationship_type: "many-to-one".to_string(),
            };
            schema_index.relationships.push(relationship);
        }

        schema_index.add_table(table);
    }

    Ok(schema_index)
}

/// Index SQLite database schema
pub async fn index_sqlite(pool: &SqlitePool) -> Result<SchemaIndex> {
    let mut schema_index = SchemaIndex::new();
    schema_index.database_name = Some("main".to_string());
    schema_index.schema_name = Some("main".to_string());

    // Query all tables
    let tables_query = r#"
        SELECT name, type
        FROM sqlite_master
        WHERE type IN ('table', 'view')
            AND name NOT LIKE 'sqlite_%'
        ORDER BY name
    "#;

    let tables_rows = sqlx::query(tables_query)
        .fetch_all(pool)
        .await
        .map_err(|e| SchemaForgeError::db_query(tables_query, e))?;

    for row in tables_rows {
        let table_name: String = row.get("name");
        let table_type: String = row.get("type");

        let is_view = table_type == "view";
        let mut table = if is_view {
            Table::new_view(&table_name)
        } else {
            Table::new(&table_name)
        };

        // Get CREATE TABLE/VIEW SQL to parse columns
        let create_sql_query = "SELECT sql FROM sqlite_master WHERE name = $1";
        let create_sql_row: Option<(String,)> = sqlx::query_as(create_sql_query)
            .bind(&table_name)
            .fetch_optional(pool)
            .await
            .map_err(|e| SchemaForgeError::db_query(create_sql_query, e))?;

        if let Some((sql,)) = create_sql_row {
            table.comment = Some(sql.clone());

            // Parse the CREATE statement to extract columns
            if let Some(columns_start) = sql.find('(') {
                let columns_str = &sql[columns_start + 1..];
                if let Some(columns_end) = columns_str.rfind(')') {
                    let columns_def = &columns_str[..columns_end];

                    for column_def in columns_def.split(',') {
                        let column_def = column_def.trim();
                        if column_def.to_uppercase().starts_with("PRIMARY KEY")
                            || column_def.to_uppercase().starts_with("FOREIGN KEY")
                            || column_def.to_uppercase().starts_with("UNIQUE")
                            || column_def.to_uppercase().starts_with("CHECK")
                            || column_def.to_uppercase().starts_with("CONSTRAINT") {
                            continue;
                        }

                        let parts: Vec<&str> = column_def.split_whitespace().collect();
                        if parts.is_empty() {
                            continue;
                        }

                        let column_name = parts[0].to_string();
                        let data_type = if parts.len() > 1 {
                            parts[1].to_string()
                        } else {
                            "TEXT".to_string()
                        };

                        // Parse constraints
                        let is_pk = column_def.to_uppercase().contains("PRIMARY KEY");
                        let is_nullable = !column_def.to_uppercase().contains("NOT NULL");
                        let is_unique = column_def.to_uppercase().contains("UNIQUE");

                        if is_pk {
                            table.primary_keys.push(column_name.clone());
                        }

                        let column_type = ColumnType {
                            base_type: data_type,
                            length: None,
                            scale: None,
                            array_dimensions: None,
                        };

                        let column = Column {
                            name: column_name,
                            column_type,
                            nullable: is_nullable,
                            default_value: None,
                            is_primary_key: is_pk,
                            is_foreign_key: false,
                            references: None,
                            is_unique,
                            comment: None,
                        };

                        table.add_column(column);
                    }
                }
            }
        }

        schema_index.add_table(table);
    }

    Ok(schema_index)
}

/// Index MSSQL database schema
pub async fn index_mssql(_pool: &sqlx::AnyPool) -> Result<SchemaIndex> {
    // TODO: Full MSSQL support requires tiberius client
    // For now, return basic schema
    let mut schema_index = SchemaIndex::new();

    // Try to get database name
    let db_row: Option<(String,)> = sqlx::query_as("SELECT DB_NAME()")
        .fetch_optional(_pool)
        .await?;
    if let Some((db_name,)) = db_row {
        schema_index.database_name = Some(db_name);
    }
    schema_index.schema_name = Some("dbo".to_string());

    // Note: This is a placeholder implementation
    // Full MSSQL indexing will be implemented with tiberius in a future update

    Err(SchemaForgeError::SchemaIndexing(
        "MSSQL indexing requires tiberius client - not yet implemented".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indexer_module_exists() {
        // Basic test to verify module compiles
        assert!(true);
    }
}
