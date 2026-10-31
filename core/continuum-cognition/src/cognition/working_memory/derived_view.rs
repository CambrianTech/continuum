//! A checkpoint's context view is DERIVED: her view is re-rendered at a new epoch from her working
//! memory, so it is a cache of what she last saw, never the record of it. A view that no longer
//! decodes (its shape changed in a later build) is dropped like an absent one, with a probe, and her
//! next act opens a new epoch. It never fails the checkpoint it rides in: on 2026-10-31 a new field on
//! the view's events (#5639) made every resident's checkpoint undecodable and the M5 hosted no one
//! for 32 minutes ("workspace registration failed: missing field `level`"). Nothing old is read here;
//! the stale view is discarded.

use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

use super::ContextView;

/// `context_view`'s deserializer: the view when it decodes, else none (and a probe saying why).
/// The view is read as its exact source text first (`RawValue`), never as a `serde_json::Value`:
/// a view's element ids are u128, which a `Value` cannot hold ("number out of range"), so a
/// `Value` intermediate would refuse every valid view (Codex on #5659, executed test).
pub(crate) fn or_new_epoch<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<ContextView>, D::Error> {
    let Some(raw) = Option::<Box<RawValue>>::deserialize(deserializer)? else {
        return Ok(None);
    };
    match serde_json::from_str::<ContextView>(raw.get()) {
        Ok(view) => Ok(Some(view)),
        Err(error) => {
            continuum_base::probe!(
                class = "persona.checkpoint.view_dropped",
                error = %error,
                "a checkpoint's context view no longer decodes (its shape changed); it is derived, so it is dropped and her next act opens a new epoch"
            );
            Ok(None)
        }
    }
}
