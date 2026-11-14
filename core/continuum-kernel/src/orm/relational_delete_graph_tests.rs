use super::*;
use crate::orm::{
    adapter::AdapterConfig,
    binary::{BinaryDescriptor, BinaryValue, ByteOrder, ElementType},
    relational::RelationalStore,
    types::*,
    QueryBuilder,
};
use sea_orm::EntityTrait;
use serde_json::json;

fn field(name: &str, kind: FieldType, nullable: bool) -> SchemaField {
    SchemaField {
        name: name.into(),
        field_type: kind,
        nullable,
        indexed: true,
        unique: false,
        max_length: None,
        foreign_key: None,
        default_value: None,
    }
}
fn schema(
    name: &str,
    native: bool,
    reference: Option<(&str, &str, CascadeRule)>,
) -> CollectionSchema {
    let mut fields = crate::orm::base_entity_fields();
    if native {
        fields.push(field("payload", FieldType::Binary, true));
    }
    let mut name_field = field("name", FieldType::String, false);
    name_field.unique = true;
    fields.push(name_field);
    if let Some((target, column, rule)) = reference {
        let mut parent = field("parent", FieldType::String, true);
        parent.foreign_key = Some(ForeignKeyRef {
            collection: target.into(),
            field: column.into(),
            on_delete: rule,
            on_update: CascadeRule::Restrict,
        });
        fields.push(parent);
    }
    CollectionSchema {
        collection: name.into(),
        fields,
        indexes: Vec::new(),
    }
}
fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}
async fn native(
    store: &RelationalStore,
    collection: &str,
    n: u128,
    parent: Option<String>,
) -> Result<(), String> {
    let payload = BinaryValue::new(
        BinaryDescriptor {
            element_type: ElementType::U8,
            byte_order: ByteOrder::Little,
            shape: vec![4096],
            producer: "cascade-fixture".into(),
            unit: false,
            scale: None,
        },
        vec![0xa5; 4096],
    )?;
    store
        .create_binary(
            DataRecord {
                id: id(n),
                collection: collection.into(),
                data: json!({"name":id(n),"parent":parent}),
                metadata: RecordMetadata::default(),
            },
            std::collections::BTreeMap::from([("payload".into(), payload)]),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn reopened_cascade_graph_preserves_restrict_set_null_and_native_tombstones(
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::orm::relational_native_revision::Entity as Revisions;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("cascade.sqlite");
    let config = AdapterConfig {
        connection_string: path.to_str().ok_or("non-UTF8 fixture")?.into(),
        ..Default::default()
    };
    let store = RelationalStore::open(config.clone()).await?;
    let root = schema("cascadeRoot", false, None);
    for declared in [
        root.clone(),
        schema(
            "cascade_child",
            true,
            Some(("cascade_root", "name", CascadeRule::Cascade)),
        ),
        schema(
            "cascade_grandchild",
            true,
            Some(("cascade_child", "id", CascadeRule::Cascade)),
        ),
        schema(
            "cascade_survivor",
            true,
            Some(("cascade_root", "name", CascadeRule::SetNull)),
        ),
        schema(
            "cascade_blocker",
            false,
            Some(("cascade_root", "id", CascadeRule::Restrict)),
        ),
    ] {
        store.ensure(declared).await?;
    }
    store
        .create(DataRecord {
            id: id(1),
            collection: "cascadeRoot".into(),
            data: json!({"name":"root-key"}),
            metadata: RecordMetadata::default(),
        })
        .await?;
    for n in 10..113 {
        native(&store, "cascade_child", n, Some("root-key".into())).await?;
    }
    native(&store, "cascade_grandchild", 200, Some(id(10))).await?;
    native(&store, "cascade_survivor", 300, Some("root-key".into())).await?;
    store
        .create(DataRecord {
            id: id(400),
            collection: "cascade_blocker".into(),
            data: json!({"name":"blocker","parent":id(1)}),
            metadata: RecordMetadata::default(),
        })
        .await?;
    drop(store);
    let store = RelationalStore::open(config.clone()).await?;
    // Only the parent caller re-admits its code declaration; restored child
    // declarations must still participate in the physical cascade's receipt.
    store.ensure(root).await?;
    let delete = || {
        QueryBuilder::new("cascadeRoot")
            .filter_eq("id", id(1))
            .build()
    };
    assert!(store.delete_matching(delete()).await.is_err());
    assert!(
        !Revisions::find_by_id(("cascade_child".to_owned(), id(10)))
            .one(&store.db)
            .await?
            .ok_or("missing live receipt")?
            .deleted
    );
    store
        .delete_matching(QueryBuilder::new("cascade_blocker").build())
        .await?;
    assert_eq!(store.delete_matching(delete()).await?, 1);
    assert_eq!(
        store
            .count(QueryBuilder::new("cascade_child").build())
            .await?,
        0
    );
    assert_eq!(
        store
            .count(QueryBuilder::new("cascade_grandchild").build())
            .await?,
        0
    );
    for n in 10..113 {
        let receipt = Revisions::find_by_id(("cascade_child".to_owned(), id(n)))
            .one(&store.db)
            .await?
            .ok_or("missing child tombstone")?;
        assert!(receipt.deleted);
        assert_eq!(receipt.revision, 104);
    }
    let survivor = Revisions::find_by_id(("cascade_survivor".to_owned(), id(300)))
        .one(&store.db)
        .await?
        .ok_or("missing survivor")?;
    assert!(!survivor.deleted);
    assert_eq!(survivor.revision, 1);
    let rows = store
        .query(
            QueryBuilder::new("cascade_survivor")
                .select(vec!["parent".into()])
                .limit(1)
                .build(),
        )
        .await?;
    assert_eq!(rows[0].data["parent"], serde_json::Value::Null);
    drop(store);
    let store = RelationalStore::open(config).await?;
    assert!(
        Revisions::find_by_id(("cascade_grandchild".to_owned(), id(200)))
            .one(&store.db)
            .await?
            .ok_or("missing durable descendant")?
            .deleted
    );
    Ok(())
}

#[tokio::test]
async fn cyclic_cascade_deduplicates_identities_and_uses_one_revision(
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::orm::relational_native_revision::Entity as Revisions;
    let store = RelationalStore::open(AdapterConfig {
        connection_string: ":memory:".into(),
        ..Default::default()
    })
    .await?;
    store
        .ensure(schema(
            "cascade_cycle",
            true,
            Some(("cascade_cycle", "id", CascadeRule::Cascade)),
        ))
        .await?;
    native(&store, "cascade_cycle", 1, None).await?;
    native(&store, "cascade_cycle", 2, Some(id(1))).await?;
    store
        .write(|store, transaction| {
            Box::pin(async move {
                store
                    .update_on(
                        transaction,
                        "cascade_cycle",
                        &id(1),
                        json!({"parent":id(2)}),
                        true,
                    )
                    .await
            })
        })
        .await?;
    store
        .delete_matching(
            QueryBuilder::new("cascade_cycle")
                .filter_eq("id", id(1))
                .build(),
        )
        .await?;
    assert_eq!(
        store
            .count(QueryBuilder::new("cascade_cycle").build())
            .await?,
        0
    );
    for n in [1, 2] {
        let receipt = Revisions::find_by_id(("cascade_cycle".to_owned(), id(n)))
            .one(&store.db)
            .await?
            .ok_or("missing cycle receipt")?;
        assert!(receipt.deleted);
        assert_eq!(receipt.revision, 3);
    }
    Ok(())
}
