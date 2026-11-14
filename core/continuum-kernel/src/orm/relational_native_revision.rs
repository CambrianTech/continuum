//! Native row revisions share the existing transaction and sequence-head owners.
//! This internal custody record is not yet a public delta/replay contract.
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
    Index::create().name("orm_native_revision_order").table(Entity)
        .col(Column::Collection).col(Column::Revision).col(Column::Id)
        .if_not_exists().to_owned()
}

impl super::relational::RelationalStore {
    pub(super) async fn native_changed_on<C: ConnectionTrait>(
        &self, connection: &C, collection: &str, id: &str,
    ) -> Result<i64, String> {
        super::binary::native_identity(id)?;
        let revision = super::relational_sequence::next_on(connection,
            super::relational_sequence::Domain::Native, collection, 1).await?;
        let statement = Query::insert().into_table(Entity)
            .columns([Column::Collection, Column::Id, Column::Revision, Column::Deleted])
            .values([collection.into(), id.into(), revision.into(), false.into()])
            .map_err(|e| e.to_string())?
            .on_conflict(OnConflict::columns([Column::Collection, Column::Id])
                .update_columns([Column::Revision, Column::Deleted]).to_owned()).to_owned();
        connection.execute(connection.get_database_backend().build(&statement)).await.map_err(|e| e.to_string())?;
        Ok(revision)
    }
}