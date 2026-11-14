use super::adapter::AdapterConfig;
use super::query::{JoinSpec, JoinType};
use super::relational::RelationalStore;
use super::types::{BatchOperation, BatchOperationType};
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize, Entity)]
#[entity(collection = "relational_parents")]
struct Parent {
    #[serde(flatten)]
    base: BaseEntity,
    label: String,
}

#[derive(Serialize, Deserialize, Entity)]
#[entity(collection = "relational_children")]
struct Child {
    #[serde(flatten)]
    base: BaseEntity,
    #[serde(rename = "parentId")]
    #[entity(foreign_key("relational_parents.id", on_delete = "cascade"))]
    parent_id: Option<String>,
    #[entity(unique)]
    token: String,
    #[entity(indexed)]
    seq: i64,
    #[entity(json)]
    body: serde_json::Value,
    #[serde(rename = "joinedLabel")]
    joined_label: Option<String>,
}

fn record(id: &str, collection: &str, data: serde_json::Value) -> DataRecord {
    DataRecord {
        id: id.into(),
        collection: collection.into(),
        data,
        metadata: RecordMetadata::default(),
    }
}
async fn store(path: &str) -> Result<RelationalStore, String> {
    let store = RelationalStore::open(AdapterConfig {
        connection_string: path.into(),
        max_connections: 1,
        ..Default::default()
    })
    .await?;
    store.ensure(Parent::collection_schema()).await?;
    store.ensure(Child::collection_schema()).await?;
    Ok(store)
}

#[tokio::test]
async fn declared_join_preserves_unmatched_null_and_filters_before_limit(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = store(":memory:").await?;
    store
        .insert(
            &store.db,
            &record("p", Parent::COLLECTION, json!({"label":"{literal text}"})),
        )
        .await?;
    for (id, parent, seq) in [("a", Some("p"), 1), ("b", None, 2)] {
        store.insert(&store.db, &record(id, Child::COLLECTION, json!({"parentId":parent,"token":id,"seq":seq,"joinedLabel":"keep me","body":{"integer":9007199254740993u64}}))).await?;
    }
    let mut query = QueryBuilder::new(Child::COLLECTION)
        .sort_asc("seq")
        .limit(2)
        .build();
    query.joins = Some(vec![JoinSpec {
        collection: Parent::COLLECTION.into(),
        alias: "parent".into(),
        local_field: "parentId".into(),
        foreign_field: "id".into(),
        join_type: JoinType::Left,
        select: Some(vec!["label".into()]),
    }]);
    let rows = store.query_joined(query.clone()).await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].data["parent"]["label"], "{literal text}");
    assert_eq!(rows[0].data["joinedLabel"], "keep me");
    assert_eq!(rows[1].data["parent"], serde_json::Value::Null);
    assert_eq!(
        rows[0].data["body"]["integer"].as_u64(),
        Some(9007199254740993)
    );
    if let Some(joins) = &mut query.joins {
        joins[0].join_type = JoinType::Inner;
    }
    assert_eq!(store.query_joined(query).await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn failed_batch_rolls_back_and_sequence_survives_pruning_and_restart(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("test.db");
    let path = path.to_str().ok_or("UTF8 path")?;
    let first = store(path).await?;
    let append = |id: &str| BatchOperation {
        operation_type: BatchOperationType::Append(SequenceSpec {
            sequence: "seq".into(),
            unique: "token".into(),
        }),
        collection: Child::COLLECTION.into(),
        id: Some(id.into()),
        data: Some(json!({"parentId":null,"token":id,"seq":0,"body":{}})),
    };
    let invalid = BatchOperation {
        operation_type: BatchOperationType::Update,
        collection: Child::COLLECTION.into(),
        id: Some("absent".into()),
        data: Some(json!({"body":{}})),
    };
    assert!(first.batch(vec![append("a"), invalid]).await.is_err());
    assert_eq!(
        first
            .count(QueryBuilder::new(Child::COLLECTION).build())
            .await?,
        0
    );
    let revision = first.revision(Child::COLLECTION);
    let committed = first.batch(vec![append("a")]).await?;
    assert_eq!(committed[0]["seq"], 1);
    assert_eq!(*revision.borrow(), 1);
    assert_eq!(first.batch(vec![append("a")]).await?[0]["fresh"], false);
    first
        .delete_matching(
            QueryBuilder::new(Child::COLLECTION)
                .filter_eq("id", "a")
                .build(),
        )
        .await?;
    first.db.close().await?;
    let reopened = store(path).await?;
    assert_eq!(reopened.batch(vec![append("b")]).await?[0]["seq"], 2);
    Ok(())
}

#[tokio::test]
async fn cancelled_request_retains_commit_and_revision_custody(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = store(":memory:").await?;
    let mut revision = store.revision(Child::COLLECTION);
    let (written_tx, written_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let worker = store.clone();
    let request = tokio::spawn(async move {
        worker
            .write_with_revisions(move |store, transaction| {
                Box::pin(async move {
                    let receipt = store
                        .append_on(
                            transaction,
                            record(
                                "cancelled",
                                Child::COLLECTION,
                                json!({"parentId":null,"token":"cancelled","seq":0,"body":{}}),
                            ),
                            &SequenceSpec {
                                sequence: "seq".into(),
                                unique: "token".into(),
                            },
                        )
                        .await?;
                    written_tx
                        .send(())
                        .map_err(|_| "write observer disappeared")?;
                    release_rx.await.map_err(|_| "commit release disappeared")?;
                    Ok(((), vec![(Child::COLLECTION.into(), receipt.seq)]))
                })
            })
            .await
    });
    written_rx.await?;
    assert_eq!(*revision.borrow(), 0);
    request.abort();
    assert!(matches!(request.await, Err(error) if error.is_cancelled()));
    release_tx
        .send(())
        .map_err(|_| "transaction owner disappeared with caller")?;
    tokio::time::timeout(std::time::Duration::from_secs(5), revision.changed()).await??;
    assert_eq!(*revision.borrow(), 1);
    assert!(store
        .read_on(&store.db, Child::COLLECTION, "cancelled")
        .await?
        .is_some());
    Ok(())
}

#[tokio::test]
async fn monotonic_updates_refuse_stale_values_and_preserve_exact_integers(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = store(":memory:").await?;
    let row = |seq| {
        record(
            "high",
            Child::COLLECTION,
            json!({"parentId":null,"token":"high","seq":seq,"body":{}}),
        )
    };
    let high = 9007199254740993i64;
    assert!(store.upsert_monotonic(row(high), "seq".into()).await?);
    assert!(!store.upsert_monotonic(row(high - 1), "seq".into()).await?);
    assert!(store.upsert_monotonic(row(high + 1), "seq".into()).await?);
    let read = store
        .read_on(&store.db, Child::COLLECTION, "high")
        .await?
        .ok_or("missing high water")?;
    assert_eq!(read.data["seq"].as_i64(), Some(high + 1));
    assert_eq!(read.metadata.version, 2);
    Ok(())
}

#[tokio::test]
async fn typed_store_uses_seaorm_adapter_and_database_side_statistics(
) -> Result<(), Box<dyn std::error::Error>> {
    let adapter = std::sync::Arc::new(
        super::relational_adapter::SeaOrmAdapter::connect(AdapterConfig {
            connection_string: ":memory:".into(),
            max_connections: 1,
            ..Default::default()
        })
        .await?,
    );
    let typed = OrmStore::<Parent>::new(adapter.clone()).await?;
    let id = uuid::Uuid::new_v4();
    typed
        .save(
            id,
            &Parent {
                base: BaseEntity::for_new_record_at(id),
                label: "{literal}".into(),
            },
        )
        .await?;
    assert_eq!(
        typed.find_by_id(id).await?.ok_or("typed row absent")?.label,
        "{literal}"
    );
    assert!(typed.find_by_id(uuid::Uuid::new_v4()).await?.is_none());
    let stats = adapter
        .collection_stats(Parent::COLLECTION)
        .await
        .into_result()?;
    assert_eq!(stats.record_count, 1);
    assert!(stats.total_size >= "{literal}".len());
    assert!(!stats.last_modified.is_empty());
    let cleared = adapter.clear_all().await.into_result()?;
    assert_eq!(cleared.records_deleted, 1);
    assert_eq!(
        adapter
            .count(QueryBuilder::new(Parent::COLLECTION).build())
            .await
            .into_result()?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn projected_cursor_pages_are_ordered_and_refuse_ambiguous_boundaries(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = store(":memory:").await?;
    assert!(store
        .query(QueryBuilder::new(Child::COLLECTION).build())
        .await
        .is_err());
    for id in ["c", "a", "b"] {
        store
            .create(record(
                id,
                Child::COLLECTION,
                json!({"parentId":null,"token":id,"seq":1,"body":{}}),
            ))
            .await?;
    }
    let mut query = QueryBuilder::new(Child::COLLECTION)
        .select(vec!["token".into()])
        .limit(2)
        .build();
    let first = store.query(query.clone()).await?;
    assert_eq!(
        first.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert!(first.iter().all(|row| row.data.get("body").is_none()));
    query.cursor = Some(super::query::Cursor {
        field: "id".into(),
        value: json!("b"),
        direction: super::query::CursorDirection::After,
    });
    let second = store.query(query.clone()).await?;
    assert_eq!(
        second.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["c"]
    );
    query.sort = Some(vec![super::query::SortSpec {
        field: "id".into(), direction: SortDirection::Desc,
    }]);
    let descending_tail = store.query(query.clone()).await?;
    assert_eq!(descending_tail.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["a"],
        "after b in descending order must advance toward a, not repeat c");
    query.cursor.as_mut().ok_or("cursor disappeared")?.direction = super::query::CursorDirection::Before;
    let descending_head = store.query(query.clone()).await?;
    assert_eq!(descending_head.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(), ["c"]);
    query.sort = None;
    query.cursor = Some(super::query::Cursor {
        field: "seq".into(),
        value: json!(1),
        direction: super::query::CursorDirection::After,
    });
    assert!(
        store.query(query).await.is_err(),
        "a repeated sequence value cannot identify a page boundary"
    );
    Ok(())
}

#[tokio::test]
async fn declared_defaults_and_length_are_enforced_by_storage() -> Result<(), Box<dyn std::error::Error>> {
    let store = store(":memory:").await?;
    let mut schema = CollectionSchema { collection: "declared_defaults".into(), fields: base_entity_fields(), indexes: vec![] };
    schema.fields.push(SchemaField {
        name: "label".into(), field_type: FieldType::String, default_value: Some(json!("abc")),
        nullable: false, indexed: false, unique: false, max_length: Some(3), foreign_key: None,
    });
    store.ensure(schema).await?;
    store.insert(&store.db, &record("default", "declared_defaults", json!({}))).await?;
    let rows = store.query(QueryBuilder::new("declared_defaults").limit(1).build()).await?;
    assert_eq!(rows[0].data["label"], json!("abc"));
    assert!(store.insert(&store.db, &record("too-long", "declared_defaults", json!({"label":"abcd"}))).await.is_err());
    assert!(store.insert(&store.db, &record("null", "declared_defaults", json!({"label":null}))).await.is_err());
    assert_eq!(store.count(QueryBuilder::new("declared_defaults").build()).await?, 1);
    Ok(())
}

#[tokio::test]
async fn restart_verifies_physical_declaration_and_refuses_unmanaged_tables() -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::{ConnectionTrait, DbBackend};
    use sea_orm::sea_query::{Alias, ColumnDef, Table};
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("declarations.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let first = store(path).await?;
    first.insert(&first.db, &record("p", Parent::COLLECTION, json!({"label":"retained"}))).await?;
    first.db.clone().close().await?;
    let reopened = store(path).await?;
    assert_eq!(reopened.count(QueryBuilder::new(Parent::COLLECTION).build()).await?, 1);
    let altered = Table::alter().table(Alias::new(Parent::COLLECTION))
        .add_column(ColumnDef::new(Alias::new("undeclared_column")).text()).to_owned();
    reopened.db.execute(DbBackend::Sqlite.build(&altered)).await?;
    reopened.db.clone().close().await?;
    let rejected = store(path).await.err().ok_or("physical drift was admitted")?;
    assert!(rejected.contains("physical schema changed"), "{rejected}");

    let unmanaged = RelationalStore::open(AdapterConfig { connection_string: ":memory:".into(), ..Default::default() }).await?;
    let schema = Parent::collection_schema();
    let declared = super::relational_schema::compile(&schema)?;
    unmanaged.db.execute(DbBackend::Sqlite.build(&declared.create)).await?;
    let rejected = unmanaged.ensure(schema).await.expect_err("unmanaged existing table must require migration");
    assert!(rejected.contains("no declared-schema receipt"), "{rejected}");
    Ok(())
}

#[tokio::test]
async fn colliding_index_names_refuse_adoption_and_roll_back_schema() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("index-collision.db");
    let path = file.to_str().ok_or("test path encoding")?;
    let owner = store(path).await?;
    let declaration = |collection: &str, index: &str| CollectionSchema {
        collection: collection.into(), fields: base_entity_fields(),
        indexes: vec![SchemaIndex { name: index.into(), fields: vec!["version".into()], unique: true, descending: false }],
    };
    owner.ensure(declaration("index_owner", "shared_index_name")).await?;
    assert!(owner.ensure(declaration("index_victim", "shared_index_name")).await.is_err(),
        "an index owned by another table must not silently satisfy this declaration");
    assert!(owner.schema("index_victim").is_err());
    // The failed DDL transaction must leave neither a table nor a receipt behind.
    // A repaired declaration is admitted without an offline migration.
    let repaired = declaration("index_victim", "victim_version_unique");
    owner.ensure(repaired.clone()).await?;
    owner.create(record("first", "index_victim", json!({}))).await?;
    assert!(owner.create(record("second", "index_victim", json!({}))).await.is_err(),
        "the declared unique index must actually constrain writes");
    owner.db.clone().close().await?;
    let reopened = store(path).await?;
    reopened.ensure(repaired).await?;
    assert_eq!(reopened.count(QueryBuilder::new("index_victim").build()).await?, 1);
    Ok(())
}

// A misdeclared enum predicate used to return zero rows rather than report that its
// stored JSON representation differed from the scalar comparison. Wrong-type queries
// must not become a false absence (including count, delete and empty collections).
#[tokio::test]
async fn predicates_use_declared_scalar_codecs_before_execution() -> Result<(), Box<dyn std::error::Error>> {
    let store = store(":memory:").await?;
    store.insert(&store.db, &record("a", Child::COLLECTION,
        json!({"parentId":null,"token":"pending","seq":7,"body":"pending","joinedLabel":null}))).await?;
    assert_eq!(store.count(QueryBuilder::new(Child::COLLECTION).filter_eq("token", "pending").build()).await?, 1);
    for query in [
        QueryBuilder::new(Child::COLLECTION).filter_eq("body", "pending").build(),
        QueryBuilder::new(Child::COLLECTION).filter_eq("seq", "7").build(),
        QueryBuilder::new(Child::COLLECTION).filter("seq", QueryOperator::In(vec![json!(7), json!("8")])).build(),
        QueryBuilder::new(Child::COLLECTION).filter("body", QueryOperator::Contains("pending".into())).build(),
    ] {
        assert!(store.count(query.clone()).await.is_err());
        assert!(store.delete_matching(query).await.is_err());
    }
    assert_eq!(store.count(QueryBuilder::new(Child::COLLECTION).build()).await?, 1);
    assert_eq!(store.count(QueryBuilder::new(Child::COLLECTION).filter_eq("parentId", serde_json::Value::Null).build()).await?, 1);
    store.delete_matching(QueryBuilder::new(Child::COLLECTION).build()).await?;
    assert!(store.count(QueryBuilder::new(Child::COLLECTION).filter_eq("seq", "7").build()).await.is_err());
    Ok(())
}