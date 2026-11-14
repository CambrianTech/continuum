//! Native payload access through the existing schema and transaction owner.
use super::{
    adapter::naming,
    binary::{descriptor_column, BinaryValue},
    types::FieldType,
};
use sea_orm::sea_query::{Alias, Expr};
use sea_orm::{ConnectionTrait, DbBackend};

impl super::relational::RelationalStore {
    pub async fn create_binary(
        &self,
        record: super::types::DataRecord,
        payloads: std::collections::BTreeMap<String, BinaryValue>,
    ) -> Result<String, String> {
        if payloads.is_empty() {
            return Err("native create requires at least one payload".into());
        }
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
