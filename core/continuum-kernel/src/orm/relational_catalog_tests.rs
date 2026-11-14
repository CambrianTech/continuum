#![cfg(test)]

use super::*;

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
    let initial = RelationalStore::open(config()).await?;
    // More than two transfer pages; empty tables still require declaration verification.
    for index in 0..131 {
        let mut schema = Parent::collection_schema();
        schema.collection = format!("catalog_{index:03}");
        initial.ensure(schema).await?;
    }
    initial.db.clone().close().await?;
    let started = std::time::Instant::now();
    let restored = RelationalStore::open(config()).await?;
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
    let failure = RelationalStore::open(config())
        .await
        .err()
        .ok_or("late drift was admitted")?;
    assert!(failure.contains("catalog_130"), "{failure}");
    assert!(failure.contains("physical schema changed"), "{failure}");
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
        failure.contains("unknown field `unsupportedCollation`"),
        "{failure}"
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
        serde_json::from_value::<CollectionSchema>(canonical.clone())?,
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
        if object
            .get("foreignKey")
            .is_some_and(serde_json::Value::is_null)
        {
            object.remove("foreignKey");
        }
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
    let initial = RelationalStore::open(config()).await?;
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
    let restored = RelationalStore::open(config()).await?;
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
    let error = RelationalStore::open(config())
        .await
        .err()
        .ok_or("corrupt late revision was admitted")?;
    assert!(error.contains("nonpositive persisted revision"), "{error}");
    Ok(())
}
