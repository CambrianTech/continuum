//! Aggregate descriptor groups in storage; never transfer or decode payload bodies.
use super::storage_error::StorageError;
use super::{adapter::{naming, NumericArrayStats}, binary::{BinaryDescriptor, descriptor_column}};
use futures::StreamExt;
use sea_orm::{DbBackend, StreamTrait};
use sea_orm::sea_query::{Alias, Expr, Func, Query};

impl super::relational::RelationalStore {
    pub async fn numeric_array_stats(&self, collection: &str, field: &str) -> Result<NumericArrayStats, StorageError> {
        let column = self.binary_column(collection, field)?;
        let descriptor = Alias::new(descriptor_column(field));
        let length = Func::char_length(Expr::col(column.clone()));
        let query = Query::select().from(Alias::new(naming::to_table_name(collection)))
            .expr_as(Expr::col(descriptor.clone()), Alias::new("descriptor"))
            .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("rows"))
            .expr_as(Expr::col(column).count(), Alias::new("payloads"))
            .expr_as(Expr::expr(length.clone()).min(), Alias::new("minimum"))
            .expr_as(Expr::expr(length).max(), Alias::new("maximum"))
            .group_by_col(descriptor).to_owned();
        let statement = DbBackend::Sqlite.build(&query);
        let mut groups = self.db.stream(statement).await?;
        let mut stats = NumericArrayStats { total_records: 0, non_empty_records: 0, min_length: 0, max_length: 0 };
        while let Some(row) = groups.next().await {
            let row = row?;
            let count: i64 = row.try_get("", "rows")?;
            let payloads: i64 = row.try_get("", "payloads")?;
            let count = usize::try_from(count).map_err(|_| StorageError::Corrupt { what: "a negative row count" })?;
            stats.total_records = stats.total_records.checked_add(count).ok_or(StorageError::Overflow { what: "a record count" })?;
            let header: Option<Vec<u8>> = row.try_get("", "descriptor")?;
            match header {
                None if payloads == 0 => {},
                None => return Err(StorageError::Corrupt { what: "a binary payload without a descriptor" }),
                Some(header) => {
                    if usize::try_from(payloads).map_err(|_| StorageError::Corrupt { what: "a negative payload count" })? != count { return Err(StorageError::Corrupt { what: "a binary descriptor without its payload" }); }
                    let descriptor = BinaryDescriptor::decode(&header)?;
                    let [dimension] = descriptor.shape.as_slice() else { return Err(StorageError::Unqueryable(super::storage_error::QueryFault::RankOne)); };
                    let bytes = i64::try_from(descriptor.byte_len()?).map_err(|_| StorageError::Overflow { what: "a binary size" })?;
                    let min: Option<i64> = row.try_get("", "minimum")?;
                    let max: Option<i64> = row.try_get("", "maximum")?;
                    if min != Some(bytes) || max != Some(bytes) { return Err(StorageError::Corrupt { what: "a stored binary payload disagrees with its descriptor length" }); }
                    let dimension = usize::try_from(*dimension).map_err(|_| StorageError::Overflow { what: "a vector dimension" })?;
                    if dimension != 0 {
                        stats.min_length = if stats.non_empty_records == 0 { dimension } else { stats.min_length.min(dimension) };
                        stats.max_length = stats.max_length.max(dimension);
                        stats.non_empty_records = stats.non_empty_records.checked_add(count).ok_or(StorageError::Overflow { what: "a record count" })?;
                    }
                }
            }
        }
        Ok(stats)
    }
}
