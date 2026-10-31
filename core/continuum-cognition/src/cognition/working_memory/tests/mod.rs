#![cfg(test)]
use super::*;

mod action_log;
mod actions;
mod custody;
mod open_views;
mod pins;
mod provenance;
mod rebuilt_marker;
mod trail;

/// A receipt saved under `handle` (what `spill::LivePin::retain` returns in production).
fn saved(handle: &str) -> crate::ai::types::ResultCustody {
    crate::ai::types::ResultCustody::Whole { handle: handle.into() }
}

/// Record `text` as a saved receipt with no typed acts: the shape a settlement collapse
/// acts on. An unsaved receipt never collapses.
fn record_saved(wm: &WorkingMemory, text: &str) {
    wm.record_receipt_typed(&[], text, None, saved("ring-test"));
}

/// A test body's size in bytes: big enough that a shed below a few of them is visible. The
/// size is the test's choice; production has no such number (the memory owner sheds).
const BODY: usize = 4_096;

/// Shed `wm`'s resident bodies down to `budget` bytes, as the memory owner's reporter does,
/// with every handle's saved output vouched for.
fn shed(wm: &WorkingMemory, budget: usize) {
    wm.evict_resident_results_over(budget as u64, &|_, _| true);
}
