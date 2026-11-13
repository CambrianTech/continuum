//! A checkpoint field that is DERIVED is a cache, never the record: her context view is re-rendered
//! at a new epoch from her working memory, and an act's declared effects only decide whether her
//! view shows it as a repeat. A derived field that no longer decodes (its shape changed in a later
//! build) is dropped like an absent one, with a probe naming its type, and never fails the checkpoint
//! it rides in: on 2026-10-31 a new field on the view's events (#5639) made every resident's
//! checkpoint undecodable and the M5 hosted no one for 32 minutes ("workspace registration failed:
//! missing field `level`"). Nothing old is read here; the stale value is discarded.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

/// A derived field's deserializer: the value when it decodes, else none (and a probe saying which
/// type and why). The value is read as its exact source text first (`RawValue`), never as a
/// `serde_json::Value`: a view's element ids are u128, which a `Value` cannot hold ("number out of
/// range"), so a `Value` intermediate would refuse every valid view (Codex on #5659, executed test).
pub(crate) fn or_none<'de, D: Deserializer<'de>, T: DeserializeOwned>(deserializer: D) -> Result<Option<T>, D::Error> {
    let Some(raw) = Option::<Box<RawValue>>::deserialize(deserializer)? else {
        return Ok(None);
    };
    match serde_json::from_str::<T>(raw.get()) {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            continuum_base::probe!(
                class = "persona.checkpoint.derived_dropped",
                derived = std::any::type_name::<T>(),
                error = %error,
                "a checkpoint's derived field no longer decodes (its shape changed); it is a cache, so it is dropped and rebuilt"
            );
            Ok(None)
        }
    }
}
