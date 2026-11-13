#![cfg(test)]
    use super::*;
    // ---- Source trait + stub tests ----

    fn persona() -> uuid::Uuid {
        uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    }

    // what this catches: a source sizing itself (FLOW plan §2d). The stub delivers every item
    // it holds whatever is large, and refuses another persona's ctx with nothing.
    #[tokio::test]
    async fn a_source_delivers_every_item_and_only_to_its_persona() {
        let items = vec![RagItem::whole("a", "a", serde_json::json!({})), RagItem::whole("huge", "x".repeat(40_000), serde_json::json!({}))];
        let source = StubRagSource::new("stub", persona(), items);
        let delivery = source.deliver(&RagContext::for_persona(persona(), 1_000_000)).await;
        assert_eq!(delivery.items.len(), 2);
        assert!(!delivery.truncated);
        let maya = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000def").unwrap();
        assert!(source.deliver(&RagContext::for_persona(maya, 1_000_000)).await.items.is_empty());
    }
