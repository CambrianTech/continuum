//! Aggregate descriptor groups in storage; never transfer or decode payload bodies.
use super::{adapter::{naming, NumericArrayStats}, binary::{BinaryDescriptor, descriptor_column}};
use futures::StreamExt;
use sea_orm::{DbBackend, StreamTrait};
use sea_orm::sea_query::{Alias, Expr, Func, Query};

impl super::relational::RelationalStore {
    pub async fn numeric_array_stats(&self, collection: &str, field: &str) -> Result<NumericArrayStats, String> {
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
        let mut groups = self.db.stream(statement).await.map_err(|e| e.to_string())?;
        let mut stats = NumericArrayStats { total_records: 0, non_empty_records: 0, min_length: 0, max_length: 0 };
        while let Some(row) = groups.next().await {
            let row = row.map_err(|e| e.to_string())?;
            let count: i64 = row.try_get("", "rows").map_err(|e| e.to_string())?;
            let payloads: i64 = row.try_get("", "payloads").map_err(|e| e.to_string())?;
            let count = usize::try_from(count).map_err(|_| "invalid binary count")?;
            stats.total_records = stats.total_records.checked_add(count).ok_or("binary count overflow")?;
            let header: Option<Vec<u8>> = row.try_get("", "descriptor").map_err(|e| e.to_string())?;
            match header {
                None if payloads == 0 => {},
                None => return Err("binary payload is missing its descriptor".into()),
                Some(header) => {
                    if usize::try_from(payloads).map_err(|_| "invalid payload count")? != count { return Err("binary descriptor is missing its payload".into()); }
                    let descriptor = BinaryDescriptor::decode(&header)?;
                    let [dimension] = descriptor.shape.as_slice() else { return Err("vector statistics require rank-one payloads".into()); };
                    let bytes = i64::try_from(descriptor.byte_len()?).map_err(|_| "binary size exceeds i64")?;
                    let min: Option<i64> = row.try_get("", "minimum").map_err(|e| e.to_string())?;
                    let max: Option<i64> = row.try_get("", "maximum").map_err(|e| e.to_string())?;
                    if min != Some(bytes) || max != Some(bytes) { return Err("binary payload length disagrees with descriptor".into()); }
                    let dimension = usize::try_from(*dimension).map_err(|_| "dimension exceeds usize")?;
                    if dimension != 0 {
                        stats.min_length = if stats.non_empty_records == 0 { dimension } else { stats.min_length.min(dimension) };
                        stats.max_length = stats.max_length.max(dimension);
                        stats.non_empty_records = stats.non_empty_records.checked_add(count).ok_or("binary count overflow")?;
                    }
                }
            }
        }
        Ok(stats)
    }
}
