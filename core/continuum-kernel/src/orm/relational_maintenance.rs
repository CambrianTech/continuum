//! Database-side aggregates and maintenance over admitted entity declarations.
use super::{
    adapter::{naming, ClearAllResult},
    query::QueryBuilder,
    types::CollectionStats,
};
use sea_orm::sea_query::{Alias, Expr, Func, Query, SimpleExpr};
use sea_orm::{ConnectionTrait, DbBackend};

impl super::relational::RelationalStore {
    pub async fn stats(&self, collection: &str) -> Result<CollectionStats, String> {
        let schema = self.schema(collection)?;
        // Logical stored value bytes, not allocated SQLite file/page bytes. The database
        // computes this aggregate; no record body crosses the connection boundary.
        let mut bytes: SimpleExpr = Expr::val(0).into();
        for field in &schema.fields {
            let length = Func::char_length(
                Expr::col(Alias::new(naming::to_snake_case(&field.name)))
                    .cast_as(Alias::new("BLOB")),
            );
            bytes = Expr::expr(bytes).add(Func::coalesce([length.into(), Expr::val(0).into()]));
        }
        let query = Query::select()
            .from(Alias::new(naming::to_table_name(collection)))
            .expr_as(Expr::col(Alias::new("id")).count(), Alias::new("count"))
            .expr_as(
                Func::coalesce([Expr::expr(bytes).sum().into(), Expr::val(0).into()]),
                Alias::new("bytes"),
            )
            .expr_as(
                Expr::col(Alias::new("updated_at")).max(),
                Alias::new("modified"),
            )
            .to_owned();
        let row = self
            .db
            .query_one(DbBackend::Sqlite.build(&query))
            .await
            .map_err(|e| e.to_string())?
            .ok_or("statistics returned no row")?;
        let count: i64 = row.try_get("", "count").map_err(|e| e.to_string())?;
        let bytes: i64 = row.try_get("", "bytes").map_err(|e| e.to_string())?;
        let modified: Option<String> = row.try_get("", "modified").map_err(|e| e.to_string())?;
        Ok(CollectionStats {
            name: collection.into(),
            record_count: count.try_into().map_err(|_| "count exceeds usize")?,
            total_size: bytes.try_into().map_err(|_| "size exceeds usize")?,
            // Empty collections have no last mutation timestamp in this row-based API.
            last_modified: match modified {
                Some(value) => value,
                None => String::new(),
            },
            schema: Some(serde_json::to_string(schema.as_ref()).map_err(|e| e.to_string())?),
            indices: Some(
                schema
                    .indexes
                    .iter()
                    .map(|index| index.name.clone())
                    .collect(),
            ),
        })
    }

    pub async fn clear(&self) -> Result<ClearAllResult, String> {
        let mut remaining = self
            .collections()
            .into_iter()
            .map(|name| self.schema(&name))
            .collect::<Result<Vec<_>, _>>()?;
        let mut ordered = Vec::with_capacity(remaining.len());
        while !remaining.is_empty() {
            let leaf = remaining
                .iter()
                .position(|candidate| {
                    !remaining.iter().any(|other| {
                        other.collection != candidate.collection
                            && other.fields.iter().any(|field| {
                                field.foreign_key.as_ref().is_some_and(|reference| {
                                    reference.collection == candidate.collection
                                        && reference.on_delete != super::types::CascadeRule::SetNull
                                })
                            })
                    })
                })
                .ok_or("clear_all requires an explicit deletion policy for cyclic foreign keys")?;
            ordered.push(remaining.remove(leaf).collection.clone());
        }
        self.write_with_revisions(move |store, transaction| {
            Box::pin(async move {
                let mut records_deleted = 0usize;
                let mut revisions = Vec::new();
                for collection in &ordered {
                    let deleted = store
                        .delete_on(transaction, QueryBuilder::new(collection).build())
                        .await?;
                    records_deleted = records_deleted
                        .checked_add(deleted.count)
                        .ok_or("deleted count exceeds usize")?;
                    revisions.extend(deleted.revisions);
                }
                // Sequence high-water records deliberately survive content deletion.
                Ok((
                    ClearAllResult {
                        tables_cleared: ordered,
                        records_deleted,
                    },
                    revisions,
                ))
            })
        })
        .await
    }

    pub async fn optimize(&self) -> Result<(), String> {
        if self.in_memory {
            return Err(
                "connection retirement is not supported for volatile in-memory storage".into(),
            );
        }
        let _guard = self.writer_gate.lock().await;
        // SQLx owns SQLite's optimize-on-close protocol. Retire one pooled connection
        // through that supported lifecycle without handwritten maintenance statements.
        self.db
            .get_sqlite_connection_pool()
            .acquire()
            .await
            .map_err(|e| e.to_string())?
            .close()
            .await
            .map_err(|e| e.to_string())
    }
}
