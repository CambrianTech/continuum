//! Her tool head's state as an ORM entity: one row per mind, keyed by her persona id, in her
//! own store (`engrams.sqlite`), written once per epoch at turnover and read at boot.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::orm::{BaseEntity, Entity, OrmStore, OrmStoreError};

use super::ToolHeadState;

/// One mind's learned tool head: her calls, her epochs, what this epoch holds and has loaded,
/// and the learned weights.
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "tool_heads")]
pub struct ToolHeadRecord {
    #[serde(flatten)]
    pub base: BaseEntity,

    /// Whose head this is; also the row id, so there is one row per mind.
    #[entity(unique, indexed)]
    pub persona_id: Uuid,

    #[entity(json)]
    pub state: ToolHeadState,
}

/// Her head's persistence: the row keyed by her persona id.
#[derive(Clone)]
pub struct ToolHeadStore {
    store: Arc<OrmStore<ToolHeadRecord>>,
    persona_id: Uuid,
}

impl std::fmt::Debug for ToolHeadStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolHeadStore").field("persona_id", &self.persona_id).finish()
    }
}

impl ToolHeadStore {
    pub fn new(store: Arc<OrmStore<ToolHeadRecord>>, persona_id: Uuid) -> Self {
        Self { store, persona_id }
    }

    /// Her saved head, if a previous life kept one.
    pub async fn load(&self) -> Result<Option<ToolHeadState>, OrmStoreError> {
        Ok(self.store.find_by_id(self.persona_id).await?.map(|record| record.state))
    }

    /// Keep `state` as her head's row: insert the first time, update after.
    pub async fn keep(&self, state: ToolHeadState) -> Result<(), OrmStoreError> {
        let exists = self.store.find_by_id(self.persona_id).await?.is_some();
        let mut base = BaseEntity::for_new_record();
        base.id = self.persona_id.to_string();
        let record = ToolHeadRecord { base, persona_id: self.persona_id, state };
        if exists {
            self.store.update(self.persona_id, &record).await
        } else {
            self.store.save(self.persona_id, &record).await
        }
    }
}
