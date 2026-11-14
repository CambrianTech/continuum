//! A native read retains the existing SeaORM transaction, including payload custody.
use super::storage_error::StorageError;
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
    pub(super) transaction: DatabaseTransaction,
    pub(super) schema: Arc<CollectionSchema>,
    revision: u64,
}
impl super::relational::RelationalStore {
    pub async fn binary_snapshot(&self, collection: &str) -> Result<BinarySnapshot, StorageError> {
        let schema = self.schema(collection)?;
        let transaction = self.db.begin().await?;
        // This read pins SQLite's snapshot even when the collection has no native commits.
        let head = super::relational_sequence::Entity::find_by_id((
            "native".to_owned(),
            collection.to_owned(),
        ))
        .one(&transaction)
        .await?;
        let revision = match head {
            Some(head) => u64::try_from(head.high).map_err(|_| StorageError::Corrupt { what: "a native revision is negative" })?,
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
    ) -> Result<Option<BinaryDescriptor>, StorageError> {
        self.page(request, None, output).await
    }

    /// [`Self::page_into`] restricted to rows written under exactly `descriptor`, so a
    /// consumer reads one producer's rows as one page sequence and never meets a mixed page.
    pub async fn page_matching_into(
        &self,
        request: &super::binary::BinaryPageRequest<'_>,
        descriptor: &BinaryDescriptor,
        output: super::binary::BinaryPageOutput<'_>,
    ) -> Result<Option<BinaryDescriptor>, StorageError> {
        let header = descriptor.encode()?;
        self.page(request, Some(&header), output).await
    }

    /// One keyset page of distinct descriptors; the transfer limit never caps the corpus.
    pub async fn descriptor_page(
        &self,
        request: &super::binary::BinaryDescriptorPageRequest<'_>,
    ) -> Result<Vec<BinaryDescriptor>, StorageError> {
        use sea_orm::sqlx::Row;
        let super::binary::BinaryDescriptorPageRequest { field, after, limit } = *request;
        self.header_column(field).ok_or_else(|| self.not_binary(field))?;
        let header = || Alias::new(descriptor_column(field));
        let mut query = Query::select();
        query
            .distinct()
            .from(Alias::new(naming::to_table_name(&self.schema.collection)))
            .expr_as(Expr::col(header()), Alias::new("descriptor"))
            .and_where(Expr::col(header()).is_not_null())
            .order_by(header(), Order::Asc)
            .limit(u64::try_from(limit.get()).map_err(|_| StorageError::Overflow { what: "a descriptor limit" })?);
        if let Some(cursor) = after {
            query.and_where(Expr::col(header()).gt(cursor.encode()?));
        }
        let rows = self
            .transaction
            .query_all(DbBackend::Sqlite.build(&query))
            .await?;
        rows.iter()
            .map(|row| {
                let row = row
                    .try_as_sqlite_row()
                    .ok_or(StorageError::Corrupt { what: "a result is not a SQLite row" })?;
                let header: &[u8] = row.try_get("descriptor")?;
                Ok(BinaryDescriptor::decode(header)?)
            })
            .collect()
    }

    fn not_binary(&self, field: &str) -> StorageError {
        StorageError::NotBinary { collection: self.schema.collection.clone(), field: field.to_owned() }
    }

    /// The descriptor column of `field`, when the schema declares it binary.
    fn header_column(&self, field: &str) -> Option<Alias> {
        self.schema
            .fields
            .iter()
            .any(|item| item.name == field && item.field_type == FieldType::Binary)
            .then(|| Alias::new(descriptor_column(field)))
    }

    async fn page(
        &self,
        request: &super::binary::BinaryPageRequest<'_>,
        matching: Option<&[u8]>,
        output: super::binary::BinaryPageOutput<'_>,
    ) -> Result<Option<BinaryDescriptor>, StorageError> {
        use sea_orm::sqlx::Row;
        let super::binary::BinaryPageRequest {
            field,
            after,
            limit,
        } = *request;
        let super::binary::BinaryPageOutput { bytes, ids } = output;
        let collection = &self.schema.collection;
        let header_column = self.header_column(field).ok_or_else(|| self.not_binary(field))?;
        let column = super::relational_query::column(&self.schema, field)?;
        let mut query = Query::select();
        if let Some(header) = matching {
            query.and_where(Expr::col(Alias::new(descriptor_column(field))).eq(header.to_vec()));
        }
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
            .limit(u64::try_from(limit.get()).map_err(|_| StorageError::Overflow { what: "a page size" })?);
        if let Some(after) = after {
            query.and_where(Expr::col(Alias::new("id")).gt(after.to_string()));
        }
        let rows = self
            .transaction
            .query_all(DbBackend::Sqlite.build(&query))
            .await?;
        let mut borrowed = Vec::with_capacity(rows.len());
        let mut first_header: Option<&[u8]> = None;
        let mut descriptor = None;
        let mut stride = 0;
        let mut total = 0_usize;
        for row in &rows {
            let row = row
                .try_as_sqlite_row()
                .ok_or(StorageError::Corrupt { what: "a result is not a SQLite row" })?;
            let id: &str = row.try_get("id")?;
            let payload: Option<&[u8]> = row.try_get("payload")?;
            let header: Option<&[u8]> = row.try_get("descriptor")?;
            let (payload, header) = match (payload, header) {
                (Some(payload), Some(header)) => (payload, header),
                _ => {
                    return Err(StorageError::Corrupt { what: "a binary payload and its descriptor disagree on NULL" })
                }
            };
            match first_header {
                Some(first) if first != header => {
                    return Err(StorageError::MixedProducers)
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
                return Err(StorageError::Corrupt { what: "a stored binary payload disagrees with its descriptor length" });
            }
            total = total
                .checked_add(payload.len())
                .ok_or(StorageError::Overflow { what: "a native page" })?;
            borrowed.push((super::binary::native_identity(id)?, payload));
        }
        bytes.try_reserve(total).map_err(|_| StorageError::Overflow { what: "a native page buffer" })?;
        ids.try_reserve(borrowed.len()).map_err(|_| StorageError::Overflow { what: "a native page id buffer" })?;
        // No await or fallible decoding after the caller buffers start changing.
        for (id, payload) in borrowed {
            bytes.extend_from_slice(payload);
            ids.push(id);
        }
        Ok(descriptor)
    }
}
