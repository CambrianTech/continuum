use super::*;

#[tokio::test]
async fn required_native_fields_create_atomically_without_body_readback(
) -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("native-pair.sqlite");
    let path = path.to_str().ok_or("test path is not UTF-8")?;
    let store = open(path).await?;
    store.ensure(RequiredNativeRow::collection_schema()).await?;
    let record = || DataRecord {
        id: uuid::Uuid::from_u128(1).to_string(),
        collection: RequiredNativeRow::COLLECTION.into(),
        data: json!({"label":"typed scalar"}),
        metadata: RecordMetadata::default(),
    };
    let payloads = || -> Result<_, String> {
        Ok(std::collections::BTreeMap::from([
            (
                "payload".into(),
                BinaryValue::new(
                    BinaryDescriptor {
                        unit: true,
                        scale: Some(0.125),
                        ..descriptor(ElementType::I8, vec![2])
                    },
                    vec![0, 8],
                )?,
            ),
            (
                "sketch".into(),
                BinaryValue::new(descriptor(ElementType::Bit, vec![2]), vec![0])?,
            ),
        ]))
    };
    assert!(store.create(record()).await.is_err());
    let mut incomplete = payloads()?;
    incomplete.remove("sketch");
    assert!(store.create_binary(record(), incomplete).await.is_err());
    assert_eq!(
        store
            .count(QueryBuilder::new(RequiredNativeRow::COLLECTION).build())
            .await?,
        0
    );
    assert_eq!(
        store.create_binary(record(), payloads()?).await?,
        uuid::Uuid::from_u128(1).to_string()
    );
    assert!(store.create_binary(record(), payloads()?).await.is_err());
    let page = read_page(&store, RequiredNativeRow::COLLECTION, "payload", None, 1).await?;
    assert_eq!(
        page[0]
            .value
            .as_ref()
            .ok_or("missing required payload")?
            .bytes(),
        &[0, 8]
    );
    let query = QueryBuilder::new(RequiredNativeRow::COLLECTION)
        .select(vec!["label".into()])
        .limit(1)
        .build();
    let rows = store.query(query).await?;
    assert_eq!(rows[0].data["label"], "typed scalar");
    assert!(rows[0].data.get("payload").is_none());
    drop(store);
    let store = open(path).await?;
    store.ensure(RequiredNativeRow::collection_schema()).await?;
    for (field, expected) in payloads()? {
        let page = read_page(&store, RequiredNativeRow::COLLECTION, &field, None, 1).await?;
        assert_eq!(page[0].value.as_ref(), Some(&expected));
    }
    // One producer result contains both its compute payload and its coarse sketch.
    // A late invalid field must not commit an earlier valid payload or its revision.
    let changed = || -> Result<_, String> {
        Ok(std::collections::BTreeMap::from([
            (
                "payload".into(),
                BinaryValue::new(descriptor(ElementType::I8, vec![2]), vec![3, 4])?,
            ),
            (
                "sketch".into(),
                BinaryValue::new(descriptor(ElementType::Bit, vec![2]), vec![3])?,
            ),
        ]))
    };
    let revision = store.revision(
        super::relational_sequence::Domain::Native,
        RequiredNativeRow::COLLECTION,
    );
    let mut invalid = changed()?;
    invalid.insert(
        "zzz_undeclared".into(),
        BinaryValue::new(descriptor(ElementType::U8, vec![1]), vec![1])?,
    );
    assert!(store
        .update_binary(
            RequiredNativeRow::COLLECTION.into(),
            record().id,
            invalid,
            BinaryWriteCondition::Replace
        )
        .await
        .is_err());
    assert_eq!(*revision.borrow(), 1);
    for (field, expected) in payloads()? {
        let page = read_page(&store, RequiredNativeRow::COLLECTION, &field, None, 1).await?;
        assert_eq!(page[0].value.as_ref(), Some(&expected));
    }
    assert!(
        !store
            .update_binary(
                RequiredNativeRow::COLLECTION.into(),
                record().id,
                changed()?,
                BinaryWriteCondition::IfAbsent
            )
            .await?
    );
    assert_eq!(*revision.borrow(), 1);
    assert!(
        store
            .update_binary(
                RequiredNativeRow::COLLECTION.into(),
                record().id,
                changed()?,
                BinaryWriteCondition::Replace
            )
            .await?
    );
    assert_eq!(*revision.borrow(), 2);
    assert!(store
        .update_binary(
            RequiredNativeRow::COLLECTION.into(),
            record().id,
            std::collections::BTreeMap::new(),
            BinaryWriteCondition::Replace
        )
        .await
        .is_err());
    drop(store);
    let store = open(path).await?;
    for (field, expected) in changed()? {
        let page = read_page(&store, RequiredNativeRow::COLLECTION, &field, None, 1).await?;
        assert_eq!(page[0].value.as_ref(), Some(&expected));
    }
    assert_eq!(
        *store
            .revision(
                super::relational_sequence::Domain::Native,
                RequiredNativeRow::COLLECTION
            )
            .borrow(),
        2
    );
    Ok(())
}

