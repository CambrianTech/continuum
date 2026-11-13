//! Slot work responsibility of the llama-server process owner.
use super::*;
/// The slot a persona's in-flight Turn generation is pinned to (card 6f3218ed): the engine
/// root and the `id_slot` the admission lease named. Registered where the adapter pins the
/// slot, cleared when that generation ends, and read by the act deadline, which then asks
/// whether THIS slot is advancing (a per-request signal, not the engine-wide fingerprint:
/// a request lost inside a busy engine must still end).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InFlightSlot {
    pub root: String,
    pub slot: u32,
}


pub(super) static IN_FLIGHT: std::sync::LazyLock<dashmap::DashMap<uuid::Uuid, InFlightSlot>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);


/// Holds a persona's in-flight registration for the life of one generation.
pub(crate) struct InFlightSlotGuard {
    pub(super) persona: uuid::Uuid,
}


impl InFlightSlotGuard {
    pub(crate) fn register(persona: uuid::Uuid, root: &str, slot: u32) -> Self {
        IN_FLIGHT.insert(persona, InFlightSlot { root: root.trim_end_matches('/').to_string(), slot });
        Self { persona }
    }
}


impl Drop for InFlightSlotGuard {
    fn drop(&mut self) {
        IN_FLIGHT.remove(&self.persona);
    }
}


/// The slot `persona`'s generation is running on right now, if one is pinned.
pub fn in_flight_slot(persona: uuid::Uuid) -> Option<InFlightSlot> {
    IN_FLIGHT.get(&persona).map(|e| e.value().clone())
}


/// One slot as `/slots` reports it, for the per-request progress check: the task it serves,
/// whether it is processing, and its work so far (prompt tokens processed + tokens decoded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotWork {
    pub task: i64,
    pub processing: bool,
    pub work: u64,
}


/// PURE: slot `slot` in a `/slots` body. `None` when the body is not the array or the slot
/// is not in it.
pub(crate) fn slot_work_of(slots: &serde_json::Value, slot: u32) -> Option<SlotWork> {
    let n = |v: &serde_json::Value| v.as_u64().unwrap_or(0); // JUSTIFIED unwrap_or: an absent counter is no work, and progress is a CHANGE
    slots.as_array()?.iter().find(|s| s["id"].as_u64() == Some(u64::from(slot))).map(|s| SlotWork {
        task: s["id_task"].as_i64().unwrap_or(-1), // JUSTIFIED unwrap_or: no task id is its own task (-1), never equal to a real one
        processing: s["is_processing"].as_bool() == Some(true),
        work: n(&s["n_prompt_tokens_processed"]).wrapping_add(n(&s["next_token"][0]["n_decoded"])),
    })
}


/// Read one slot's work from the engine (bounded like every probe here). `None` = the engine
/// did not answer or has no such slot: no evidence of progress.
pub async fn slot_work(root: &str, slot: u32, client: &reqwest::Client) -> Option<SlotWork> {
    let body: serde_json::Value = client
        .get(format!("{root}/slots"))
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    slot_work_of(&body, slot)
}


/// PURE: the act deadline's question at one checkpoint (card 6f3218ed). `previous` is what
/// the slot showed at the last checkpoint, `None` at the first. The first checkpoint only
/// asks that the slot is processing (there is nothing to compare yet); every later one asks
/// that it is the SAME task and its work moved. A new task on the slot means ours ended
/// or was never there; an idle or silent slot is not progress.
pub fn slot_advanced(previous: Option<SlotWork>, now: Option<SlotWork>) -> bool {
    match (previous, now) {
        (_, None) => false,
        (_, Some(n)) if !n.processing => false,
        (None, Some(_)) => true,
        (Some(p), Some(n)) => p.task == n.task && n.work != p.work,
    }
}
