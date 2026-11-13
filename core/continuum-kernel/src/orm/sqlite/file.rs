//! Opening a SQLite file owned by an ORM store.

use super::SqliteAdapter;
use crate::orm::adapter::{AdapterConfig, StorageAdapter};

impl SqliteAdapter {
    /// Open (creating if absent) the SQLite database file at `path`: THE way a store
    /// that owns its own file (a persona home's engrams, a peer home's grant
    /// watermark) gets an adapter, so the open is written once, not per store.
    pub async fn open(path: &std::path::Path) -> Result<Self, String> {
        let connection_string = path
            .to_str()
            .ok_or_else(|| format!("database path {} is not valid UTF-8", path.display()))?
            .to_string();
        let mut adapter = Self::new();
        adapter
            .initialize(AdapterConfig {
                connection_string,
                ..AdapterConfig::default()
            })
            .await?;
        Ok(adapter)
    }
}
