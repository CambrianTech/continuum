//! `AircHandleAdapter` — bridges `Arc<airc_lib::Airc>` into the
//! substrate's `dyn AircCitizen + dyn AircTranscriptReader` surface.
//!
//! `PersonaAircRuntime` carries persona-specific lifecycle state
//! (default_room, source, inbound_handle) and impls `AircCitizen` ON
//! ITSELF. Every non-persona kind (Agent — Claude / Codex / Gemini /
//! etc., future Jtag, Human, Web) needs the same `Arc<Airc>` →
//! `dyn AircCitizen` shape WITHOUT those persona-specific fields.
//!
//! Slice 3 (PR #1524) introduced this adapter as a private inner
//! struct in `context/claude.rs`. Slice 4 lifts it to a shared
//! module so every non-persona kind reuses one impl per CLAUDE.md
//! outlier discipline.

use std::sync::Arc;

use airc_core::EventId;
use airc_lib::{Airc, AircError};
use async_trait::async_trait;
use uuid::Uuid;

use crate::persona::airc_citizen::AircCitizen;
use crate::persona::airc_source::AircTranscriptReader;

/// Wraps an `Arc<airc_lib::Airc>` so it can be handed to substrate
/// surfaces expecting `Arc<dyn AircCitizen>`. Stateless; delegates
/// every call directly.
pub struct AircHandleAdapter {
    inner: Arc<Airc>,
}

impl AircHandleAdapter {
    pub fn new(inner: Arc<Airc>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl AircTranscriptReader for AircHandleAdapter {
    async fn page_recent(
        &self,
        limit: usize,
    ) -> Result<Vec<airc_lib::TranscriptEvent>, AircError> {
        self.inner.page_recent(limit).await
    }
}

#[async_trait]
impl AircCitizen for AircHandleAdapter {
    fn peer_id(&self) -> Uuid {
        self.inner.peer_id().as_uuid()
    }

    async fn subscribe(&self) -> Result<airc_lib::EventStream, AircError> {
        self.inner.subscribe().await
    }

    async fn say(&self, text: &str) -> Result<EventId, AircError> {
        self.inner.say(text).await
    }
}
