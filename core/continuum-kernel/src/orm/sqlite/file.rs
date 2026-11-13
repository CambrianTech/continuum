//! Opening a SQLite file a store owns, and the writer handle an in-`orm/` migration
//! runs its transaction on.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

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

    /// The writer connection, for an in-`orm/` migration that must run its statements in
    /// one transaction on the same file ([`crate::orm::legacy_grant_watermark`]).
    pub(in crate::orm) fn writer(&self) -> Result<Arc<Mutex<Connection>>, String> {
        self.get_writer()
    }
}
