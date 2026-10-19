//! Durable backing for the tracked-root registry, through the node-local ORM.
use crate::orm::entity::BaseEntity;
use crate::orm::Entity;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::paths::native::NativePath;

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "disk_cache_roots")]
pub(super) struct RootRecord {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub class: String,
    #[entity(json)]
    pub path: NativePath,
    #[serde(default)]
    pub volume_id: Option<u64>,
    #[serde(default)]
    pub citizen: Option<uuid::Uuid>,
}

inventory::submit! {
    crate::orm::entity::ProtectedCollection {
        collection: <RootRecord as crate::orm::OrmEntity>::COLLECTION,
        owning_command: "system/pressure-broker-state",
    }
}

impl RootRecord {
    pub(super) fn new(class: &str, path: &Path) -> Self {
        Self {
            base: BaseEntity::for_new_record(),
            class: class.into(),
            path: NativePath::encode(path),
            volume_id: None,
            citizen: None,
        }
    }
}
