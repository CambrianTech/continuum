//! Production [`PersonaConversation`] impl wrapping
//! `Arc<dyn AircCitizen>` — slice 11 of #133, re-shaped in slice 13.5
//! around the [`AircCitizen`] trait.
//!
//! This is where the substrate's transport-agnostic loop
//! ([`super::service_loop::serve_persona_loop`]) meets the live airc
//! daemon. The conversation trait stays the loop's boundary; this
//! struct is the one place the substrate calls
//! [`AircCitizen::subscribe`] / [`AircCitizen::say`] /
//! [`AircTranscriptReader::page_recent`] directly. Holding
//! `Arc<dyn AircCitizen>` instead of the concrete runtime keeps the
//! production projection symmetric with whatever stub a future test
//! plugs in.
//!
//! ## Why slice 11 isn't in slice 10
//!
//! - **Testability**: slice 10's loop runs against a stub
//!   conversation; if its `next_message` / `say` / `high_water_mark`
//!   needed an airc daemon, the loop wouldn't be unit-testable. The
//!   PersonaConversation trait gives slice 10 a no-daemon contract;
//!   slice 11 fulfills that contract for production.
//! - **Cleanly bisectable**: when the substrate misbehaves later, we
//!   know whether the loop logic broke (slice 10's tests) or the
//!   airc transport broke (slice 11's smoke path).
//!
//! ## Non-text events
//!
//! `next_message` filters out events with no text body. Binary
//! attachments, control envelopes, and image messages don't reach
//! the service loop — the slice-10 contract is text-in / text-out
//! today. Vision + audio land in later slices via separate
//! conversation trait methods (per
//! [[ai-namespace-multimodal-crutches]] — multi-modal as first-class
//! peer, not a hack on top of the text path).
//!
//! ## Subscribe lifecycle
//!
//! The airc subscribe stream is lazy: created on the FIRST call to
//! `next_message`, not at construction. This keeps
//! [`AircPersonaConversation::new`] cheap + infallible — useful for
//! the slice-12 supervisor that constructs one of these per hosted
//! persona at boot, before any of them have necessarily attached to
//! their rooms yet.

use continuum_base::time::EpochMs;
use crate::persona::airc_citizen::AircCitizen;
use crate::persona::service_loop::{IncomingMessage, PersonaConversation};
use airc_core::TranscriptEvent;
use airc_lib::FilteredEventStream;
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use super::durable_history::event_from_row;

mod polled;
pub use polled::*;
mod inbox_capacity;
pub(crate) use inbox_capacity::*;

#[cfg(test)]
mod tests;
