use super::*;

/// Auto-create table from JSON data shape (matches SQLite behavior).
/// `qualified_table` is the schema-qualified name for DDL/DML (e.g. "test_orm.users").
/// `bare_table` is the unqualified name for information_schema queries.
/// `schema` is the schema name for information_schema filtering.
pub(super) async fn ensure_table_exists_pg(
    client: &impl deadpool_postgres::GenericClient,
    qualified_table: &str,
    bare_table: &str,
    schema: &str,
    data: &Value,
) -> Result<(), String> {
    // Try CREATE TABLE IF NOT EXISTS first (idempotent, handles races).
    // Previous approach checked SELECT EXISTS first, but that TOCTOU race
    // caused "duplicate key on pg_class" errors under concurrent creates.
    let mut columns = vec![
        "id TEXT PRIMARY KEY".to_string(),
        "created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()".to_string(),
        "updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()".to_string(),
        "version BIGINT NOT NULL DEFAULT 1".to_string(),
    ];

    if let Value::Object(obj) = data {
        for (key, value) in obj {
            if METADATA_KEYS.contains(&key.as_str()) {
                continue;
            }
            let col_name = naming::to_snake_case(key);
            let col_type = pg_type_from_value(value, &col_name);
            columns.push(format!("{} {}", col_name, col_type));
        }
    }

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} ({})",
        qualified_table,
        columns.join(", ")
    );

    match client.execute(&sql, &[]).await {
        Ok(_) => {}
        Err(e) => {
            // Concurrent CREATE TABLE IF NOT EXISTS can race on pg_class.
            // Error code 42P07 = duplicate_table — another connection won the race.
            // Error code 23505 = unique_violation on pg_class index.
            // Both mean the table exists — safe to continue.
            let is_race = e.as_db_error().map_or(false, |db| {
                matches!(
                    db.code(),
                    &tokio_postgres::error::SqlState::DUPLICATE_TABLE
                        | &tokio_postgres::error::SqlState::UNIQUE_VIOLATION
                )
            });
            if !is_race {
                return Err(format!("Create table failed: {}", format_pg_error(&e)));
            }
        }
    }

    // Schema evolution: add any missing columns
    if let Value::Object(obj) = data {
        let cols_sql = format!(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = '{}' AND table_name = '{}'",
            schema, bare_table
        );
        let rows = client
            .query(&cols_sql, &[])
            .await
            .map_err(|e| format!("Column check failed: {}", format_pg_error(&e)))?;
        let existing_cols: Vec<String> = rows.iter().map(|r| r.get::<_, String>(0)).collect();

        for (key, value) in obj {
            if METADATA_KEYS.contains(&key.as_str()) {
                continue;
            }
            let col_name = naming::to_snake_case(key);
            if !existing_cols.contains(&col_name) {
                let col_type = pg_type_from_value(value, &col_name);
                let alter_sql = format!(
                    "ALTER TABLE {} ADD COLUMN IF NOT EXISTS {} {}",
                    qualified_table, col_name, col_type
                );
                client
                    .execute(&alter_sql, &[])
                    .await
                    .map_err(|e| format!("Add column failed: {}", format_pg_error(&e)))?;
            }
        }
    }

    Ok(())
}
