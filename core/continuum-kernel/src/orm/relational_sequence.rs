//! Commit-ordered sequence heads are declared once and advanced inside the write transaction.
use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "orm_sequence_heads")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub domain: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub collection: String,
    pub high: i64,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

impl super::relational::RelationalStore {
    pub async fn append_on<C: ConnectionTrait>(
        &self,
        connection: &C,
        mut record: super::types::DataRecord,
        spec: &super::types::SequenceSpec,
    ) -> Result<super::types::Sequenced, String> {
        use sea_orm::sea_query::{Alias, Expr, Query};
        let schema = self.schema(&record.collection)?;
        let unique_field = schema
            .fields
            .iter()
            .find(|field| field.name == spec.unique)
            .ok_or("append unique field is not declared")?;
        if unique_field.nullable
            || !(unique_field.unique
                || schema
                    .indexes
                    .iter()
                    .any(|index| index.unique && index.fields == [spec.unique.clone()]))
        {
            return Err("append identity requires a declared non-null unique constraint".into());
        }
        let sequence_field = schema
            .fields
            .iter()
            .find(|field| field.name == spec.sequence)
            .ok_or("append sequence field is not declared")?;
        if sequence_field.nullable || sequence_field.field_type != super::types::FieldType::Number {
            return Err("append sequence requires a declared non-null numeric field".into());
        }
        let sequence = super::relational_query::column(&schema, &spec.sequence)?;
        let unique = super::relational_query::column(&schema, &spec.unique)?;
        let unique_value = record
            .data
            .get(&spec.unique)
            .ok_or("append omitted its unique field")?;
        let table_name = super::adapter::naming::to_table_name(&record.collection);
        let table = Alias::new(&table_name);
        let existing = Query::select()
            .column(sequence.clone())
            .from(table.clone())
            .and_where(Expr::col(unique).eq(super::relational_query::value(unique_value)?))
            .limit(1)
            .to_owned();
        if let Some(row) = connection
            .query_one(connection.get_database_backend().build(&existing))
            .await
            .map_err(|e| e.to_string())?
        {
            let seq: i64 = row.try_get_by_index(0).map_err(|e| e.to_string())?;
            return Ok(super::types::Sequenced {
                seq: seq.try_into().map_err(|_| "negative sequence")?,
                fresh: false,
            });
        }
        let seed = match Entity::find_by_id(("append".to_owned(), table_name.clone()))
            .one(connection)
            .await
            .map_err(|e| e.to_string())?
        {
            Some(head) => head.high.checked_add(1).ok_or("sequence exhausted")?,
            None => {
                let max = Query::select()
                    .expr(Expr::col(sequence).max())
                    .from(table)
                    .to_owned();
                let row = connection
                    .query_one(connection.get_database_backend().build(&max))
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or("MAX returned no row")?;
                let max: Option<i64> = row.try_get_by_index(0).map_err(|e| e.to_string())?;
                match max {
                    Some(high) => high.checked_add(1).ok_or("sequence exhausted")?,
                    None => 1,
                }
            }
        };
        let high = next_on(connection, Domain::Append, &table_name, seed).await?;
        record
            .data
            .as_object_mut()
            .ok_or("append requires object data")?
            .insert(spec.sequence.clone(), serde_json::json!(high));
        self.insert(connection, &record).await?;
        Ok(super::types::Sequenced {
            seq: high.try_into().map_err(|_| "negative sequence")?,
            fresh: true,
        })
    }
}

/// Shared durable counter mechanism. Call only inside the existing write transaction.
/// Distinct custody domains supply distinct keys; append sequences retain their keys.
pub(super) async fn next_on<C: ConnectionTrait>(
    connection: &C,
    domain: Domain,
    key: &str,
    seed: i64,
) -> Result<i64, String> {
    use sea_orm::sea_query::{Expr, OnConflict, Query};
    let domain = match domain {
        Domain::Append => "append",
        Domain::Native => "native",
    };
    if seed <= 0 {
        return Err("sequence seed must be positive".into());
    }
    let statement = Query::insert()
        .into_table(Entity)
        .columns([Column::Domain, Column::Collection, Column::High])
        .values([domain.into(), key.into(), seed.into()])
        .map_err(|e| e.to_string())?
        .on_conflict(
            OnConflict::columns([Column::Domain, Column::Collection])
                .value(Column::High, Expr::col(Column::High).add(1))
                .action_and_where(
                    Expr::col(Column::High)
                        .gte(1)
                        .and(Expr::col(Column::High).lt(i64::MAX)),
                )
                .to_owned(),
        )
        .returning_col(Column::High)
        .to_owned();
    let row = connection
        .query_one(connection.get_database_backend().build(&statement))
        .await
        .map_err(|e| e.to_string())?
        .ok_or("sequence exhausted or invalid persisted head")?;
    row.try_get("", "high").map_err(|e| e.to_string())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Domain {
    Append,
    Native,
}
