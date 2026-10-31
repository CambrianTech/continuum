//! Dormancy — whether a mind whose home is on this node is awake, as a durable fact.
//!
//! Joel's population rule (2026-10-25): "born on request, awake by engagement within the
//! node's measured capacity, dormant otherwise". The M5 went from sixteen homes to four
//! residents. Until this module, dormancy was only the operator's EXPIRING roster hold
//! ([`super::roster_hold`], at most a day): it lapsed at 2026-10-27 00:16Z, the hosting
//! reconciler resumed every home from disk on the next restart, and ten minds shared two
//! lanes (each resident got about one request per fifteen minutes).
//!
//! A hold is an operator's measurement window; dormancy is a mind's own state. It is one
//! ORM row per mind ([`PersonaWakefulness`], keyed by her peer id), so it survives restarts
//! and never expires. No row = Awake (every newborn, every home before the migration).
//!
//! - The reconciler's draw skips a Dormant mind (`spawner_module::draw_intents`) and the
//!   seat count excludes her, so a dormant home is never resumed on boot.
//! - `persona/instances/despawn --dormant` records her Dormant, then takes her offline.
//! - `persona/instances/wake --name` records her Awake and hands her back to the draw.
//! - An explicit `persona/spawn` mints a new mind: she has no row, she is Awake.
//!
//! The ledger owns the rows; this process holds a copy of the dormant set (`HELD`) for the
//! reconciler's synchronous draw, replaced only after a write commits. Until the ledger
//! loads, the draw resumes no home from disk (fail closed); `persona/spawn` still births.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::identity::PeerId;
use crate::orm::{OrmStore, OrmStoreError, StorageAdapter};

/// The reason the first boot with durable dormancy writes for every home the hold kept down.
pub const MIGRATION_REASON: &str = "population decision 2026-10-25, migrated from the expiring hold";

/// A mind's wakefulness. Awake is the default (no row); Dormant names when, why and who.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Wakefulness {
    #[serde(rename_all = "camelCase")]
    Awake { since_ms: u64, by: String },
    #[serde(rename_all = "camelCase")]
    Dormant { since_ms: u64, reason: String, by: String },
}

/// One mind's wakefulness row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "persona_wakefulness")]
pub struct PersonaWakefulness {
    /// Her peer id: her identity, and the directory name of her home.
    #[entity(primary_key)]
    pub id: PeerId,
    /// Her name as the draw reads it (case-insensitive).
    #[entity(indexed)]
    pub agent_name: String,
    #[entity(json)]
    pub state: Wakefulness,
    /// When this row last changed; the upsert keeps the newest change.
    pub changed_at_ms: u64,
}

impl PersonaWakefulness {
    pub fn is_dormant(&self) -> bool {
        matches!(self.state, Wakefulness::Dormant { .. })
    }
}

/// A home on this node, as the identity scan found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Home {
    pub id: PeerId,
    pub agent_name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DormancyError {
    #[error("wakefulness store: {0}")]
    Store(#[from] OrmStoreError),
}

/// The dormant minds this process holds for the draw. `None` until the ledger loads: the
/// draw then resumes NO home from disk (fail closed), because reading "nobody dormant" from
/// a store that never opened replays the 2026-10-27 incident.
static HELD: parking_lot::RwLock<Option<Vec<PersonaWakefulness>>> = parking_lot::RwLock::new(None);

/// The dormant set as the draw sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DormantSet {
    /// The ledger has not loaded (or failed to open): no home is resumed from disk.
    Unavailable,
    Known(Vec<PersonaWakefulness>),
}

impl Default for DormantSet {
    /// A fixture's roster: the ledger answered, nobody sleeps.
    fn default() -> Self {
        DormantSet::Known(Vec::new())
    }
}

impl DormantSet {
    pub fn contains(&self, agent_name: &str) -> bool {
        match self {
            DormantSet::Known(rows) => rows.iter().any(|r| r.agent_name.eq_ignore_ascii_case(agent_name)),
            DormantSet::Unavailable => false,
        }
    }

    /// How many known dormant homes come off the seat count (none known while unavailable:
    /// the draw blocks every resumed home instead).
    pub fn len(&self) -> usize {
        match self {
            DormantSet::Known(rows) => rows.len(),
            DormantSet::Unavailable => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The dormant set this process holds.
pub fn dormant_set() -> DormantSet {
    match HELD.read().as_ref() {
        Some(rows) => DormantSet::Known(rows.clone()),
        None => DormantSet::Unavailable,
    }
}

/// Every known dormant mind (none while the ledger is unavailable).
pub fn dormant() -> Vec<PersonaWakefulness> {
    match dormant_set() {
        DormantSet::Known(rows) => rows,
        DormantSet::Unavailable => Vec::new(),
    }
}

pub fn is_dormant(agent_name: &str) -> bool {
    dormant_set().contains(agent_name)
}

/// Who changed her wakefulness, for the record: the authenticated caller, or the owner when
/// the node's own code (the CLI, the operator seat) called with no caller identity.
pub fn caller_label(ctx: &crate::sdk_codegen::Ctx) -> String {
    match &ctx.caller {
        Some(caller) => format!("peer {}", caller.peer_id),
        None => "owner (local)".to_string(),
    }
}

/// The durable owner of every mind's wakefulness.
pub struct DormancyLedger {
    adapter: Arc<dyn StorageAdapter>,
    store: OrmStore<PersonaWakefulness>,
}

impl DormancyLedger {
    pub async fn open(adapter: Arc<dyn StorageAdapter>) -> Result<Self, DormancyError> {
        let store = OrmStore::new(Arc::clone(&adapter)).await?;
        Ok(Self { adapter, store })
    }

    /// Boot: read every row, migrate from the hold when the table is empty, and hold the
    /// dormant minds whose homes are on this node. Probes each mind it keeps down.
    pub async fn load(&self, homes: &[Home], migration: Migration<'_>) -> Result<Vec<PersonaWakefulness>, DormancyError> {
        let mut rows: Vec<PersonaWakefulness> = self.store.find_all().await?.into_iter().map(|(_, r)| r).collect();
        if rows.is_empty() && !homes.is_empty() {
            rows = migrate_from_hold(homes, migration);
            let ops = rows.iter().map(|r| self.store.create_op(r.id.as_uuid(), r)).collect::<Result<Vec<_>, _>>()?;
            crate::orm::store::commit(&self.adapter, ops).await?;
            continuum_base::probe!(
                class = "persona.dormancy.migrated",
                homes = homes.len() as u64,
                dormant = rows.iter().filter(|r| r.is_dormant()).count() as u64,
                held_by = migration.hold.map(|h| h.reason.as_str()).unwrap_or(""), // unwrap_or: no hold = nothing kept down, prose only
                "first boot with durable dormancy: every home not on the hold and not online is dormant"
            );
        }
        let dormant: Vec<PersonaWakefulness> = rows
            .into_iter()
            .filter(|r| r.is_dormant() && homes.iter().any(|h| h.id == r.id))
            .collect();
        for r in &dormant {
            if let Wakefulness::Dormant { reason, by, since_ms } = &r.state {
                continuum_base::probe!(
                    class = "persona.dormancy.kept_down",
                    agent = %r.agent_name,
                    reason = %reason,
                    by = %by,
                    since_ms = *since_ms,
                    "a dormant home: the reconciler will not resume her until she is woken"
                );
            }
        }
        *HELD.write() = Some(dormant.clone());
        Ok(dormant)
    }

    /// Record her wakefulness (the newest change wins), then update the held set.
    pub async fn set(&self, id: PeerId, agent_name: &str, state: Wakefulness, now_ms: u64) -> Result<(), DormancyError> {
        let row = PersonaWakefulness { id, agent_name: agent_name.to_string(), state, changed_at_ms: now_ms };
        self.store.save_monotonic(id.as_uuid(), &row, "changedAtMs").await?;
        // Only a loaded set is updated: a write never marks an unloaded ledger as answered.
        if let Some(held) = HELD.write().as_mut() {
            held.retain(|r| r.id != id);
            if row.is_dormant() {
                held.push(row);
            }
        }
        Ok(())
    }

}

/// What the first boot with durable dormancy migrates from.
#[derive(Clone, Copy)]
pub struct Migration<'a> {
    /// The LAST hold this node recorded, standing or lapsed (`roster_hold::last_recorded`):
    /// the population decision outlives the hold's window, so a first boot after the lapse
    /// still migrates it. `None` = no hold was ever recorded: every home is Awake. Only an
    /// exclusive (operator) hold keeps anyone down.
    pub hold: Option<&'a super::roster_hold::RosterHold>,
    /// Names online when the migration runs (none at boot).
    pub online: &'a [String],
    pub now_ms: u64,
}

/// The rows the first boot writes: a home the last recorded hold allows, or that is online,
/// is Awake; every other home is Dormant, whether or not the hold has lapsed. Without an
/// exclusive hold ever recorded, every home is Awake. Pure.
pub fn migrate_from_hold(homes: &[Home], m: Migration<'_>) -> Vec<PersonaWakefulness> {
    let hold = m.hold.filter(|h| h.exclusive);
    homes
        .iter()
        .map(|home| {
            let online = m.online.iter().any(|n| n.eq_ignore_ascii_case(&home.agent_name));
            let state = match hold {
                Some(h) if !online && !h.allows(&home.agent_name) => Wakefulness::Dormant {
                    since_ms: m.now_ms,
                    reason: MIGRATION_REASON.to_string(),
                    by: format!("migration of the hold '{}'", h.reason),
                },
                _ => Wakefulness::Awake { since_ms: m.now_ms, by: "migration".to_string() },
            };
            PersonaWakefulness { id: home.id, agent_name: home.agent_name.clone(), state, changed_at_ms: m.now_ms }
        })
        .collect()
}

#[cfg(test)]
mod tests;
