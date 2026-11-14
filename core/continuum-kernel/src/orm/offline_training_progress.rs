//! Offline-only migration; included by the migration executable, never the runtime ORM.
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::{Alias, Expr, Func};
use sea_orm::{Condition, ConnectOptions, Database, DatabaseConnection, TransactionTrait};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "training_trigger_submissions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub learned: Option<i64>,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Receipt {
    pub rows: u64,
    pub historical_nulls: u64,
    pub changed: u64,
    pub applied: bool,
}

/// Call only on an explicit offline database. Apply is an explicit assertion
/// that these historical NULLs mean unstarted, not unknown training progress.
pub async fn run(path: &Path, apply_historical_unstarted: bool) -> Result<Receipt, String> {
    if !path.is_file() {
        return Err("migration source must be an existing database file".into());
    }
    let path = path
        .to_str()
        .ok_or("database path is not UTF-8")?
        .replace('\\', "/");
    if path.contains(['?', '#', '%']) {
        return Err("database path contains URI delimiters or escapes".into());
    }
    let mode = if apply_historical_unstarted {
        "rw"
    } else {
        "ro"
    };
    let mut options = ConnectOptions::new(format!("sqlite://{path}?mode={mode}"));
    // One offline transaction, no runtime pool or alternate backend.
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options)
        .await
        .map_err(|error| error.to_string())?;
    let result = migrate(&db, apply_historical_unstarted).await;
    db.close().await.map_err(|error| error.to_string())?;
    result
}

async fn migrate(db: &DatabaseConnection, apply: bool) -> Result<Receipt, String> {
    let tx = db.begin().await.map_err(|error| error.to_string())?;
    // Validate storage classes before typed decoding can coerce REAL or TEXT.
    // Aggregate queries never retrieve bodies or materialize all submissions.
    let invalid = Condition::all().add(Column::Learned.is_not_null()).add(
        Condition::any()
            .add(Column::Learned.lt(0_i64))
            .add(Column::Learned.gt(i64::from(u32::MAX)))
            .add(
                Expr::expr(Func::cust(Alias::new("typeof")).arg(Expr::col(Column::Learned)))
                    .ne("integer"),
            ),
    );
    let invalid = Entity::find()
        .filter(invalid)
        .count(&tx)
        .await
        .map_err(|error| error.to_string())?;
    if invalid != 0 {
        return Err(format!(
            "refusing {invalid} invalid non-NULL progress values"
        ));
    }
    let rows = Entity::find()
        .count(&tx)
        .await
        .map_err(|error| error.to_string())?;
    let historical_nulls = Entity::find()
        .filter(Column::Learned.is_null())
        .count(&tx)
        .await
        .map_err(|error| error.to_string())?;
    let changed = if apply {
        let changed = Entity::update_many()
            .col_expr(Column::Learned, Expr::value(0_i64))
            .filter(Column::Learned.is_null())
            .exec(&tx)
            .await
            .map_err(|error| error.to_string())?
            .rows_affected;
        let remaining = Entity::find()
            .filter(Column::Learned.is_null())
            .count(&tx)
            .await
            .map_err(|error| error.to_string())?;
        if changed != historical_nulls || remaining != 0 {
            return Err("migration cardinality mismatch; refusing commit".into());
        }
        tx.commit().await.map_err(|error| error.to_string())?;
        changed
    } else {
        tx.rollback().await.map_err(|error| error.to_string())?;
        0
    };
    Ok(Receipt {
        rows,
        historical_nulls,
        changed,
        applied: apply,
    })
}

#[cfg(test)]
#[path = "offline_training_progress_tests.rs"]
mod tests;
