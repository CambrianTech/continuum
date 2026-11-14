//! A native read retains the existing SeaORM transaction, including payload custody.
use super::{
    adapter::naming,
    binary::{descriptor_column, BinaryDescriptor},
    types::{CollectionSchema, FieldType},
};
use sea_orm::sea_query::{Alias, Expr, Order, Query};
use sea_orm::{ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait, TransactionTrait};
use std::sync::Arc;

/// A database snapshot, not a consumer-adoption receipt. Drop releases the read transaction.
pub struct BinarySnapshot {
    transaction: DatabaseTransaction,
    schema: Arc<CollectionSchema>,
    revision: u64,
}
impl super::relational::RelationalStore {
    pub async fn binary_snapshot(&self, collection: &str) -> Result<BinarySnapshot, String> {
        let schema = self.schema(collection)?;
        let transaction = self.db.begin().await.map_err(|error| error.to_string())?;
        // This read pins SQLite's snapshot even when the collection has no native commits.
        let head = super::relational_sequence::Entity::find_by_id((
            "native".to_owned(),
            collection.to_owned(),
        ))
        .one(&transaction)
        .await
        .map_err(|error| error.to_string())?;
        let revision = match head {
            Some(head) => u64::try_from(head.high).map_err(|_| "invalid native revision")?,
            None => 0,
        };
        Ok(BinarySnapshot {
            transaction,
            schema,
            revision,
        })
    }
}
impl BinarySnapshot {
    /// Native payload revision visible to these reads, not proof of applied GPU work.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// One provider-to-caller bulk copy per row; no per-row payload allocation.
    /// NULL payloads are selected out before LIMIT. Every row is validated before
    /// output mutation, so refusal or cancellation leaves both buffers unchanged.
    /// Pages use the same pinned database snapshot; ordering is still by ID.
    pub async fn page_into(
        &self,
        request: &super::binary::BinaryPageRequest<'_>,
        output: super::binary::BinaryPageOutput<'_>,
    ) -> Result<Option<BinaryDescriptor>, String> {
        use sea_orm::sqlx::Row;
        let super::binary::BinaryPageRequest {
            field,
            after,
            limit,
        } = *request;
        let super::binary::BinaryPageOutput { bytes, ids } = output;
        let collection = &self.schema.collection;
        let declared = self
            .schema
            .fields
            .iter()
            .find(|item| item.name == field)
            .ok_or("undeclared binary field")?;
        if declared.field_type != FieldType::Binary {
            return Err("native payload access requires a declared binary field".into());
        }
        let column = super::relational_query::column(&self.schema, field)?;
        let header_column = Alias::new(descriptor_column(field));
        let mut query = Query::select();
        query
            .from(Alias::new(naming::to_table_name(collection)))
            .column(Alias::new("id"))
            .expr_as(Expr::col(column.clone()), Alias::new("payload"))
            .expr_as(Expr::col(header_column.clone()), Alias::new("descriptor"))
            .and_where(
                Expr::col(column)
                    .is_not_null()
                    .or(Expr::col(header_column).is_not_null()),
            )
            .order_by(Alias::new("id"), Order::Asc)
            .limit(u64::try_from(limit.get()).map_err(|_| "page size exceeds u64")?);
        if let Some(after) = after {
            query.and_where(Expr::col(Alias::new("id")).gt(after.to_string()));
        }
        let rows = self
            .transaction
            .query_all(DbBackend::Sqlite.build(&query))
            .await
            .map_err(|e| e.to_string())?;
        let mut borrowed = Vec::with_capacity(rows.len());
        let mut first_header: Option<&[u8]> = None;
        let mut descriptor = None;
        let mut stride = 0;
        let mut total = 0_usize;
        for row in &rows {
            let row = row
                .try_as_sqlite_row()
                .ok_or("native page requires SQLite provider row")?;
            let id: &str = row.try_get("id").map_err(|e| e.to_string())?;
            let payload: Option<&[u8]> = row.try_get("payload").map_err(|e| e.to_string())?;
            let header: Option<&[u8]> = row.try_get("descriptor").map_err(|e| e.to_string())?;
            let (payload, header) = match (payload, header) {
                (Some(payload), Some(header)) => (payload, header),
                _ => {
                    return Err("binary payload and descriptor have inconsistent NULL state".into())
                }
            };
            match first_header {
                Some(first) if first != header => {
                    return Err("binary page mixes producer descriptors".into())
                }
                Some(_) => {}
                None => {
                    let decoded = BinaryDescriptor::decode(header)?;
                    stride = decoded.byte_len()?;
                    descriptor = Some(decoded);
                    first_header = Some(header);
                }
            }
            if stride != payload.len() {
                return Err("binary payload length disagrees with descriptor".into());
            }
            total = total
                .checked_add(payload.len())
                .ok_or("native page exceeds address space")?;
            borrowed.push((super::binary::native_identity(id)?, payload));
        }
        bytes.try_reserve(total).map_err(|e| e.to_string())?;
        ids.try_reserve(borrowed.len()).map_err(|e| e.to_string())?;
        // No await or fallible decoding after the caller buffers start changing.
        for (id, payload) in borrowed {
            bytes.extend_from_slice(payload);
            ids.push(id);
        }
        Ok(descriptor)
    }
}
