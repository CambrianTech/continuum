//! SAVED STATE: one contract for every piece of state that outlives a process
//! (docs/architecture/SAVED-STATE.md).
//!
//! Before this crate each owner wrote its own read, write, validate and fallback, and they differed
//! mostly in how silently they failed: four engine rate stores replaced a corrupt file with
//! defaults and ignored a failed rename, so a node could plan from "measured" numbers it never
//! measured. Here an owner DECLARES what its state is ([`SavedState`]) and the trunk does the rest
//! once:
//!
//! - one envelope `{kind, format, generation, saved_at_ms, build, key, contract, value}` placed
//!   atomically through `continuum_host::durable` under the key's lock; a save returns a receipt
//!   of what it wrote ([`Saved`]: generation and digest), and a read-modify-write is refused when
//!   another writer saved since it read ([`Store::save_over`]);
//! - provenance is recorded, never compared: the build that wrote a value is in its envelope;
//!   whether a value still holds is the kind's declared format and contract, and how far to trust
//!   a measurement is its observer's business, never the medium's;
//! - one typed verdict per load ([`Loaded`]): `Absent | Matched | Upgraded | Diverged | Stale | Newer |
//!   Older | Corrupt | Unrooted`, and one probe class per verdict (`state.load.<outcome>`, the kind a field),
//!   probed when a key's verdict changes, never on every load;
//! - the disk is read once per key in a process's life: the store is the only writer of its
//!   files, so later loads answer from the process's copy and a save replaces it (`memory`);
//! - a failed save never latches the owner dead: the store is asked again on the next save and
//!   reports consecutive failures (`state.save.failed`, then `state.save.recovered`);
//! - one migration seam ([`SavedState::upgrade`]): an older file read into the current value loads
//!   as `Upgraded` and is rewritten by the next save, so moving a store onto the contract keeps
//!   what a node already measured or was told (a serving pin, a knee);
//! - the contract is compared field by field, so a mismatch names the field that changed;
//! - a kind whose state can be re-measured says so in its type ([`Rebuildable`]) and only such a
//!   kind may turn a non-match into an empty value; every other kind matches the verdict;
//! - one generic round trip (`round_trip::every_outcome`, feature `test-fixtures`) that every kind
//!   runs instead of its own save/load/corrupt test.
//!
//! # How KV pages fit (slice 2, not built here)
//!
//! KV pages are the outlier this contract was shaped against. Their bytes are written by ANOTHER
//! process (llama-server) and confirmed by its receipt, so they get a second [`Medium`]:
//! `ExternalBlob`, a store whose `save` records the engine's receipt rather than placing bytes. It
//! keeps the confirmed-save ledger generically: `confirm` binds the operation, the engine
//! incarnation, the slot and the page revision; the entry is invalidated before any overwrite
//! (today's `saved_ledger.rs` invariant, made reusable); a key with bytes but no receipt loads as
//! `Unconfirmed`. `Matched` there means eligible to try, never an engine-confirmed restore or a
//! reused prefix. Its `Contract` is today's `KvPageContract` fingerprint inputs as fields (model
//! and adapter revisions, cache type, flash attention), so a layout change is
//! `Diverged { what: "cache_type" }` instead of an untyped refusal string; the engine incarnation
//! is provenance plus the confirm binding, not a field that rejects every other build. A partly
//! reused restore is `Diverged { at }` only when the engine reports what it reused, never inferred
//! from a low cached count; `at` joins the variant with that report. Fork pages declare a
//! retention bound by their fork's lifetime, so they leave when the fork ends instead of waiting
//! on a byte budget.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod envelope;
mod kind;
mod loaded;
mod memory;
mod provenance;
mod store;

#[cfg(any(test, feature = "test-fixtures"))]
pub mod round_trip;

pub use kind::{adopt_bare, Key, Medium, OwnedFile, Rebuildable, Retention, SavedState, Singleton};
pub use loaded::{CorruptCause, Loaded};
pub use provenance::record_build;
pub use store::{SaveError, Saved, Store};
