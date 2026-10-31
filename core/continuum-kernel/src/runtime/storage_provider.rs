//! The node's storage, reached by any module without naming its owner's concrete type.
//!
//! Six modules found the data module in the registry and downcast it to its concrete type just to
//! resolve an adapter (card 39f08cda), so none of them could leave the core. The data module
//! offers this trait instead ([`ServiceModule::as_storage`](super::ServiceModule::as_storage)),
//! and a module binds to it through [`ModuleContext::storage`](super::ModuleContext::storage).
//! Binding does no I/O, so a module that resolves later (a transient disk failure retried by its
//! own tick) keeps that timing.

use std::sync::Arc;

use crate::orm::StorageAdapter;

/// Named storage adapters (`"main"`, `"@node"`, ...), resolved on demand. The same handle always
/// resolves to the same shared adapter: the owner caches one per backend.
#[async_trait::async_trait]
pub trait StorageProvider: Send + Sync {
    async fn adapter(&self, handle: &str) -> Result<Arc<dyn StorageAdapter>, String>;
}
