//! What each declared collection would take to match its store, read and planned, never applied
//! (ONE-FRAME-ONE-MEMORY.md, "The durable store", step 0a).
//!
//! The 5090's control build failed five modules on 2026-11-27 because its `training_trigger_submissions`
//! held columns no one had decided on: the adapter evolves a table on write (`evolve_table_schema`), so a
//! build that writes a field, merged or not, changes what every later build opens. Before that path is
//! replaced by explicit migrations, this plan says what each node's store holds against what the code
//! declares: the inventory that gates the deletion, and the same diff the migrator will apply.
//!
//! Types are compared by SQLite affinity, never by their declared text: a table created from a row
//! (`infer_sqlite_type`: INTEGER, REAL) and one created from its schema (`sql_column_type`: NUMERIC) hold
//! the same numbers. A column whose affinity differs from its declaration is drift, reported. It is
//! never refused here, because live tables already carry such columns (`staged_credit.rejected_at_ms`
//! as TEXT, 2026-10-13) and serve.

use super::*;
use crate::orm::{SchemaField, SchemaIndex};

/// A column as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalColumn {
    pub name: String,
    /// The type the table was declared with, verbatim (empty for a column declared without one).
    pub declared_type: String,
    pub not_null: bool,
}

/// A table as the store holds it: its columns in order and the names of its indexes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PhysicalTable {
    pub columns: Vec<PhysicalColumn>,
    pub indexes: Vec<String>,
}

/// SQLite's type affinity (its own rules, section 3.1 of its datatype document).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/orm/Affinity.ts")]
#[serde(rename_all = "camelCase")]
pub enum Affinity {
    Text,
    Numeric,
    Blob,
}

impl Affinity {
    /// The affinity SQLite gives a declared column type. INTEGER, REAL and NUMERIC are one family: each
    /// stores a number as a number.
    pub fn of(declared_type: &str) -> Self {
        let upper = declared_type.to_ascii_uppercase();
        if upper.contains("CHAR") || upper.contains("CLOB") || upper.contains("TEXT") {
            Affinity::Text
        } else if upper.is_empty() || upper.contains("BLOB") {
            Affinity::Blob
        } else {
            Affinity::Numeric
        }
    }
}

/// One difference between a declared collection and its table.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/orm/PlanItem.ts")]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PlanItem {
    /// The store has no table for the collection: nothing was written yet.
    CreateTable,
    /// A declared column the table lacks. Additive when the declaration allows it to be absent.
    AddColumn { column: String, field_type: FieldType, required: bool },
    /// A declared index the table lacks, over columns it has (or gains in this plan).
    AddIndex { index: String, columns: Vec<String>, unique: bool },
    /// A declared index over a column the declaration does not name and the table does not hold: it can
    /// never be built, so it is a defect of the declaration, reported and never planned as an addition.
    IndexUnbuildable { index: String, missing: Vec<String> },
    /// A column the table holds that no declaration names: written by a newer or another build.
    UndeclaredColumn { column: String },
    /// A column whose affinity differs from its declaration's.
    TypeDrift { column: String, declared: Affinity, physical: Affinity },
}

/// How a collection's store stands against its declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/orm/Standing.ts")]
#[serde(rename_all = "camelCase")]
pub enum Standing {
    /// Nothing to do.
    Current,
    /// Only additions: a table, nullable columns, indexes. The migrator applies these at open.
    Additive,
    /// The store holds more than the declaration: columns a newer or other build wrote. Served as is.
    Newer,
    /// A declared required column is missing: no addition can make it hold the declaration.
    Breaking,
}

/// One collection's plan.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/orm/CollectionPlan.ts")]
#[serde(rename_all = "camelCase")]
pub struct CollectionPlan {
    pub collection: String,
    pub table: String,
    /// The worst of its items: a breaking one decides, then additions, then a newer store.
    pub standing: Standing,
    pub items: Vec<PlanItem>,
}

impl CollectionPlan {
    fn new(schema: &CollectionSchema, table: String, items: Vec<PlanItem>) -> Self {
        let standing = standing_of(&items);
        Self { collection: schema.collection.clone(), table, standing, items }
    }

    pub fn standing(&self) -> Standing {
        self.standing
    }
}

fn standing_of(items: &[PlanItem]) -> Standing {
    let breaking = items.iter().any(|item| matches!(item, PlanItem::AddColumn { required: true, .. }));
    let additive = items.iter().any(|item| matches!(item, PlanItem::CreateTable | PlanItem::AddColumn { .. } | PlanItem::AddIndex { .. }));
    let newer = items.iter().any(|item| matches!(item, PlanItem::UndeclaredColumn { .. }));
    match (breaking, additive, newer) {
        (true, _, _) => Standing::Breaking,
        (false, true, _) => Standing::Additive,
        (false, false, true) => Standing::Newer,
        (false, false, false) => Standing::Current,
    }
}

/// PURE: what `schema` would take to match `physical` (`None`: the table does not exist).
pub fn plan(schema: &CollectionSchema, physical: Option<&PhysicalTable>) -> CollectionPlan {
    let table = naming::to_table_name(&schema.collection);
    let declared: Vec<(String, &SchemaField)> = schema
        .fields
        .iter()
        .map(|field| (naming::to_snake_case(&field.name), field))
        .filter(|(column, _)| !super::super::entity::is_base_entity_column(column))
        .collect();
    let indexes = declared_indexes(schema, &table);
    let Some(physical) = physical else {
        let mut items = vec![PlanItem::CreateTable];
        items.extend(indexes.into_iter().map(|(index, columns, unique)| PlanItem::AddIndex { index, columns, unique }));
        return CollectionPlan::new(schema, table, items);
    };
    let held = |column: &str| physical.columns.iter().find(|c| c.name == column);
    let mut items = Vec::new();
    for (column, field) in &declared {
        match held(column) {
            None => items.push(PlanItem::AddColumn { column: column.to_string(), field_type: field.field_type.clone(), required: !field.nullable }),
            Some(found) => {
                let declared_affinity = Affinity::of(sql_column_type(&field.field_type));
                let physical_affinity = Affinity::of(&found.declared_type);
                if declared_affinity != physical_affinity {
                    items.push(PlanItem::TypeDrift { column: column.to_string(), declared: declared_affinity, physical: physical_affinity });
                }
            }
        }
    }
    for column in &physical.columns {
        let declared_here = declared.iter().any(|(name, _)| *name == column.name);
        if !declared_here && !super::super::entity::is_base_entity_column(&column.name) {
            items.push(PlanItem::UndeclaredColumn { column: column.name.clone() });
        }
    }
    for (index, columns, unique) in indexes {
        if physical.indexes.contains(&index) {
            continue;
        }
        // an index can be built only over columns the table holds or this plan adds
        let missing: Vec<String> = columns
            .iter()
            .filter(|column| held(column).is_none() && !declared.iter().any(|(name, _)| name == *column) && !super::super::entity::is_base_entity_column(column))
            .cloned()
            .collect();
        items.push(match missing.is_empty() {
            true => PlanItem::AddIndex { index, columns, unique },
            false => PlanItem::IndexUnbuildable { index, missing },
        });
    }
    CollectionPlan::new(schema, table, items)
}

/// The indexes `schema` declares on `table`, named as `do_ensure_indexes` names them.
fn declared_indexes(schema: &CollectionSchema, table: &str) -> Vec<(String, Vec<String>, bool)> {
    let single = schema.fields.iter().filter(|f| f.indexed).filter_map(|f| {
        let column = naming::to_snake_case(&f.name);
        (!super::super::entity::is_base_entity_column(&column)).then(|| (format!("idx_{table}_{column}"), vec![column], false))
    });
    let composite = schema.indexes.iter().map(|index| (naming::to_snake_case(&index.name), index.fields.iter().map(|f| naming::to_snake_case(f)).collect(), index.unique));
    single.chain(composite).collect()
}

/// Why a store's tables could not be read.
#[derive(Debug, thiserror::Error)]
pub enum SchemaRead {
    #[error("reading {table}: {source}")]
    Sql { table: String, #[source] source: rusqlite::Error },
}

/// `table` as `conn` holds it, or `None` when it does not exist.
pub(super) fn read_physical(conn: &Connection, table: &str) -> Result<Option<PhysicalTable>, SchemaRead> {
    let sql = |source| SchemaRead::Sql { table: table.to_owned(), source };
    let mut columns_stmt = conn.prepare(&format!("PRAGMA table_info({table})")).map_err(sql)?;
    let columns = columns_stmt
        .query_map([], |row| Ok(PhysicalColumn { name: row.get(1)?, declared_type: row.get(2)?, not_null: row.get::<_, i64>(3)? != 0 }))
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    if columns.is_empty() {
        return Ok(None);
    }
    let mut index_stmt = conn.prepare(&format!("PRAGMA index_list({table})")).map_err(sql)?;
    let indexes = index_stmt.query_map([], |row| row.get::<_, String>(1)).map_err(sql)?.collect::<Result<Vec<_>, _>>().map_err(sql)?;
    Ok(Some(PhysicalTable { columns, indexes }))
}

/// Plan `schema` against the table `conn` holds now, before anything changes it, and keep the plan in
/// `planned`: what the store held when it was opened for this collection. A table that cannot be read
/// is probed and leaves no plan, so the report never says current for a store it could not see.
pub(super) fn record(planned: &parking_lot::RwLock<Vec<CollectionPlan>>, conn: &Connection, schema: &CollectionSchema) {
    let table = naming::to_table_name(&schema.collection);
    match read_physical(conn, &table) {
        Ok(physical) => {
            let planned_now = plan(schema, physical.as_ref());
            let standing = planned_now.standing();
            if standing != Standing::Current {
                continuum_base::probe!(class = "orm.schema.plan", collection = schema.collection.as_str(), standing = ?standing, items = planned_now.items.len() as u64,
                    "a declared collection's store differs from its declaration at open");
            }
            // one plan per collection: a store reopened for it keeps the latest
            let mut planned = planned.write();
            match planned.iter_mut().find(|held| held.collection == schema.collection) {
                Some(held) => *held = planned_now,
                None => planned.push(planned_now),
            }
        }
        Err(why) => continuum_base::probe!(class = "orm.schema.unread", collection = schema.collection.as_str(), error = %why,
            "a declared collection's table could not be read to plan it"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, field_type: FieldType, nullable: bool) -> SchemaField {
        SchemaField { name: name.into(), field_type, indexed: false, unique: false, nullable, max_length: None, foreign_key: None }
    }

    fn column(name: &str, declared_type: &str) -> PhysicalColumn {
        PhysicalColumn { name: name.into(), declared_type: declared_type.into(), not_null: false }
    }

    fn submissions() -> CollectionSchema {
        CollectionSchema {
            collection: "training_trigger_submissions".into(),
            fields: vec![field("sequence", FieldType::Number, false), field("isPending", FieldType::Boolean, false), field("carriedFrom", FieldType::Uuid, true)],
            indexes: vec![SchemaIndex { name: "idx_training_submission_pending".into(), fields: vec!["isPending".into(), "sequence".into()], unique: false }],
        }
    }

    // what this catches (the 5090, 2026-11-27: control 2135 opened a table with four columns it did not
    // declare and failed five modules): a store holding a column no declaration names, read as anything
    // but Newer, so a rollback build would refuse a store that only gained columns.
    #[test]
    fn a_column_no_declaration_names_is_a_newer_store_never_breaking() {
        let physical = PhysicalTable {
            columns: vec![column("id", "TEXT"), column("sequence", "INTEGER"), column("is_pending", "INTEGER"), column("carried_from", "TEXT"), column("example_count", "INTEGER")],
            indexes: vec!["idx_training_submission_pending".into()],
        };
        let planned = plan(&submissions(), Some(&physical));
        assert_eq!(planned.items, vec![PlanItem::UndeclaredColumn { column: "example_count".into() }]);
        assert_eq!(planned.standing(), Standing::Newer);
    }

    // what this catches: a declared required column the store lacks read as additive (it can never be
    // added to existing rows without a value), or a nullable one read as breaking (it always can).
    #[test]
    fn a_missing_required_column_is_breaking_and_a_missing_nullable_one_additive() {
        let physical = PhysicalTable { columns: vec![column("id", "TEXT"), column("sequence", "INTEGER"), column("is_pending", "INTEGER")], indexes: vec!["idx_training_submission_pending".into()] };
        let planned = plan(&submissions(), Some(&physical));
        assert_eq!(planned.items, vec![PlanItem::AddColumn { column: "carried_from".into(), field_type: FieldType::Uuid, required: false }]);
        assert_eq!(planned.standing(), Standing::Additive);

        let physical = PhysicalTable { columns: vec![column("id", "TEXT"), column("is_pending", "INTEGER"), column("carried_from", "TEXT")], indexes: vec!["idx_training_submission_pending".into()] };
        assert_eq!(plan(&submissions(), Some(&physical)).standing(), Standing::Breaking);
    }

    // what this catches: types compared as text, so a table created from a row (INTEGER) and one from
    // its schema (NUMERIC) read as drift, or a TEXT column holding a declared number read as current.
    #[test]
    fn types_compare_by_affinity_and_a_text_number_is_drift() {
        assert_eq!(Affinity::of("INTEGER"), Affinity::of("NUMERIC"));
        assert_eq!(Affinity::of("REAL"), Affinity::Numeric);
        let physical = PhysicalTable { columns: vec![column("id", "TEXT"), column("sequence", "TEXT"), column("is_pending", "INTEGER"), column("carried_from", "TEXT")], indexes: vec!["idx_training_submission_pending".into()] };
        let planned = plan(&submissions(), Some(&physical));
        assert_eq!(planned.items, vec![PlanItem::TypeDrift { column: "sequence".into(), declared: Affinity::Numeric, physical: Affinity::Text }]);
        assert_eq!(planned.standing(), Standing::Current, "drift is reported, never refused or migrated here");
    }

    // what this catches (the 5090's control build, 2026-11-27): a store whose table holds a column a newer
    // build wrote, opened by a build that does not declare it, reported as anything but Newer, or the
    // plan taken after the open changed the table instead of before.
    #[tokio::test]
    async fn opening_a_store_records_its_plan_before_changing_it() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let newer = CollectionSchema { fields: vec![field("sequence", FieldType::Number, false), field("isPending", FieldType::Boolean, false), field("carriedFrom", FieldType::Uuid, true), field("exampleCount", FieldType::Number, true)], ..submissions() };
        assert!(adapter.ensure_schema(newer).await.success, "test: the newer build creates its table");
        let first = adapter.schema_report();
        assert_eq!(first.iter().map(|p| p.standing()).collect::<Vec<_>>(), [Standing::Additive], "a fresh store plans the create");

        assert!(adapter.ensure_schema(submissions()).await.success, "test: the older build opens the same table");
        let planned = adapter.schema_report();
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].standing(), Standing::Newer);
        assert!(planned[0].items.contains(&PlanItem::UndeclaredColumn { column: "example_count".into() }), "the column the older build does not declare");
    }

    // what this catches (the 5090's control build 2135, 2026-11-27, Codex): one collection's mismatch
    // failing another collection's open: the inbound chat store failed to attach because
    // training_trigger_submissions held columns its build did not declare, even when only chat_messages
    // was being ensured. Each collection is planned and opened alone; a newer store for one never
    // touches another's open or its plan.
    #[tokio::test]
    async fn one_collections_newer_table_never_fails_another_collections_open() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let newer = CollectionSchema { fields: vec![field("sequence", FieldType::Number, false), field("isPending", FieldType::Boolean, false), field("carriedFrom", FieldType::Uuid, true), field("exampleCount", FieldType::Number, true)], ..submissions() };
        assert!(adapter.ensure_schema(newer).await.success, "test: a newer build wrote the submissions table");
        assert!(adapter.ensure_schema(submissions()).await.success, "the older build still opens submissions");

        let chat = CollectionSchema { collection: "chat_messages".into(), fields: vec![field("roomId", FieldType::Uuid, false), field("text", FieldType::String, false)], indexes: Vec::new() };
        assert!(adapter.ensure_schema(chat).await.success, "chat opens beside a newer submissions table");
        let report = adapter.schema_report();
        let standing = |collection: &str| report.iter().find(|p| p.collection == collection).map(|p| p.standing());
        assert_eq!(standing("training_trigger_submissions"), Some(Standing::Newer));
        assert_eq!(standing("chat_messages"), Some(Standing::Additive), "chat's own plan, its create: nothing of submissions in it");
    }

    // what this catches (found by the open-records test above): a declared index over a column nobody
    // declares or holds, planned as an addition, so a store that can never build it reads Additive forever
    // and the migrator would try to create an index over a missing column.
    #[test]
    fn an_index_over_an_undeclared_column_is_unbuildable_never_additive() {
        let schema = CollectionSchema { fields: vec![field("sequence", FieldType::Number, false), field("carriedFrom", FieldType::Uuid, true)], ..submissions() };
        let physical = PhysicalTable { columns: vec![column("id", "TEXT"), column("sequence", "INTEGER"), column("carried_from", "TEXT")], indexes: Vec::new() };
        let planned = plan(&schema, Some(&physical));
        assert_eq!(planned.items, vec![PlanItem::IndexUnbuildable { index: "idx_training_submission_pending".into(), missing: vec!["is_pending".into()] }]);
        assert_eq!(planned.standing(), Standing::Current, "an unbuildable index is a declaration defect, reported, never a change to apply");
    }

    // what this catches: a missing table read as anything but a create plus its indexes.
    #[test]
    fn a_missing_table_is_created_with_its_declared_indexes() {
        let planned = plan(&submissions(), None);
        assert_eq!(planned.items[0], PlanItem::CreateTable);
        assert!(planned.items.iter().any(|item| matches!(item, PlanItem::AddIndex { index, .. } if index == "idx_training_submission_pending")));
        assert_eq!(planned.standing(), Standing::Additive);
    }
}
