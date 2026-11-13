//! Mutation statements use the same declared columns and codecs as reads.
use super::{
    adapter::naming,
    query::{QueryBuilder, StorageQuery},
    types::DataRecord,
};
use sea_orm::sea_query::{Alias, Expr, OnConflict, Query};
use sea_orm::{ConnectionTrait, DbBackend};
use serde_json::Value;

impl super::relational::RelationalStore {
    pub async fn read_on<C: ConnectionTrait>(
        &self,
        connection: &C,
        collection: &str,
        id: &str,
    ) -> Result<Option<DataRecord>, String> {
        let schema = self.schema(collection)?;
        let query = QueryBuilder::new(collection)
            .filter_eq("id", id)
            .limit(1)
            .build();
        let statement = super::relational_query::select(&schema, &query, false)?;
        connection
            .query_one(DbBackend::Sqlite.build(&statement))
            .await
            .map_err(|e| e.to_string())?
            .map(|row| super::relational_codec::decode(row, &schema))
            .transpose()
    }

    pub async fn update_on<C: ConnectionTrait>(
        &self,
        connection: &C,
        collection: &str,
        id: &str,
        data: Value,
        increment: bool,
    ) -> Result<bool, String> {
        let schema = self.schema(collection)?;
        let mut statement = Query::update();
        statement
            .table(Alias::new(naming::to_table_name(collection)))
            .and_where(Expr::col(Alias::new("id")).eq(id));
        for (name, value) in data.as_object().ok_or("update requires an object")? {
            if super::types::METADATA_KEYS.contains(&name.as_str()) {
                return Err(format!("metadata {name} is owned by storage"));
            }
            let field = schema
                .fields
                .iter()
                .find(|f| f.name == *name)
                .ok_or_else(|| format!("undeclared update field {collection}.{name}"))?;
            statement.value(
                Alias::new(naming::to_snake_case(name)),
                super::relational_codec::encode(field, value)?,
            );
        }
        statement.value(Alias::new("updated_at"), chrono::Utc::now().to_rfc3339());
        if increment {
            statement.value(
                Alias::new("version"),
                Expr::col(Alias::new("version")).add(1),
            );
        }
        Ok(connection
            .execute(DbBackend::Sqlite.build(&statement))
            .await
            .map_err(|e| e.to_string())?
            .rows_affected()
            != 0)
    }

    pub async fn delete_on<C: ConnectionTrait>(
        &self,
        connection: &C,
        query: StorageQuery,
    ) -> Result<usize, String> {
        if query.joins.is_some() || query.limit.is_some() || query.offset.is_some() {
            return Err("delete_matching accepts a predicate, not joins or pagination".into());
        }
        let schema = self.schema(&query.collection)?;
        let statement = Query::delete()
            .from_table(Alias::new(naming::to_table_name(&query.collection)))
            .cond_where(super::relational_query::condition(&schema, &query)?)
            .to_owned();
        connection
            .execute(DbBackend::Sqlite.build(&statement))
            .await
            .map_err(|e| e.to_string())?
            .rows_affected()
            .try_into()
            .map_err(|_| "affected row count exceeds usize".into())
    }

    pub async fn ensure_on<C: ConnectionTrait>(
        &self,
        connection: &C,
        record: &DataRecord,
    ) -> Result<bool, String> {
        let mut statement = self.insert_statement(record)?;
        statement.on_conflict(OnConflict::column(Alias::new("id")).do_nothing().to_owned());
        Ok(connection
            .execute(DbBackend::Sqlite.build(&statement))
            .await
            .map_err(|e| e.to_string())?
            .rows_affected()
            != 0)
    }

    pub async fn monotonic_on<C: ConnectionTrait>(
        &self,
        connection: &C,
        record: &DataRecord,
        field: &str,
    ) -> Result<bool, String> {
        let schema = self.schema(&record.collection)?;
        let compare = super::relational_query::column(&schema, field)?;
        if !record
            .data
            .as_object()
            .is_some_and(|data| data.contains_key(field))
        {
            return Err(format!("record omitted monotonic field {field}"));
        }
        let table = Alias::new(naming::to_table_name(&record.collection));
        let mut conflict = OnConflict::column(Alias::new("id"));
        conflict.update_columns(
            schema
                .fields
                .iter()
                .filter(|f| !matches!(f.name.as_str(), "id" | "createdAt" | "version"))
                .map(|f| Alias::new(naming::to_snake_case(&f.name))),
        );
        conflict.value(
            Alias::new("version"),
            Expr::col((table.clone(), Alias::new("version"))).add(1),
        );
        conflict.action_and_where(
            Expr::col((table, compare.clone())).lte(Expr::col((Alias::new("excluded"), compare))),
        );
        let mut statement = self.insert_statement(record)?;
        statement.on_conflict(conflict);
        Ok(connection
            .execute(DbBackend::Sqlite.build(&statement))
            .await
            .map_err(|e| e.to_string())?
            .rows_affected()
            != 0)
    }
}
