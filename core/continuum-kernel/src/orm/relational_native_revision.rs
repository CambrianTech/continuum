//! Native row revisions share the existing transaction and sequence-head owners.
//! This internal custody record is not yet a public delta/replay contract.
use super::storage_error::StorageError;
use sea_orm::entity::prelude::*;
use sea_orm::sea_query::{Index, IndexCreateStatement, OnConflict, Query};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "orm_native_revisions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub collection: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub revision: i64,
    pub deleted: bool,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

pub(super) fn revision_index() -> IndexCreateStatement {
    Index::create()
        .name("orm_native_revision_order")
        .table(Entity)
        .col(Column::Collection)
        .col(Column::Revision)
        .col(Column::Id)
        .if_not_exists()
        .to_owned()
}

impl super::relational::RelationalStore {
    pub(super) async fn native_changed_on<C: ConnectionTrait>(
        &self,
        connection: &C,
        collection: &str,
        id: &str,
    ) -> Result<super::relational_transaction::Revision, StorageError> {
        super::binary::native_identity(id)?;
        let revision = super::relational_sequence::next_on(
            connection,
            super::relational_sequence::Domain::Native,
            collection,
            1,
        )
        .await?;
        let statement = Query::insert()
            .into_table(Entity)
            .columns([
                Column::Collection,
                Column::Id,
                Column::Revision,
                Column::Deleted,
            ])
            .values([collection.into(), id.into(), revision.into(), false.into()])?
            .on_conflict(
                OnConflict::columns([Column::Collection, Column::Id])
                    .update_columns([Column::Revision, Column::Deleted])
                    .to_owned(),
            )
            .to_owned();
        connection
            .execute(connection.get_database_backend().build(&statement))
            .await?;
        Ok(super::relational_transaction::Revision {
            domain: super::relational_sequence::Domain::Native,
            collection: collection.into(),
            high: revision
                .try_into()
                .map_err(|_| StorageError::Corrupt { what: "a native revision is negative" })?,
        })
    }
}
impl super::relational::RelationalStore {
    /// Record the declared cascade closure before physical DELETE, in its transaction.
    pub(super) async fn native_deleting_on(
        &self,
        connection: &sea_orm::DatabaseTransaction,
        schema: &super::types::CollectionSchema,
        query: &super::query::StorageQuery,
    ) -> Result<Vec<super::relational_transaction::Revision>, StorageError> {
        use sea_orm::sea_query::{Alias, Expr};
        use sea_orm::PaginatorTrait;
        let catalog_count = super::relational_declaration::Entity::find()
            .count(connection)
            .await?;
        if catalog_count != self.collections().len() as u64 {
            return Err(StorageError::Corrupt { what: "the schema catalog changed outside its owner; admit every declaration before deleting" });
        }
        let (graph, native) = self.deletion_graph(schema, query)?;
        let mut revisions = Vec::new();
        for collection in native {
            let victims = Query::select()
                .column(Alias::new("id"))
                .from(Alias::new(super::relational_delete_graph::VICTIMS))
                .and_where(Expr::col(Alias::new("collection")).eq(collection.as_str()))
                .with_cte(graph.clone())
                .to_owned();
            let condition = sea_orm::Condition::all()
                .add(Expr::col(Column::Collection).eq(collection.as_str()))
                .add(Expr::col(Column::Deleted).eq(false))
                .add(Expr::col(Column::Id).in_subquery(victims));
            // Probe one matching receipt, not row bodies or a materialized victim list.
            let probe = Query::select()
                .column(Column::Id)
                .from(Entity)
                .cond_where(condition.clone())
                .limit(1)
                .to_owned();
            if connection
                .query_one(connection.get_database_backend().build(&probe))
                .await?
                .is_none()
            {
                continue;
            }
            let revision = super::relational_sequence::next_on(
                connection,
                super::relational_sequence::Domain::Native,
                &collection,
                1,
            )
            .await?;
            let statement = Query::update()
                .table(Entity)
                .value(Column::Revision, revision)
                .value(Column::Deleted, true)
                .cond_where(condition)
                .to_owned();
            connection
                .execute(connection.get_database_backend().build(&statement))
                .await?;
            revisions.push(super::relational_transaction::Revision {
                domain: super::relational_sequence::Domain::Native,
                collection,
                high: revision
                    .try_into()
                    .map_err(|_| StorageError::Corrupt { what: "a native revision is negative" })?,
            });
        }
        Ok(revisions)
    }
}
