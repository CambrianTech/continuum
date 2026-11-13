//! Native payload access through the existing schema and transaction owner.
use super::{adapter::naming, binary::{BinaryDescriptor, BinaryRecord, BinaryValue, descriptor_column}, types::FieldType};
use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm::sea_query::{Alias, Expr, Order, Query};

impl super::relational::RelationalStore {
    pub async fn create_binary(&self, record: super::types::DataRecord,
        payloads: std::collections::BTreeMap<String, BinaryValue>) -> Result<String, String> {
        // Compile typed bindings before dispatch, then move them into the same committing owner.
        let statement = self.insert_statement_with_binary(&record, payloads)?;
        self.write(move |_store, transaction| Box::pin(async move {
            transaction.execute(DbBackend::Sqlite.build(&statement)).await.map_err(|e| e.to_string())?;
            Ok(record.id)
        })).await
    }

    pub(super) fn binary_column(&self, collection: &str, field: &str) -> Result<Alias, String> {
        let schema = self.schema(collection)?;
        let declared = schema.fields.iter().find(|f| f.name == field).ok_or("undeclared binary field")?;
        if declared.field_type != FieldType::Binary { return Err("native payload access requires a declared binary field".into()); }
        super::relational_query::column(&schema, field)
    }

    pub async fn binary_page(&self, collection: &str, field: &str, after: Option<String>, limit: usize) -> Result<Vec<BinaryRecord>, String> {
        if limit == 0 { return Err("binary page size must be positive".into()); }
        let column = self.binary_column(collection, field)?;
        let mut query = Query::select();
        query.from(Alias::new(naming::to_table_name(collection))).column(Alias::new("id"))
            .expr_as(Expr::col(column), Alias::new("payload"))
            .expr_as(Expr::col(Alias::new(descriptor_column(field))), Alias::new("descriptor"))
            .order_by(Alias::new("id"), Order::Asc).limit(u64::try_from(limit).map_err(|_| "page size exceeds u64")?);
        if let Some(after) = after { query.and_where(Expr::col(Alias::new("id")).gt(after)); }
        let rows = self.db.query_all(DbBackend::Sqlite.build(&query)).await.map_err(|e| e.to_string())?;
        rows.into_iter().map(|row| {
            let id: String = row.try_get("", "id").map_err(|e| e.to_string())?;
            let bytes: Option<Vec<u8>> = row.try_get("", "payload").map_err(|e| e.to_string())?;
            let descriptor: Option<Vec<u8>> = row.try_get("", "descriptor").map_err(|e| e.to_string())?;
            let value = match (bytes, descriptor) {
                (None, None) => None,
                (Some(bytes), Some(header)) => Some(BinaryValue::new(BinaryDescriptor::decode(&header)?, bytes)?),
                _ => return Err("binary payload and descriptor have inconsistent NULL state".into()),
            };
            Ok(BinaryRecord { id, value })
        }).collect()
    }

    pub async fn update_binary(&self, collection: String, id: String, field: String, value: BinaryValue, condition: super::binary::BinaryWriteCondition) -> Result<bool, String> {
        let column = self.binary_column(&collection, &field)?;
        let (descriptor, bytes) = value.into_parts();
        let header = descriptor.encode()?;
        self.write(move |store, transaction| Box::pin(async move {
            let predicate = match condition {
                super::binary::BinaryWriteCondition::Replace => sea_orm::Condition::all(),
                super::binary::BinaryWriteCondition::IfAbsent => sea_orm::Condition::all().add(Expr::col(column.clone()).is_null()),
            };
            let fields = vec![(column, bytes.into()), (Alias::new(descriptor_column(&field)), header.into())];
            let changed = store.update_fields_matching_on(transaction, &collection, &id, fields, true, predicate).await?;
            if !changed && condition == super::binary::BinaryWriteCondition::Replace {
                return Err(format!("binary row {collection}:{id} does not exist"));
            }
            Ok(changed)
        })).await
    }
}
