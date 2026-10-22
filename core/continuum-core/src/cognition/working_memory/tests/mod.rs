#![cfg(test)]
use super::*;

mod actions;
mod custody;
mod faculty;
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
