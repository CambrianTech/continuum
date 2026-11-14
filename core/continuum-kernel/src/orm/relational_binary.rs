//! Native payload access through the existing schema and transaction owner.
use super::{
    adapter::naming,
    binary::{descriptor_column, BinaryDescriptor, BinaryValue},
    types::FieldType,
};
use sea_orm::sea_query::{Alias, Expr, Order, Query};
use sea_orm::{ConnectionTrait, DbBackend};

impl super::relational::RelationalStore {
    pub async fn create_binary(
        &self,
        record: super::types::DataRecord,
        payloads: std::collections::BTreeMap<String, BinaryValue>,
    ) -> Result<String, String> {
        // Compile typed bindings before dispatch, then move them into the same committing owner.
        super::binary::native_identity(&record.id)?;
        let statement = self.insert_statement_with_binary(&record, payloads)?;
        self.write_with_revisions(move |store, transaction| {
            Box::pin(async move {
                transaction
                    .execute(DbBackend::Sqlite.build(&statement))
                    .await
                    .map_err(|e| e.to_string())?;
                let revision = store
                    .native_changed_on(transaction, &record.collection, &record.id)
                    .await?;
                Ok((record.id, vec![revision]))
            })
        })
        .await
    }

    pub(super) fn binary_column(&self, collection: &str, field: &str) -> Result<Alias, String> {
        let schema = self.schema(collection)?;
        let declared = schema
            .fields
            .iter()
            .find(|f| f.name == field)
            .ok_or("undeclared binary field")?;
        if declared.field_type != FieldType::Binary {
            return Err("native payload access requires a declared binary field".into());
        }
        super::relational_query::column(&schema, field)
    }

    /// One provider-to-caller bulk copy per row; no per-row payload allocation.
    /// NULL payloads are selected out before LIMIT. Every row is validated before
    /// output mutation, so refusal or cancellation leaves both buffers unchanged.
    /// This is an ID-ordered live page, not a revision-watermarked snapshot.
    pub async fn binary_page_into(
        &self,
        collection: &str,
        field: &str,
        after: Option<uuid::Uuid>,
        limit: usize,
        bytes: &mut Vec<u8>,
        ids: &mut Vec<uuid::Uuid>,
    ) -> Result<Option<BinaryDescriptor>, String> {
        use sea_orm::sqlx::Row;
        if limit == 0 {
            return Err("binary page size must be positive".into());
        }
        let column = self.binary_column(collection, field)?;
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
            .limit(u64::try_from(limit).map_err(|_| "page size exceeds u64")?);
        if let Some(after) = after {
            query.and_where(Expr::col(Alias::new("id")).gt(after.to_string()));
        }
        let rows = self
            .db
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
    pub async fn update_binary(
        &self,
        collection: String,
        id: String,
        payloads: std::collections::BTreeMap<String, BinaryValue>,
        condition: super::binary::BinaryWriteCondition,
    ) -> Result<bool, String> {
        super::binary::native_identity(&id)?;
        if payloads.is_empty() {
            return Err("native update requires at least one payload".into());
        }
        let mut fields = Vec::with_capacity(payloads.len() * 2);
        let mut predicate = sea_orm::Condition::all();
        for (field, value) in payloads {
            let column = self.binary_column(&collection, &field)?;
            let header_column = Alias::new(descriptor_column(&field));
            if condition == super::binary::BinaryWriteCondition::IfAbsent {
                predicate = predicate
                    .add(Expr::col(column.clone()).is_null())
                    .add(Expr::col(header_column.clone()).is_null());
            }
            let (descriptor, bytes) = value.into_parts();
            fields.push((column, bytes.into()));
            fields.push((header_column, descriptor.encode()?.into()));
        }
        self.write_with_revisions(move |store, transaction| {
            Box::pin(async move {
                let changed = store
                    .update_fields_matching_on(
                        transaction,
                        &collection,
                        &id,
                        fields,
                        true,
                        predicate,
                    )
                    .await?;
                if !changed && condition == super::binary::BinaryWriteCondition::Replace {
                    return Err(format!("binary row {collection}:{id} does not exist"));
                }
                let revisions = if changed {
                    vec![
                        store
                            .native_changed_on(transaction, &collection, &id)
                            .await?,
                    ]
                } else {
                    Vec::new()
                };
                Ok((changed, revisions))
            })
        })
        .await
    }
}
