//! KV serving slots as the third implementer of THE one paging engine.
//!
//! **The law this module exists under (Joel, 2026-08-26):** *"we made a paging
//! system for reuse across many things … instead of 10 different codes doing
//! compression or paging, one exists and all trait implementers know HOW this
//! is done for their concern."* MoE experts and the LoRA genome already page
//! through [`PagedResourcePool`]; a llama-server slot — one warm conversation's
//! KV — is the same shape: a scarce residency under contention with a
//! measurable cost of loss. So this module is a thin CONCERN adapter over the
//! shared engine, never a second lease/eviction implementation
//! ([[one-paging-engine-many-trait-implementers]], KV-CACHE-ECONOMY §6: "KV
//! slot residency implements the same shape rather than inventing a rival").
//!
//! **And the key law:** *"we do NOT use strings for keys, we use roomId … the
//! moment you start prepending strings to uuids is the moment it falls
//! apart."* The unit of warmth is the MIND — one resident's acts in every room
//! share one page ([`PageKey::Mind`]); a fork acting beside her keeps a page per
//! room ([`PageKey::Fork`]) — and the key is that typed enum of UUIDs. Its
//! predecessor, a
//! `format!("{persona}@{room}")` string, lived half a day; the
//! `no_string_composite_id_keys_in_serving` ratchet keeps it dead.
//!
//! What the engine gives this concern for free: single-flight assignment,
//! pin-during-use (mid-turn eviction immunity, wired when the request path
//! pins), hit/miss/eviction stats, `ResourcePool` for the `PressureBroker`,
//! and an [`EvictionPriority`] seam where the PRICED policy
//! (`cost_of_loss = tail_tokens / prefill_rate`, plan slice B5) drops in
//! without new machinery. What this concern supplies: the typed key, the
//! slot-index free list (RAII — an evicted activity's index returns on drop),
//! and the per-server directory.

use std::sync::Arc;

use parking_lot::Mutex;
use uuid::Uuid;

use std::sync::atomic::{AtomicU64, Ordering};

use crate::paging::pool::{PagedResourcePool, PinHandle, PoolConfig};

mod state_sampler;
#[cfg(test)]
use state_sampler::StateSampler;
mod citizen_slots;
pub use citizen_slots::*;
mod page_key;
pub use page_key::*;
mod mind_gate;
pub(crate) use mind_gate::MindTurn;
mod kv_slot_pool;
pub use kv_slot_pool::*;
mod saved_ledger;
pub(crate) use saved_ledger::SavedLedger;
use saved_ledger::{LedgerSnapshot, LoadedLedger};
#[cfg(test)]
use saved_ledger::{SavedLedgerFile, LEDGER_FORMAT, SAVED_LEDGER_FILE};
mod engine_generation;
mod relaunch_discard;
pub(crate) use relaunch_discard::{DiscardReason, PendingDiscard};
use relaunch_discard::DiscardLog;
pub use engine_generation::*;
mod placement;
pub use placement::*;
pub(crate) mod page_io;
pub(crate) use page_io::admit as admit_page_io;
pub use page_io::PageIoSettle;

#[cfg(test)]
mod tests;
