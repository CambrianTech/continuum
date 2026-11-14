//! Compile the existing entity declarations into SeaQuery schema statements.
//! Rust derives and generated decorator schemas share this translation.
use std::collections::BTreeSet;

use sea_orm::sea_query::{
    Alias, Expr, Func, IndexOrder, ColumnDef, ForeignKey, ForeignKeyAction, Index, IndexCreateStatement, Table,
    TableCreateStatement,
};

use super::adapter::naming;
use super::types::{CascadeRule, CollectionSchema, FieldType};

pub(super) struct DeclaredTable {
    pub create: TableCreateStatement,
    pub indexes: Vec<IndexCreateStatement>,
}

impl DeclaredTable {
    pub(super) fn from_entity<E: sea_orm::EntityTrait>(entity: E) -> Self {
        let schema = sea_orm::Schema::new(sea_orm::DbBackend::Sqlite);
        Self { create: schema.create_table_from_entity(entity), indexes: schema.create_index_from_entity(entity) }
    }
}

fn action(rule: CascadeRule) -> ForeignKeyAction {
    match rule {
        CascadeRule::Restrict => ForeignKeyAction::Restrict,
        CascadeRule::Cascade => ForeignKeyAction::Cascade,
        CascadeRule::SetNull => ForeignKeyAction::SetNull,
        CascadeRule::NoAction => ForeignKeyAction::NoAction,
    }
}

pub(super) fn compile(schema: &CollectionSchema) -> Result<DeclaredTable, String> {
    if schema.collection.is_empty() {
        return Err("entity collection is empty".into());
    }
    let table = naming::to_table_name(&schema.collection);
    let mut columns = BTreeSet::new();
    let mut create = Table::create();
    // Admission already verified a new declaration; collisions must fail, not skip DDL.
    create.table(Alias::new(&table));
    let mut indexes = Vec::new();
    for field in &schema.fields {
        let name = naming::to_snake_case(&field.name);
        if name.is_empty() || !columns.insert(name.clone()) {
            return Err(format!(
                "{}: empty or duplicate column {}",
                schema.collection, field.name
            ));
        }
        let mut column = ColumnDef::new(Alias::new(&name));
        match field.field_type {
            FieldType::String | FieldType::Uuid | FieldType::Date | FieldType::Json => {
                column.text();
            }
            FieldType::Binary => { column.binary(); }
            FieldType::Boolean => {
                column.boolean();
            }
            // SQLite NUMERIC affinity preserves integral storage, unlike REAL.
            FieldType::Number => {
                column.custom(Alias::new("NUMERIC"));
            }
        }
        if !field.nullable {
            column.not_null();
        }
        if name == "id" {
            column.primary_key();
        }
        if field.unique {
            column.unique_key();
        }
        if let Some(default) = &field.default_value {
            column.default(super::relational_codec::encode(field, default)?);
        }
        if let Some(max) = field.max_length {
            if !matches!(field.field_type, FieldType::String | FieldType::Uuid | FieldType::Date) {
                return Err(format!("{}.{}: max_length requires a text field", schema.collection, field.name));
            }
            let max = i64::try_from(max).map_err(|_| "max_length exceeds integer range")?;
            column.check(Expr::expr(Func::cust(Alias::new("length")).arg(Expr::col(Alias::new(&name)))).lte(max));
        }
        create.col(&mut column);
        if field.field_type == FieldType::Binary {
            let descriptor_name = super::binary::descriptor_column(&field.name);
            if !columns.insert(descriptor_name.clone()) { return Err("binary descriptor column collision".into()); }
            let mut descriptor = ColumnDef::new(Alias::new(&descriptor_name));
            descriptor.binary();
            if !field.nullable { descriptor.not_null(); }
            descriptor.check(Expr::expr(Expr::col(Alias::new(&descriptor_name)).is_null())
                .eq(Expr::col(Alias::new(&name)).is_null()));
            create.col(&mut descriptor);
        }
        if let Some(reference) = &field.foreign_key {
            if !field.nullable
                && (reference.on_delete == CascadeRule::SetNull
                    || reference.on_update == CascadeRule::SetNull)
            {
                return Err(format!(
                    "{}.{}: SET NULL requires a nullable field",
                    schema.collection, field.name
                ));
            }
            create.foreign_key(
                ForeignKey::create()
                    .from(Alias::new(&table), Alias::new(&name))
                    .to(
                        Alias::new(naming::to_table_name(&reference.collection)),
                        Alias::new(naming::to_snake_case(&reference.field)),
                    )
                    .on_delete(action(reference.on_delete))
                    .on_update(action(reference.on_update)),
            );
        }
        if field.indexed && name != "id" {
            indexes.push(
                Index::create()
                    .name(format!("idx_{table}_{name}"))
                    .table(Alias::new(&table))
                    .col(Alias::new(&name))
                    .to_owned(),
            );
        }
    }
    if !columns.contains("id") {
        return Err(format!("{}: entity has no id", schema.collection));
    }
    for declared in &schema.indexes {
        if declared.fields.is_empty() {
            return Err(format!(
                "{}: index {} has no fields",
                schema.collection, declared.name
            ));
        }
        let mut index = Index::create();
        index
            .name(&declared.name)
            .table(Alias::new(&table));
        for (position, field) in declared.fields.iter().enumerate() {
            let name = naming::to_snake_case(field);
            if !columns.contains(&name) {
                return Err(format!(
                    "{}: index {} names undeclared field {field}",
                    schema.collection, declared.name
                ));
            }
            let order = if declared.descending && position + 1 == declared.fields.len() { IndexOrder::Desc } else { IndexOrder::Asc };
            index.col((Alias::new(name), order));
        }
        if declared.unique {
            index.unique();
        }
        indexes.push(index);
    }
    Ok(DeclaredTable { create, indexes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::{base_entity_fields, ForeignKeyRef, SchemaField, SchemaIndex};
    use sea_orm::{ConnectionTrait, Database, DbBackend};

    #[tokio::test]
    async fn declarations_enforce_relationships_nullability_and_unique_indexes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::connect("sqlite::memory:").await?;
        let parent = CollectionSchema {
            collection: "schema_parent".into(),
            fields: base_entity_fields(),
            indexes: vec![],
        };
        let mut child = CollectionSchema {
            collection: "schema_child".into(),
            fields: base_entity_fields(),
            indexes: vec![],
        };
        child.fields.push(SchemaField {
                default_value: None,
            name: "parentId".into(),
            field_type: FieldType::Uuid,
            indexed: true,
            unique: false,
            nullable: false,
            max_length: None,
            foreign_key: Some(ForeignKeyRef {
                collection: parent.collection.clone(),
                field: "id".into(),
                on_delete: CascadeRule::Cascade,
                on_update: CascadeRule::Restrict,
            }),
        });
        child.indexes.push(SchemaIndex {
                descending: false,
            name: "unique_parent".into(),
            fields: vec!["parentId".into()],
            unique: true,
        });
        for schema in [&parent, &child] {
            let table = compile(schema)?;
            db.execute(DbBackend::Sqlite.build(&table.create)).await?;
            for index in table.indexes {
                db.execute(DbBackend::Sqlite.build(&index)).await?;
            }
        }
        use sea_orm::sea_query::Query;
        let insert = |table: &str, id: &str, parent: Option<&str>| {
            let mut q = Query::insert();
            q.into_table(Alias::new(table));
            let mut cols = vec![
                Alias::new("id"),
                Alias::new("created_at"),
                Alias::new("updated_at"),
                Alias::new("version"),
            ];
            let mut vals: Vec<sea_orm::sea_query::SimpleExpr> =
                vec![id.into(), "now".into(), "now".into(), 1.into()];
            if let Some(parent) = parent {
                cols.push(Alias::new("parent_id"));
                vals.push(parent.into());
            }
            q.columns(cols).values(vals)?;
            Ok::<_, sea_orm::sea_query::error::Error>(DbBackend::Sqlite.build(&q))
        };
        assert!(db
            .execute(insert("schema_child", "orphan", Some("absent"))?)
            .await
            .is_err());
        db.execute(insert("schema_parent", "p", None)?).await?;
        db.execute(insert("schema_child", "c", Some("p"))?).await?;
        assert!(db
            .execute(insert("schema_child", "duplicate", Some("p"))?)
            .await
            .is_err());
        assert!(db
            .execute(insert("schema_child", "null", None)?)
            .await
            .is_err());
        child.indexes[0].fields = vec!["notDeclared".into()];
        assert!(compile(&child).is_err());
        Ok(())
    }
}
