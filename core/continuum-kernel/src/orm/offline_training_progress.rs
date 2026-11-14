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

/// Why the offline migration refused or failed, typed for its caller.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("migration source must be an existing database file")]
    NotAFile,
    #[error("database path is not UTF-8")]
    NotUtf8,
    #[error("database path contains URI delimiters or escapes")]
    UriDelimiters,
    #[error("refusing {invalid} invalid non-NULL progress values")]
    InvalidProgress { invalid: u64 },
    #[error("migration cardinality mismatch: changed {changed} of {historical_nulls} historical NULLs, {remaining} remain; refusing commit")]
    Cardinality { changed: u64, historical_nulls: u64, remaining: u64 },
    #[error(transparent)]
    Database(#[from] DbErr),
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Receipt {
    pub rows: u64,
    pub historical_nulls: u64,
    pub changed: u64,
    pub applied: bool,
}

/// Call only on an explicit offline database. Apply is an explicit assertion
/// that these historical NULLs mean unstarted, not unknown training progress.
pub async fn run(path: &Path, apply_historical_unstarted: bool) -> Result<Receipt, MigrationError> {
    if !path.is_file() {
        return Err(MigrationError::NotAFile);
    }
    let path = path
        .to_str()
        .ok_or(MigrationError::NotUtf8)?
        .replace('\\', "/");
    if path.contains(['?', '#', '%']) {
        return Err(MigrationError::UriDelimiters);
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
        .await?;
    let result = migrate(&db, apply_historical_unstarted).await;
    db.close().await?;
    result
}

async fn migrate(db: &DatabaseConnection, apply: bool) -> Result<Receipt, MigrationError> {
    let tx = db.begin().await?;
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
        .await?;
    if invalid != 0 {
        return Err(MigrationError::InvalidProgress { invalid });
    }
    let rows = Entity::find()
        .count(&tx)
        .await?;
    let historical_nulls = Entity::find()
        .filter(Column::Learned.is_null())
        .count(&tx)
        .await?;
    let changed = if apply {
        let changed = Entity::update_many()
            .col_expr(Column::Learned, Expr::value(0_i64))
            .filter(Column::Learned.is_null())
            .exec(&tx)
            .await?
            .rows_affected;
        let remaining = Entity::find()
            .filter(Column::Learned.is_null())
            .count(&tx)
            .await?;
        if changed != historical_nulls || remaining != 0 {
            return Err(MigrationError::Cardinality { changed, historical_nulls, remaining });
        }
        tx.commit().await?;
        changed
    } else {
        tx.rollback().await?;
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
