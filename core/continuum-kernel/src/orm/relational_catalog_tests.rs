#![cfg(test)]

use super::storage_error::StorageError;
use super::schema_error::{SchemaDiff, SchemaError};
use super::*;

#[tokio::test]
async fn canonical_integer_text_evolves_exactly_with_and_without_receipt()
    -> Result<(), Box<dyn std::error::Error>>
{
    use sea_orm::EntityTrait;
    for recorded in [true, false] {
        let directory = tempfile::tempdir()?;
        let file = directory.path().join("integer-conversion.db");
        let path = file.to_str().ok_or("test path encoding")?;
        let initial = store(path).await?;
        let mut text = Parent::collection_schema();
        text.fields.iter_mut().find(|f| f.name == "label").ok_or("label absent")?.nullable = true;
        initial.ensure(&text).await?;
        let values = [Some(i64::MIN), Some(-1), Some(0), Some(9007199254740993), Some(i64::MAX), None];
        for (index, value) in values.iter().enumerate() {
            initial.insert(&initial.db, &record(&index.to_string(), Parent::COLLECTION,
                json!({"label": value.map(|number| number.to_string())}))).await?;
        }
        if !recorded {
            super::relational_declaration::Entity::delete_by_id(Parent::COLLECTION).exec(&initial.db).await?;
        }
        initial.db.clone().close().await?;
        let reopened = RelationalStore::open(&AdapterConfig {
            connection_string: path.into(), max_connections: 1, ..Default::default()
        }).await?;
        let mut numeric = text;
        numeric.fields.iter_mut().find(|f| f.name == "label").ok_or("label absent")?.field_type = FieldType::Number;
        reopened.ensure(&numeric).await?;
        reopened.db.clone().close().await?;
        let final_store = RelationalStore::open(&AdapterConfig {
            connection_string: path.into(), max_connections: 1, ..Default::default()
        }).await?;
        final_store.ensure(&numeric).await?;
        let rows = final_store.query(&QueryBuilder::new(Parent::COLLECTION).limit(6).build()).await?;
        assert_eq!(rows.len(), values.len());
        for row in rows {
            let index = row.id.parse::<usize>()?;
            assert_eq!(row.data["label"], json!(values[index]));
        }
    }
    Ok(())
}

#[tokio::test]
async fn noncanonical_integer_text_refuses_without_partial_conversion()
    -> Result<(), Box<dyn std::error::Error>>
{
    for invalid in ["", "01", "-0", "+1", " 1", "1 ", "1.5", "1e3", "bad", "9223372036854775808", "-9223372036854775809"] {
        let initial = store(":memory:").await?;
        initial.insert(&initial.db, &record("valid", Parent::COLLECTION,
            json!({"label":"9007199254740993"}))).await?;
        initial.insert(&initial.db, &record("invalid", Parent::COLLECTION,
            json!({"label":invalid}))).await?;
        let mut numeric = Parent::collection_schema();
        numeric.fields.iter_mut().find(|f| f.name == "label").ok_or("label absent")?.field_type = FieldType::Number;
        assert!(initial.ensure(&numeric).await.is_err(), "accepted {invalid:?}");
        let rows = initial.query(&QueryBuilder::new(Parent::COLLECTION).limit(2).build()).await?;
        assert_eq!(rows.len(), 2);
        for row in rows {
            assert_eq!(row.data["label"], if row.id == "valid" { "9007199254740993" } else { invalid });
        }
    }
    Ok(())
}

/// Unsupported conversions identify their declaration or physical boundary.
/// Both admission paths identify the exact boundary without changing stored evidence.
#[tokio::test]
async fn conversion_refusal_identifies_boundary_and_preserves_evidence()
    -> Result<(), Box<dyn std::error::Error>>
{
    use sea_orm::TransactionTrait;
    use super::schema_error::SchemaError;
    let original = Parent::collection_schema();
    let initial = store(":memory:").await?;
    initial.insert(&initial.db, &record("negative-review", Parent::COLLECTION,
        json!({"label":"9007199254740993"}))).await?;
    let mut requested = original.clone();
    requested.fields.iter_mut().find(|field| field.name == "label")
        .ok_or("fixture label missing")?.field_type = FieldType::Boolean;

    let tx = initial.db.begin().await?;
    let declared = super::relational_evolution::evolve(&tx, &original, &requested).await;
    assert!(matches!(declared, Err(SchemaError::DeclaredTypeConversion {
        collection, field, previous: FieldType::String, requested: FieldType::Boolean,
    }) if collection == Parent::COLLECTION && field == "label"));
    tx.rollback().await?;

    let tx = initial.db.begin().await?;
    let physical = super::relational_evolution::admit_table(&tx, Parent::COLLECTION,
        super::relational_schema::compile(&requested)?).await;
    assert!(matches!(physical, Err(SchemaError::PhysicalTypeConversion {
        table, column, previous, requested,
    }) if table == Parent::COLLECTION && column == "label"
        && previous == "TEXT" && !requested.as_str().is_empty() && requested != previous));
    tx.rollback().await?;
    let rows = initial.query(&QueryBuilder::new(Parent::COLLECTION).limit(1).build()).await?;
    assert_eq!(rows[0].id, "negative-review");
    assert_eq!(rows[0].data["label"], "9007199254740993");
    Ok(())
}

/// Validate the historical rejection timestamps before designing their one-time transfer.
#[tokio::test]
#[ignore = "requires ORM_DECLARATION_DB"]
async fn validate_review_rejection_timestamps_read_only() -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::{ConnectionTrait, TransactionTrait, sea_query::{Alias, Expr, Order, Query}};
    let path = std::env::var("ORM_DECLARATION_DB")?;
    let db = sea_orm::Database::connect(format!("sqlite://{}?mode=ro", path.replace('\\', "/"))).await?;
    let tx = db.begin().await?;
    super::relational_conversion::prove_integer_text(&tx, "staged_credit", "rejected_at_ms").await?;
    let mut cursor: Option<String> = None;
    let (mut valid, mut invalid) = (0_u64, 0_u64);
    loop {
        let mut query = Query::select();
        query.columns([Alias::new("id"), Alias::new("rejected_at_ms")])
            .from(Alias::new("staged_credit"))
            .and_where(Expr::col(Alias::new("rejected_at_ms")).is_not_null())
            .order_by(Alias::new("id"), Order::Asc).limit(128);
        if let Some(after) = &cursor {
            query.and_where(Expr::col(Alias::new("id")).gt(after.as_str()));
        }
        let rows = tx.query_all(tx.get_database_backend().build(&query)).await?;
        if rows.is_empty() { break; }
        for row in rows {
            let id: String = row.try_get("", "id")?;
            let timestamp: String = row.try_get("", "rejected_at_ms")?;
            match timestamp.parse::<u64>().ok().and_then(|value| i64::try_from(value).ok()) {
                Some(_) => valid += 1,
                None => { invalid += 1; println!("INVALID_REJECTION_TIMESTAMP id={id}"); }
            }
            cursor = Some(id);
        }
    }
    tx.rollback().await?;
    db.close().await?;
    println!("REJECTION_TIMESTAMP_VALIDATION valid={valid} invalid={invalid}");
    if invalid != 0 { return Err("invalid rejection timestamps; row identities printed above".into()); }
    Ok(())
}

/// Aggregate scalar storage classes in the database; never load credit bodies.
#[tokio::test]
#[ignore = "requires ORM_DECLARATION_DB, ORM_DECLARATION_COLLECTION, ORM_DECLARATION_COLUMN"]
async fn inspect_scalar_storage_classes_read_only() -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::{ConnectionTrait, sea_query::{Alias, Expr, Func, Query}};
    let path = std::env::var("ORM_DECLARATION_DB")?;
    let collection = std::env::var("ORM_DECLARATION_COLLECTION")?;
    let column = std::env::var("ORM_DECLARATION_COLUMN")?;
    let table = super::adapter::naming::to_table_name(&collection);
    let db = sea_orm::Database::connect(format!("sqlite://{}?mode=ro", path.replace('\\', "/"))).await?;
    let physical = super::relational_structure::read(&db, &table).await?;
    if !physical.columns.iter().any(|field| field.name == column) {
        return Err("requested physical column is absent".into());
    }
    let query = Query::select()
        .expr_as(Func::cust(Alias::new("typeof")).arg(Expr::col(Alias::new(&column))), Alias::new("storage_class"))
        .expr_as(Func::count(Expr::col(sea_orm::sea_query::Asterisk)), Alias::new("rows"))
        .from(Alias::new(&table))
        .group_by_col(Alias::new("storage_class"))
        .to_owned();
    for row in db.query_all(db.get_database_backend().build(&query)).await? {
        println!("SCALAR_CLASS {collection} {column} {} {}", row.try_get::<String>("", "storage_class")?, row.try_get::<i64>("", "rows")?);
    }
    db.close().await?;
    Ok(())
}

/// Inspect one persisted declaration without admitting schemas or reading row bodies.
/// The connection is read-only even when the source is a running persona's store.
#[tokio::test]
#[ignore = "requires ORM_DECLARATION_DB and ORM_DECLARATION_COLLECTION"]
async fn inspect_persisted_declaration_read_only() -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::EntityTrait;
    let path = std::env::var("ORM_DECLARATION_DB")?;
    let collection = std::env::var("ORM_DECLARATION_COLLECTION")?;
    let db = sea_orm::Database::connect(format!(
        "sqlite://{}?mode=ro", path.replace('\\', "/")
    )).await?;
    let physical = super::relational_structure::read(&db, &super::adapter::naming::to_table_name(&collection)).await?;
    for column in &physical.columns {
        println!("PHYSICAL_COLUMN {} {} {} required={}", collection, column.name, column.r#type, column.notnull);
    }
    match super::relational_declaration::Entity::find_by_id(&collection).one(&db).await? {
        Some(receipt) => {
            let schema: CollectionSchema = serde_json::from_value(receipt.declaration)?; // Decode the persisted ORM catalog format, never application row bodies.
            for field in &schema.fields {
                println!("DECLARED_FIELD {} {} {:?} nullable={}", collection, field.name, field.field_type, field.nullable);
            }
        }
        None => println!("NO_DECLARATION_RECEIPT {collection}"),
    }
    db.close().await?;
    Ok(())
}

#[tokio::test]
async fn native_text_declaration_retires_json_codec_without_rewriting_values() -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};
    use super::relational_declaration as declaration;
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("native-text.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let initial = store(path).await?;
    let values = ["Episodic", "\"Semantic\"", "{\"kept\":true}"];
    for (index, value) in values.iter().enumerate() {
        initial.insert(&initial.db, &record(&index.to_string(), Parent::COLLECTION, json!({"label":value}))).await?;
    }
    // Reproduce the rollout fault: native text rows certified with the wrong JSON codec.
    let receipt = declaration::Entity::find_by_id(Parent::COLLECTION).one(&initial.db).await?.ok_or("receipt absent")?;
    let mut incorrect = Parent::collection_schema();
    incorrect.fields.iter_mut().find(|field| field.name == "label").ok_or("label absent")?.field_type = FieldType::Json;
    let physical = receipt.physical.clone();
    let mut active = receipt.into_active_model();
    active.declaration = Set(serde_json::to_value(incorrect)?);
    active.update(&initial.db).await?;
    initial.db.clone().close().await?;
    for _ in 0..2 {
        let reopened = RelationalStore::open(&AdapterConfig { connection_string: path.into(), ..Default::default() }).await?;
        reopened.ensure(&Parent::collection_schema()).await?;
        let rows = reopened.query(&QueryBuilder::new(Parent::COLLECTION).limit(3).build()).await?;
        assert_eq!(rows.len(), 3);
        for value in values { assert!(rows.iter().any(|row| row.data["label"] == value)); }
        let receipt = declaration::Entity::find_by_id(Parent::COLLECTION).one(&reopened.db).await?.ok_or("receipt absent")?;
        assert_eq!(receipt.physical, physical, "codec retirement must not rewrite the table");
        let mut reverse = Parent::collection_schema();
        reverse.fields.iter_mut().find(|field| field.name == "label").ok_or("label absent")?.field_type = FieldType::Json;
        assert!(reopened.ensure(&reverse).await.is_err(), "text must not become JSON by guessing");
        reopened.db.clone().close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn rebuild_preserves_cascading_children_and_rolls_back_constraint_failure() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("rebuild.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let initial = store(path).await?;
    initial.insert(&initial.db, &record("p", Parent::COLLECTION, json!({"label":"retained"}))).await?;
    initial.insert(&initial.db, &record("c", Child::COLLECTION,
        json!({"parentId":"p","token":"unique","seq":9007199254740993u64,"body":{"kept":true},"joinedLabel":null}))).await?;
    let mut invalid = Parent::collection_schema();
    invalid.fields.iter_mut().find(|field| field.name == "label").ok_or("label absent")?.max_length = Some(2);
    assert!(initial.ensure(&invalid).await.is_err());
    let mut current = Parent::collection_schema();
    current.fields.retain(|field| field.name != "label");
    initial.ensure(&current).await?;
    let query = || QueryBuilder::new(Child::COLLECTION).limit(1).build();
    let children = initial.query(&query()).await?;
    assert_eq!(children.len(), 1, "DROP during rebuild must not cascade");
    assert_eq!(children[0].data["seq"], json!(9007199254740993u64));
    initial.db.clone().close().await?;
    let reopened = RelationalStore::open(&AdapterConfig {
        connection_string: path.into(), max_connections: 1, ..Default::default()
    }).await?;
    reopened.ensure(&current).await?;
    reopened.ensure(&Child::collection_schema()).await?;
    assert_eq!(reopened.query(&query()).await?.len(), 1);
    // Normal writes still enforce FK constraints after the schema connection closes.
    assert!(reopened.insert(&reopened.db, &record("bad", Child::COLLECTION,
        json!({"parentId":"absent","token":"bad","seq":2,"body":{},"joinedLabel":null}))).await.is_err());
    Ok(())
}

#[tokio::test]
async fn declaration_addition_evolves_rows_and_receipt_atomically() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("evolution.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let initial = store(path).await?;
    initial.insert(&initial.db, &record("p", Parent::COLLECTION, json!({"label":"retained"}))).await?;
    let mut current = Parent::collection_schema();
    current.fields.push(SchemaField {
        name: "status".into(), field_type: FieldType::String, nullable: false,
        default_value: Some(json!("ready")), indexed: true, unique: false,
        max_length: None, foreign_key: None,
    });
    initial.ensure(&current).await?;
    let query = || QueryBuilder::new(Parent::COLLECTION).limit(1).build();
    let rows = initial.query(&query()).await?;
    assert_eq!(rows[0].data["status"], "ready");
    assert_eq!(rows[0].data["label"], "retained");
    // A missing required value rejects the entire next declaration, including
    // an otherwise-valid addition that precedes it.
    let mut invalid = current.clone();
    let mut extra = current.fields.last().ok_or("new field absent")?.clone();
    extra.name = "extra".into();
    invalid.fields.push(extra.clone());
    extra.name = "missingValue".into();
    extra.default_value = None;
    invalid.fields.push(extra);
    assert!(initial.ensure(&invalid).await.is_err());
    // Fail after ALTER has run: another table already owns this index name.
    // The admitted map, physical table and receipt must all retain current.
    let mut collision = current.clone();
    let mut extra = current.fields.last().ok_or("new field absent")?.clone();
    extra.name = "extra".into();
    collision.fields.push(extra);
    collision.indexes.push(SchemaIndex {
        name: "idx_relational_children_seq".into(), fields: vec!["status".into()],
        unique: false, descending: false,
    });
    assert!(initial.ensure(&collision).await.is_err());
    initial.db.clone().close().await?;
    let reopened = RelationalStore::open(&AdapterConfig {
        connection_string: path.into(), max_connections: 1, ..Default::default()
    }).await?;
    reopened.ensure(&current).await?;
    let rows = reopened.query(&query()).await?;
    assert_eq!(rows[0].data["status"], "ready");
    let actual: std::collections::BTreeSet<_> =
        rows[0].data.as_object().ok_or("missing record data")?.keys().collect();
    assert_eq!(actual, current.fields.iter().map(|field| &field.name).collect());
    Ok(())
}

#[tokio::test]
async fn receipt_free_equivalent_column_order_preserves_rows() -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::EntityTrait;
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("column-order.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let initial = RelationalStore::open(&AdapterConfig {
        connection_string: path.into(), max_connections: 1, ..Default::default()
    }).await?;
    let mut reordered = Parent::collection_schema();
    reordered.fields.reverse();
    initial.ensure(&reordered).await?;
    initial.insert(&initial.db, &record("p", Parent::COLLECTION, json!({"label":"preserved"}))).await?;
    super::relational_declaration::Entity::delete_many().exec(&initial.db).await?;
    initial.db.clone().close().await?;
    let reopened = store(path).await?;
    let rows = reopened.query(&QueryBuilder::new(Parent::COLLECTION).limit(1).build()).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].data["label"], "preserved");
    Ok(())
}

#[tokio::test]
async fn receipt_free_current_schema_is_proved_without_rewriting_rows()
    -> Result<(), Box<dyn std::error::Error>>
{
    use sea_orm::{EntityTrait, ConnectionTrait, DbBackend};
    use crate::orm::relational_declaration as declaration;
    use sea_orm::sea_query::{Alias, ColumnDef, Table};
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("adoption.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let initial = store(path).await?;
    initial.insert(&initial.db, &record("p", Parent::COLLECTION, json!({"label":"unchanged"}))).await?;
    initial.insert(&initial.db, &record("c", Child::COLLECTION,
        json!({"parentId":"p","token":"unique","seq":9007199254740993u64,"body":{"original":true},"joinedLabel":null}))).await?;
    let query = || QueryBuilder::new(Child::COLLECTION).limit(1).build();
    let before = initial.query(&query()).await?;
    declaration::Entity::delete_many().exec(&initial.db).await?;
    initial.db.clone().close().await?;

    let reopened = store(path).await?;
    let after = reopened.query(&query()).await?;
    assert_eq!(serde_json::to_value(&before)?, serde_json::to_value(&after)?);
    assert!(declaration::Entity::find_by_id(Child::COLLECTION).one(&reopened.db).await?.is_some());
    // The receipt remains sufficient on another restart; it is not a repeated adoption.
    reopened.db.clone().close().await?;
    let admitted = store(path).await?;
    assert_eq!(admitted.query(&query()).await?.len(), 1);

    declaration::Entity::delete_by_id(Child::COLLECTION).exec(&admitted.db).await?;
    let drift = Table::alter().table(Alias::new(Child::COLLECTION))
        .add_column(ColumnDef::new(Alias::new("extra")).text()).to_owned();
    admitted.db.execute(DbBackend::Sqlite.build(&drift)).await?;
    admitted.db.clone().close().await?;
    let evolved = RelationalStore::open(&AdapterConfig {
        connection_string: path.into(), max_connections: 1, ..Default::default()
    }).await?;
    evolved.ensure(&Child::collection_schema()).await?;
    assert!(declaration::Entity::find_by_id(Child::COLLECTION).one(&evolved.db).await?.is_some());
    assert_eq!(serde_json::to_value(&before)?, serde_json::to_value(evolved.query(&query()).await?)?);
    let shape = super::relational_structure::read(&evolved.db, Child::COLLECTION).await?;
    assert!(!shape.columns.iter().any(|column| column.name == "extra"));
    Ok(())
}

#[test]
fn declaration_equivalence_rejects_duplicate_names_on_either_side() {
    use crate::orm::entity::schemas_equivalent;
    let mut valid = Parent::collection_schema();
    valid.indexes = vec![
        SchemaIndex {
            name: "a".into(),
            fields: vec!["id".into()],
            unique: true,
            descending: false,
        },
        SchemaIndex {
            name: "b".into(),
            fields: vec!["label".into()],
            unique: false,
            descending: false,
        },
    ];
    let mut reordered = valid.clone();
    reordered.fields.reverse();
    reordered.indexes.reverse();
    assert!(schemas_equivalent(&valid, &reordered));
    let mut bad_index = valid.clone();
    bad_index.indexes[1] = bad_index.indexes[0].clone();
    assert!(!schemas_equivalent(&valid, &bad_index));
    assert!(!schemas_equivalent(&bad_index, &valid));
    let mut bad_field = valid.clone();
    bad_field.fields[1] = bad_field.fields[0].clone();
    assert!(!schemas_equivalent(&valid, &bad_field));
    assert!(!schemas_equivalent(&bad_field, &valid));
}

#[tokio::test]
async fn restart_catalog_crosses_pages_and_rejects_late_physical_drift(
) -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::sea_query::{Alias, ColumnDef, Table};
    use sea_orm::{ConnectionTrait, DbBackend};
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("catalog.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let config = || AdapterConfig {
        connection_string: path.into(),
        max_connections: 1,
        ..Default::default()
    };
    let initial = RelationalStore::open(&config()).await?;
    // More than two transfer pages; empty tables still require declaration verification.
    for index in 0..131 {
        let mut schema = Parent::collection_schema();
        schema.collection = format!("catalog_{index:03}");
        initial.ensure(&schema).await?;
    }
    initial.db.clone().close().await?;
    let started = std::time::Instant::now();
    let restored = RelationalStore::open(&config()).await?;
    println!(
        "CATALOG_REPLAY declarations=131 connections=1 elapsed_us={}",
        started.elapsed().as_micros()
    );
    assert_eq!(restored.collections().len(), 131);
    for index in 0..131 {
        let name = format!("catalog_{index:03}");
        assert_eq!(restored.schema(&name)?.collection, name);
    }
    let altered = Table::alter()
        .table(Alias::new("catalog_130"))
        .add_column(ColumnDef::new(Alias::new("unexpected")).text())
        .to_owned();
    restored
        .db
        .execute(DbBackend::Sqlite.build(&altered))
        .await?;
    restored.db.clone().close().await?;
    let failure = RelationalStore::open(&config())
        .await
        .err()
        .ok_or("late drift was admitted")?;
    assert!(
        matches!(&failure, StorageError::Schema(SchemaError::Difference { collection, difference: SchemaDiff::RecordedPhysical }) if collection == "catalog_130"),
        "{failure}"
    );
    Ok(())
}

#[tokio::test]
async fn restart_refuses_unknown_nested_receipt_metadata() -> Result<(), Box<dyn std::error::Error>>
{
    use crate::orm::relational_declaration as declaration;
    use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("unknown-metadata.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let owner = store(path).await?;
    let mut receipt = declaration::Entity::find_by_id(Parent::COLLECTION)
        .one(&owner.db)
        .await?
        .ok_or("fixture declaration absent")?;
    receipt.declaration["fields"][0]["unsupportedCollation"] = json!("NOCASE");
    let changed = receipt.declaration.clone();
    let mut active = receipt.into_active_model();
    active.declaration = Set(changed);
    active.update(&owner.db).await?;
    owner.db.clone().close().await?;
    let failure = store(path)
        .await
        .err()
        .ok_or("unknown metadata was silently discarded")?;
    assert!(
        matches!(failure.downcast_ref::<StorageError>(), Some(StorageError::Schema(SchemaError::Codec(_)))),
        "unknown declaration metadata refuses as a codec failure: {failure}"
    );
    Ok(())
}

#[test]
fn declaration_decoding_requires_semantic_fields_and_refuses_unknown_metadata(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut schema = Child::collection_schema();
    schema.indexes.push(SchemaIndex {
        name: "explicit_direction".into(),
        fields: vec!["seq".into()],
        unique: false,
        descending: false,
    });
    let canonical = serde_json::to_value(&schema)?;
    assert_eq!(
        CollectionSchema::deserialize(&canonical)?,
        schema
    );
    let fk_index = schema
        .fields
        .iter()
        .position(|field| field.foreign_key.is_some())
        .ok_or("fixture foreign key absent")?;
    let field_path = format!("/fields/{fk_index}");
    let fk_path = format!("{field_path}/foreignKey");
    let index_path = format!("/indexes/{}", schema.indexes.len() - 1);
    for (path, required) in [
        ("", vec!["collection", "fields", "indexes"]),
        (
            field_path.as_str(),
            vec!["name", "fieldType", "indexed", "unique", "nullable"],
        ),
        (
            fk_path.as_str(),
            vec!["collection", "field", "onDelete", "onUpdate"],
        ),
        (
            index_path.as_str(),
            vec!["name", "fields", "unique", "descending"],
        ),
    ] {
        for field in required {
            let mut malformed = canonical.clone();
            malformed
                .pointer_mut(path)
                .and_then(serde_json::Value::as_object_mut)
                .ok_or("fixture object absent")?
                .remove(field);
            assert!(
                serde_json::from_value::<CollectionSchema>(malformed).is_err(),
                "accepted absent {path}/{field}"
            );
        }
        let mut malformed = canonical.clone();
        malformed
            .pointer_mut(path)
            .and_then(serde_json::Value::as_object_mut)
            .ok_or("fixture object absent")?
            .insert("unsupported".into(), json!(true));
        assert!(
            serde_json::from_value::<CollectionSchema>(malformed).is_err(),
            "accepted unknown metadata at {path}"
        );
    }
    // Optional insertion defaults, length limits and references express valid absence.
    let mut optional_absent = canonical;
    for field in optional_absent["fields"]
        .as_array_mut()
        .ok_or("fixture fields absent")?
    {
        let object = field.as_object_mut().ok_or("fixture field object absent")?;
        for key in ["defaultValue", "maxLength"] {
            object.remove(key);
        }
        object.retain(|key, value| key != "foreignKey" || !value.is_null());
    }
    assert_eq!(
        serde_json::from_value::<CollectionSchema>(optional_absent)?,
        schema
    );
    Ok(())
}

#[tokio::test]
async fn revision_restart_crosses_domain_pages_and_rejects_late_corruption(
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::orm::relational_sequence::{ActiveModel, Domain};
    use sea_orm::{ActiveModelTrait, Set};
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("revision-heads.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let config = || AdapterConfig {
        connection_string: path.into(),
        max_connections: 1,
        ..Default::default()
    };
    let initial = RelationalStore::open(&config()).await?;
    for domain in ["append", "native"] {
        for index in 0..131i64 {
            ActiveModel {
                domain: Set(domain.into()),
                collection: Set(format!("head_{index:03}")),
                high: Set(index + 1),
            }
            .insert(&initial.db)
            .await?;
        }
    }
    initial.db.clone().close().await?;
    let started = std::time::Instant::now();
    let restored = RelationalStore::open(&config()).await?;
    println!(
        "REVISION_REPLAY heads=262 elapsed_us={}",
        started.elapsed().as_micros()
    );
    for domain in [Domain::Append, Domain::Native] {
        for index in 0..131u64 {
            assert_eq!(
                *restored
                    .revision(domain, &format!("head_{index:03}"))
                    .borrow(),
                index + 1
            );
        }
    }
    ActiveModel {
        domain: Set("native".into()),
        collection: Set("head_130".into()),
        high: Set(0),
    }
    .update(&restored.db)
    .await?;
    restored.db.clone().close().await?;
    let error = RelationalStore::open(&config())
        .await
        .err()
        .ok_or("corrupt late revision was admitted")?;
    assert!(matches!(error, StorageError::Corrupt { .. }), "{error}");
    Ok(())
}

#[tokio::test]
async fn empty_declaration_replacement_survives_restart() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("empty-declaration.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let initial = store(path).await?;
    let mut requested = Parent::collection_schema();
    requested.fields.iter_mut().find(|field| field.name == "label")
        .ok_or("fixture label absent")?.field_type = FieldType::Boolean;
    let mut required = requested.fields.iter().find(|field| field.name == "label")
        .ok_or("fixture label absent")?.clone();
    required.name = "requiredFlag".into();
    required.nullable = false;
    requested.fields.push(required);
    initial.ensure(&requested).await?;
    initial.insert(&initial.db, &record("new-row", Parent::COLLECTION,
        json!({"label": true, "requiredFlag": false}))).await?;
    initial.db.clone().close().await?;
    let reopened = RelationalStore::open(&AdapterConfig {
        connection_string: path.into(), max_connections: 1, ..Default::default()
    }).await?;
    reopened.ensure(&requested).await?;
    let rows = reopened.query(&QueryBuilder::new(Parent::COLLECTION).limit(1).build()).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].data["label"], true);
    assert_eq!(rows[0].data["requiredFlag"], false);
    Ok(())
}
