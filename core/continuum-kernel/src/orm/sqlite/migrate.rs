//! Step 0b of the durable store (ONE-FRAME-ONE-MEMORY.md, "The durable store", rules 1, 3, 4 and 6): a
//! collection's declaration has a generation, the store records the generation it holds, and an open applies
//! only what the plan (`schema_plan`, step 0a) proves additive, in one transaction with that record.
//!
//! - Current: the record is written if absent; nothing else changes.
//! - Additive: the table, nullable columns and indexes, then the record, commit together (rule 6), and the
//!   plan read back must be Current or Newer.
//! - Newer: the store holds columns a newer or other build wrote. It is served as is and its record is left
//!   as that writer recorded it (rule 4), so a rollback works across added columns.
//! - Breaking or Unreadable: reported by the plan and left untouched. Refusing that collection at open waits
//!   for the three nodes' inventory (step 0a's `data/schema-plan`), so no live store is refused unseen.

use rusqlite::Connection;
use continuum_base::time::EpochMs;
use sha2::{Digest, Sha256};

use super::naming;
use super::schema_plan::{plan_now, CollectionPlan, PlanItem, Standing};
use crate::orm::types::CollectionSchema;

/// A declaration's identity: its columns (name, type, required) and its indexes, in a fixed order, hashed.
/// Two builds that declare the same collection the same way agree on it whatever order they list fields in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation(pub String);

impl Generation {
    pub fn of(schema: &CollectionSchema) -> Self {
        let mut columns: Vec<String> = schema
            .fields
            .iter()
            .map(|field| (naming::to_snake_case(&field.name), field))
            .filter(|(column, _)| !super::super::entity::is_base_entity_column(column))
            .map(|(column, field)| format!("{column}:{}:{}", super::sql_column_type(&field.field_type), !field.nullable))
            .collect();
        columns.sort();
        let mut indexes: Vec<String> = schema
            .indexes
            .iter()
            .map(|index| format!("{}:{}:{}", naming::to_snake_case(&index.name), index.fields.join(","), index.unique))
            .chain(schema.fields.iter().filter(|field| field.indexed).map(|field| format!("indexed:{}", naming::to_snake_case(&field.name))))
            .collect();
        indexes.sort();
        let mut hasher = Sha256::new();
        hasher.update(columns.join("\n").as_bytes());
        hasher.update(b"\n--\n");
        hasher.update(indexes.join("\n").as_bytes());
        Self(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
    }
}

/// What an open did to one collection's store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// It already held its declaration; the record says so.
    Current,
    /// The plan's additions and the record committed together.
    Migrated { items: usize },
    /// It holds more than its declaration: served as is, its record untouched.
    Newer,
    /// Breaking or unreadable: reported, left untouched (see the module doc).
    Left { standing: Standing },
}

#[derive(Debug, thiserror::Error)]
pub enum Migrate {
    #[error("migrating {collection}: {source}")]
    Sql { collection: String, #[source] source: rusqlite::Error },
    #[error("creating {collection}: {error}")]
    Create { collection: String, error: String },
    #[error("index {index} on {collection}: {error}")]
    Index { collection: String, index: String, error: String },
    #[error("{collection} after its migration still stands {standing:?}: {items:?}")]
    NotApplied { collection: String, standing: Standing, items: Vec<PlanItem> },
}

/// The generation the store records for `collection`, if any.
pub fn recorded(conn: &Connection, collection: &str) -> Result<Option<Generation>, rusqlite::Error> {
    ensure_record_table(conn)?;
    let mut stmt = conn.prepare("SELECT generation FROM orm_schema_generations WHERE collection = ?1")?;
    let mut rows = stmt.query([collection])?;
    match rows.next()? {
        Some(row) => Ok(Some(Generation(row.get(0)?))),
        None => Ok(None),
    }
}

fn ensure_record_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS orm_schema_generations (collection TEXT PRIMARY KEY, generation TEXT NOT NULL, migrated_at_ms INTEGER NOT NULL)",
    )
}

fn write_record(conn: &Connection, schema: &CollectionSchema, now: EpochMs) -> Result<(), rusqlite::Error> {
    let now_ms = i64::try_from(now.millis()).map_err(|overflow| rusqlite::Error::ToSqlConversionFailure(Box::new(overflow)))?;
    conn.execute(
        "INSERT INTO orm_schema_generations (collection, generation, migrated_at_ms) VALUES (?1, ?2, ?3) \
         ON CONFLICT(collection) DO UPDATE SET generation = excluded.generation, migrated_at_ms = excluded.migrated_at_ms",
        rusqlite::params![schema.collection, Generation::of(schema).0, now_ms],
    )?;
    Ok(())
}

/// Open `schema`'s collection on `conn` by its `plan` (the store as it stood: `schema_plan::record`).
pub(super) fn open(conn: &Connection, schema: &CollectionSchema, plan: &CollectionPlan, now: EpochMs) -> Result<Opened, Migrate> {
    let sql = |source| Migrate::Sql { collection: schema.collection.clone(), source };
    match plan.standing() {
        Standing::Current => {
            if recorded(conn, &schema.collection).map_err(sql)?.is_none() {
                write_record(conn, schema, now).map_err(sql)?;
            }
            Ok(Opened::Current)
        }
        Standing::Newer => Ok(Opened::Newer),
        standing @ (Standing::Breaking | Standing::Unreadable) => Ok(Opened::Left { standing }),
        Standing::Additive => {
            let tx = conn.unchecked_transaction().map_err(sql)?;
            apply(&tx, schema, plan)?;
            ensure_record_table(&tx).map_err(sql)?;
            write_record(&tx, schema, now).map_err(sql)?;
            tx.commit().map_err(sql)?;
            let after = plan_now(conn, schema);
            match after.standing() {
                Standing::Current | Standing::Newer => Ok(Opened::Migrated { items: plan.items.len() }),
                standing => Err(Migrate::NotApplied { collection: schema.collection.clone(), standing, items: after.items }),
            }
        }
    }
}

/// The plan's additions, in its order: the table (with its indexes) or each nullable column, then each index.
fn apply(conn: &Connection, schema: &CollectionSchema, plan: &CollectionPlan) -> Result<(), Migrate> {
    let sql = |source| Migrate::Sql { collection: schema.collection.clone(), source };
    for item in &plan.items {
        match item {
            PlanItem::CreateTable => {
                let created = super::do_ensure_schema(conn, schema);
                if let Some(error) = created.error {
                    return Err(Migrate::Create { collection: schema.collection.clone(), error });
                }
            }
            PlanItem::AddColumn { column, field_type, required: false } => {
                conn.execute_batch(&format!("ALTER TABLE \"{}\" ADD COLUMN \"{column}\" {}", plan.table, super::sql_column_type(field_type))).map_err(sql)?;
            }
            // a required column is Breaking and never reaches here; drift and undeclared columns are reported only
            PlanItem::AddColumn { required: true, .. }
            | PlanItem::IndexUnbuildable { .. }
            | PlanItem::UndeclaredColumn { .. }
            | PlanItem::TypeDrift { .. }
            | PlanItem::NullDrift { .. }
            | PlanItem::Unreadable { .. }
            | PlanItem::AddIndex { .. } => {}
        }
    }
    if plan.items.iter().any(|item| matches!(item, PlanItem::AddIndex { .. })) {
        let pass = super::ensure_indexes::do_ensure_indexes(conn, schema);
        if let Some(failed) = pass.failed.first() {
            return Err(Migrate::Index { collection: failed.collection.clone(), index: failed.index.clone(), error: failed.error.clone() });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::sqlite::schema_plan::record;
    use crate::orm::types::{FieldType, SchemaField, SchemaIndex};

    fn field(name: &str, field_type: FieldType, nullable: bool) -> SchemaField {
        SchemaField { name: name.into(), field_type, indexed: false, unique: false, nullable, max_length: None, foreign_key: None }
    }

    /// The collection as this build declares it: `carriedFrom` is new and nullable, `example_count` is not declared.
    fn submissions() -> CollectionSchema {
        CollectionSchema {
            collection: "training_trigger_submissions".into(),
            fields: vec![field("sequence", FieldType::Number, false), field("isPending", FieldType::Boolean, false), field("carriedFrom", FieldType::Uuid, true)],
            indexes: vec![SchemaIndex { name: "idx_training_submission_pending".into(), fields: vec!["isPending".into(), "sequence".into()], unique: false }],
        }
    }

    /// A store an older build created and a newer build wrote into, shaped like the 5090's (2026-11-27): no
    /// `carried_from` yet, an `example_count` no declaration here names, values on most rows and NULL on some.
    fn the_5090s_shape(rows: i64) -> Connection {
        let conn = Connection::open_in_memory().expect("test: an in-memory store");
        conn.execute_batch(
            "CREATE TABLE training_trigger_submissions (id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, \
             version INTEGER NOT NULL DEFAULT 1, sequence NUMERIC NOT NULL, is_pending INTEGER NOT NULL, example_count INTEGER)",
        )
        .expect("test: the old table");
        for at in 0..rows {
            let examples = (at % 10 != 0).then_some(at * 3); // NULL on one row in ten
            conn.execute(
                "INSERT INTO training_trigger_submissions (id, created_at, updated_at, sequence, is_pending, example_count) VALUES (?1, 't', 't', ?2, ?3, ?4)",
                rusqlite::params![format!("sub-{at}"), at, at % 2, examples],
            )
            .expect("test: a row");
        }
        conn
    }

    /// Every row's every column, by id: what a migration must keep.
    fn every_value(conn: &Connection) -> Vec<(String, Vec<rusqlite::types::Value>)> {
        let mut stmt = conn.prepare("SELECT * FROM training_trigger_submissions ORDER BY id").expect("test: read the rows");
        let width = stmt.column_count();
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, (0..width).map(|at| row.get::<_, rusqlite::types::Value>(at)).collect::<Result<Vec<_>, _>>()?)))
            .expect("test: query the rows");
        rows.collect::<Result<Vec<_>, _>>().expect("test: every row")
    }

    fn open_now(conn: &Connection, schema: &CollectionSchema) -> Result<Opened, Migrate> {
        let planned = parking_lot::RwLock::new(Vec::new());
        let plan = record(&planned, conn, schema);
        open(conn, schema, &plan, EpochMs::from_millis(1))
    }

    // what this catches (the 5090's fixture, Codex 2026-11-27: 1,310 example_count values, carried_* NULL): an
    // additive migration that loses or changes a value it does not own, the undeclared column's above all.
    // Every row keeps every column it had, the new column arrives NULL, and the store records its generation.
    #[test]
    fn an_additive_migration_keeps_every_value_by_id_and_records_its_generation() {
        let conn = the_5090s_shape(1_319);
        let before = every_value(&conn);
        assert_eq!(open_now(&conn, &submissions()).expect("test: it migrates"), Opened::Migrated { items: 3 });
        let after = every_value(&conn);
        assert_eq!(after.len(), before.len());
        for ((id, was), (same_id, now)) in before.iter().zip(&after) {
            assert_eq!(id, same_id);
            assert_eq!(&now[..was.len()], &was[..], "row {id} kept every column it had");
            assert_eq!(now[was.len()..], [rusqlite::types::Value::Null], "row {id}: the added column arrives NULL");
        }
        assert_eq!(recorded(&conn, "training_trigger_submissions").expect("test: read the record"), Some(Generation::of(&submissions())));
        assert_eq!(plan_now(&conn, &submissions()).standing(), Standing::Newer, "read back: only the newer writer's column is left over");
    }

    // what this catches (rule 6, Codex): a restart between a migration and its generation record leaving one
    // without the other. The record's write is made to fail; the column the migration added is rolled back with it.
    #[test]
    fn a_failed_generation_record_rolls_the_migration_back() {
        let conn = the_5090s_shape(20);
        conn.execute_batch("CREATE TABLE orm_schema_generations (collection TEXT PRIMARY KEY, generation TEXT NOT NULL CHECK (0), migrated_at_ms INTEGER NOT NULL)")
            .expect("test: a record table that refuses every write");
        let before = every_value(&conn);
        assert!(matches!(open_now(&conn, &submissions()), Err(Migrate::Sql { .. })), "the record failed, so the migration did");
        assert_eq!(every_value(&conn), before, "nothing the migration did is left behind");
        assert!(plan_now(&conn, &submissions()).items.iter().any(|item| matches!(item, PlanItem::AddColumn { column, .. } if column == "carried_from")), "the column is still to add");
    }

    // what this catches (rule 4): a rollback build rewriting the generation a newer writer recorded, so the store
    // claims a declaration it does not hold. A newer store is served as is and its record is left.
    #[test]
    fn a_newer_store_is_served_and_its_record_left_as_the_newer_writer_wrote_it() {
        let conn = the_5090s_shape(5);
        conn.execute_batch("ALTER TABLE training_trigger_submissions ADD COLUMN carried_from TEXT").expect("test: the newer writer's column");
        conn.execute_batch("CREATE INDEX idx_training_submission_pending ON training_trigger_submissions (is_pending, sequence)").expect("test: its index");
        ensure_record_table(&conn).expect("test: the record table");
        conn.execute("INSERT INTO orm_schema_generations VALUES ('training_trigger_submissions', 'the-newer-build', 7)", []).expect("test: its record");
        assert_eq!(open_now(&conn, &submissions()).expect("test: it opens"), Opened::Newer);
        assert_eq!(recorded(&conn, "training_trigger_submissions").expect("test: read the record"), Some(Generation("the-newer-build".into())));
    }

    // what this catches: a generation that changes with the order fields are listed in, so two builds that
    // declare the same collection read each other's store as a different generation.
    #[test]
    fn a_generation_names_the_declaration_never_its_order() {
        let mut reordered = submissions();
        reordered.fields.reverse();
        assert_eq!(Generation::of(&reordered), Generation::of(&submissions()));
        let mut changed = submissions();
        changed.fields[2].nullable = false;
        assert_ne!(Generation::of(&changed), Generation::of(&submissions()), "a column made required is a new generation");
    }
}
