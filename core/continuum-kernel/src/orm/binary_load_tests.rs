#![cfg(test)]

//! Isolated storage-stage replay. This does not simulate GPU search or resident adoption.
use super::{binary::{BinaryDescriptor, BinaryValue, ByteOrder, ElementType}, *};
use super::{adapter::AdapterConfig, relational::RelationalStore};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[path = "binary_delta_load.rs"]
mod delta_load;

#[derive(Serialize, Deserialize, Entity)]
#[entity(collection = "binary_load_native")]
struct Native {
    #[serde(flatten)]
    base: BaseEntity,
    #[entity(binary)]
    payload: Vec<u8>,
}

#[derive(Serialize, Deserialize, Entity)]
#[entity(collection = "binary_load_json_baseline")]
struct JsonBaseline {
    #[serde(flatten)]
    base: BaseEntity,
    #[entity(json)]
    payload: Vec<f32>,
}

fn record(collection: &str, id: String, data: serde_json::Value) -> DataRecord {
    DataRecord { id, collection: collection.into(), data, metadata: RecordMetadata::default() }
}

async fn replay(store: RelationalStore, native: bool, expected: Vec<f32>, bytes: Vec<u8>) -> Result<(), String> {
    let collection = if native { Native::COLLECTION } else { JsonBaseline::COLLECTION };
    let start = Instant::now();
    let mut operations = tokio::task::JoinSet::new();
    // Fixed offered arrivals, not a closed loop that stops submitting during stalls.
    for sequence in 0..128_u32 {
        let scheduled = start + Duration::from_millis(u64::from(sequence) * 5);
        tokio::time::sleep_until(tokio::time::Instant::from_std(scheduled)).await;
        let store = store.clone();
        let expected = expected.clone();
        let bytes = bytes.clone();
        operations.spawn(async move {
            let execution_start = Instant::now();
            let cursor = uuid::Uuid::from_u128((u128::from(sequence) * 73) % 9900);
            let rows;
            if native {
                let mut contiguous = Vec::new();
                let mut ids = Vec::new();
                let snapshot = store.binary_snapshot(collection).await?;
                let descriptor = snapshot.page_into(
            &crate::orm::binary::BinaryPageRequest { field: "payload", after: Some(cursor), limit: std::num::NonZeroUsize::new(32).ok_or("test page size must be positive")? },
            crate::orm::binary::BinaryPageOutput { bytes: &mut contiguous, ids: &mut ids },
        ).await?
                    .ok_or("missing native descriptor")?;
                rows = ids.len();
                if descriptor.byte_len()? != bytes.len() || contiguous.len() != rows * bytes.len() {
                    return Err("native replay shape mismatch".to_string());
                }
                for row in contiguous.chunks_exact(bytes.len()) {
                    if row != bytes { return Err("native replay byte mismatch".to_string()); }
                }            } else {
                let query = QueryBuilder::new(collection).filter("id", QueryOperator::Gt(cursor.to_string().into())).limit(32).build();
                let page = store.query(query).await?;
                rows = page.len();
                for mut row in page {
                    // Reproduce the former corpus consumer's numeric materialization.
                    let values: Vec<f32> = serde_json::from_value(row.data["payload"].take()).map_err(|e| e.to_string())?;
                    if values != expected { return Err("JSON replay value mismatch".to_string()); }
                }
            }
            if rows != 32 { return Err("replay page cardinality mismatch".to_string()); }
            Ok((scheduled.elapsed().as_secs_f64() * 1000.0, execution_start.elapsed().as_secs_f64() * 1000.0))
        });
    }
    let mut arrival_ms = Vec::new();
    let mut execution_ms = Vec::new();
    while let Some(result) = operations.join_next().await {
        let (arrival, execution) = result.map_err(|e| e.to_string())??;
        arrival_ms.push(arrival);
        execution_ms.push(execution);
    }
    arrival_ms.sort_by(f64::total_cmp);
    execution_ms.sort_by(f64::total_cmp);
    println!("STORAGE_REPLAY kind={} rows=10003 dimensions=768 page=32 offered=128 completed={} failed=0 interval_ms=5 state=filesystem_warm validation_included=true elapsed_ms={:.3} arrival_p50_ms={:.3} arrival_p99_ms={:.3} execution_p50_ms={:.3} execution_p99_ms={:.3}",
        if native { "native" } else { "json_baseline" }, arrival_ms.len(), start.elapsed().as_secs_f64() * 1000.0,
        arrival_ms[63], arrival_ms[126], execution_ms[63], execution_ms[126]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit isolated storage replay; run beside serving only with measured resource headroom"]
async fn native_vs_json_storage_replay() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("replay.db");
    let store = RelationalStore::open(AdapterConfig {
        connection_string: path.to_str().ok_or("non-UTF8 fixture path")?.into(), ..Default::default()
    }).await?;
    store.ensure(Native::collection_schema()).await?;
    store.ensure(JsonBaseline::collection_schema()).await?;
    // Deterministic offline fixture generation, never a production storage codec.
    let expected: Vec<f32> = (0..768).map(|i| ((i * 17 % 251) as f32 - 125.0) / 128.0).collect();
    let bytes: Vec<u8> = expected.iter().flat_map(|value| value.to_ne_bytes()).collect();
    let descriptor = BinaryDescriptor { element_type: ElementType::F32,
        byte_order: if cfg!(target_endian = "little") { ByteOrder::Little } else { ByteOrder::Big },
        shape: vec![768], producer: "isolated-replay/seed17-mod251".into(), unit: false, scale: None };
    let json_payload = serde_json::json!({"payload": expected});
    for first in (0..10003).step_by(100) {
        let mut native_statements = Vec::new();
        let mut json_statements = Vec::new();
        for n in first..(first + 100).min(10003) {
            native_statements.push(store.insert_statement_with_binary(
                &record(Native::COLLECTION, uuid::Uuid::from_u128(n as u128).to_string(), serde_json::json!({})),
                std::collections::BTreeMap::from([("payload".into(), BinaryValue::new(descriptor.clone(), bytes.clone())?)])
            )?);
            json_statements.push(store.insert_statement(&record(JsonBaseline::COLLECTION, uuid::Uuid::from_u128(n as u128).to_string(), json_payload.clone()))?);
        }
        store.write(move |_, transaction| Box::pin(async move {
            use sea_orm::ConnectionTrait;
            for statement in native_statements.into_iter().chain(json_statements) {
                transaction.execute(sea_orm::DbBackend::Sqlite.build(&statement)).await.map_err(|e| e.to_string())?;
            }
            Ok(())
        })).await?;
    }
    // Alternate ordering to make cache/order effects visible rather than choose one winning run.
    for native in [false, true, true, false] {
        replay(store.clone(), native, expected.clone(), bytes.clone()).await?;
    }
    delta_load::replay(&store, &descriptor, &bytes).await?;
    Ok(())
}
