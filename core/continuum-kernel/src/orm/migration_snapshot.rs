//! Bounded offline transfer through the existing snapshot and declared codecs.
//! Native columns never enter the scalar JSON envelope.
use super::{adapter::naming, binary::{BinaryDescriptor, BinaryValue, descriptor_column}, types::{DataRecord, FieldType}};
use sea_orm::{ConnectionTrait, DbBackend, sea_query::{Alias, Expr, Order, Query}};
use std::{collections::BTreeMap, num::NonZeroUsize};

pub(super) struct MigrationPage<'a> {
    pub after: Option<&'a str>,
    pub limit: NonZeroUsize,
}

pub(super) struct MigrationRow {
    pub record: DataRecord,
    pub payloads: BTreeMap<String, BinaryValue>,
}

impl super::BinarySnapshot {
    pub(super) async fn migration_page(&self, request: &MigrationPage<'_>) -> Result<Vec<MigrationRow>, String> {
        let mut query = Query::select();
        query.from(Alias::new(naming::to_table_name(&self.schema.collection)));
        let mut native = Vec::new();
        let mut omitted = Vec::new();
        for field in &self.schema.fields {
            let column = naming::to_snake_case(&field.name);
            query.column(Alias::new(&column));
            if field.field_type == FieldType::Binary {
                let header = descriptor_column(&field.name);
                query.column(Alias::new(&header));
                omitted.extend([column.clone(), header.clone()]);
                native.push((field, column, header));
            }
        }
        query.order_by(Alias::new("id"), Order::Asc)
            .limit(u64::try_from(request.limit.get()).map_err(|_| "migration page size exceeds u64")?);
        if let Some(after) = request.after {
            query.and_where(Expr::col(Alias::new("id")).gt(after));
        }
        let rows = self.transaction.query_all(DbBackend::Sqlite.build(&query)).await.map_err(|e| e.to_string())?;
        let decoder = super::relational_codec::RowDecoder::new(self.schema.clone(), &omitted);
        let mut output = Vec::with_capacity(rows.len());
        for result in rows {
            use sea_orm::sqlx::Row;
            let row = result.try_as_sqlite_row().ok_or("SQLite migration row required")?;
            let mut payloads = BTreeMap::new();
            for (field, column, header) in &native {
                let bytes: Option<Vec<u8>> = row.try_get(column.as_str()).map_err(|e| e.to_string())?;
                let descriptor: Option<&[u8]> = row.try_get(header.as_str()).map_err(|e| e.to_string())?;
                match (bytes, descriptor) {
                    (Some(bytes), Some(header)) => {
                        let value = BinaryValue::new(BinaryDescriptor::decode(header)?, bytes)?;
                        payloads.insert(field.name.clone(), value);
                    }
                    (None, None) if field.nullable => {}
                    _ => return Err(format!("{}.{} has an incomplete native value", self.schema.collection, field.name)),
                }
            }
            let record = decoder.decode(result)?;
            if !payloads.is_empty() { super::binary::native_identity(&record.id)?; }
            output.push(MigrationRow { record, payloads });
        }
        Ok(output)
    }
}
