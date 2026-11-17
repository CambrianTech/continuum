//! Offline replay against a PRIVATE COPY of a quiescent database snapshot.
use continuum_kernel::orm::{adapter::StorageAdapter, OrmStore, SeaOrmAdapter};
use futures::TryStreamExt;
use std::{path::PathBuf, sync::Arc};

#[tokio::test]
#[ignore = "requires ENGRAM_SNAPSHOT naming a stopped-runtime database snapshot"]
async fn typed_snapshot_reopens_without_losing_rows() -> Result<(), Box<dyn std::error::Error>> {
    let source = PathBuf::from(std::env::var("ENGRAM_SNAPSHOT")?);
    let directory = tempfile::tempdir()?;
    let target = directory.path().join("engrams.sqlite");
    std::fs::copy(&source, &target)?;
    for suffix in ["-wal", "-shm"] {
        let sibling = PathBuf::from(format!("{}{suffix}", source.display()));
        if sibling.exists() { std::fs::copy(sibling, format!("{}{suffix}", target.display()))?; }
    }
    let mut prior = None;
    for _ in 0..2 {
        let started = std::time::Instant::now();
        let mut adapter = Arc::new(SeaOrmAdapter::open(&target).await?);
        let engrams = OrmStore::<crate::engram::Engram>::new(adapter.clone()).await?;
        let metadata = OrmStore::<crate::recall_metadata::EngramRecallMetadata>::new(adapter.clone()).await?;
        let mut counts = (0usize, 0usize);
        {
            let mut rows = engrams.scan().await?;
            while rows.try_next().await?.is_some() { counts.0 += 1; }
            let mut rows = metadata.scan().await?;
            while rows.try_next().await?.is_some() { counts.1 += 1; }
        }
        if let Some(previous) = prior { assert_eq!(counts, previous); }
        println!("TYPED_SNAPSHOT rows={} metadata={} elapsed_us={}", counts.0, counts.1, started.elapsed().as_micros());
        prior = Some(counts);
        drop(engrams);
        drop(metadata);
        Arc::get_mut(&mut adapter).ok_or("adapter still borrowed")?.close().await?;
    }
    Ok(())
}
