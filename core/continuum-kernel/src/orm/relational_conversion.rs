//! Exact, database-side proofs for supported declaration conversions.
//! Proof and the existing rebuild share one transaction. No row bodies leave storage.
use sea_orm::{ConnectionTrait, DatabaseTransaction, DbBackend};
use sea_orm::sea_query::{Alias, Expr, Func, Query};
use super::schema_error::SchemaError;

/// TEXT -> numeric assignment is supported only for canonical signed integer text.
/// Round-trip equality rejects overflow, fractions, exponents, whitespace and malformed
/// values before SQLite's permissive numeric assignment can change their meaning.
/// NULL remains NULL. Conversion is performed once by the existing INSERT SELECT rebuild.
pub(super) async fn prove_integer_text(
    db: &DatabaseTransaction, table: &str, column: &str,
) -> Result<(), SchemaError> {
    let value = Expr::col(Alias::new(column));
    let round_trip = value.clone().cast_as(Alias::new("INTEGER"))
        .cast_as(Alias::new("TEXT"));
    let invalid = Query::select().expr(Expr::val(1))
        .from(Alias::new(table))
        .and_where(value.clone().is_not_null())
        .and_where(
            Expr::expr(Func::cust(Alias::new("typeof")).arg(value.clone())).ne("text")
                // Compare bytes so a source column's collation cannot accept a
                // spelling that differs from the exact canonical representation.
                .or(value.cast_as(Alias::new("BLOB")).ne(round_trip.cast_as(Alias::new("BLOB")))),
        ).limit(1).to_owned();
    if db.query_one(DbBackend::Sqlite.build(&invalid)).await?.is_some() {
        return Err(SchemaError::NonCanonicalIntegerText {
            table: table.into(), column: column.into(),
        });
    }
    Ok(())
}
