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
        // Route through the ONE kinds-filtered impl on `airc_lib::Airc`
        // (persona/airc_source.rs, #297) — never the raw inherent page.
        crate::persona::airc_source::AircTranscriptReader::page_recent(&*self.inner, limit).await
    }
}

#[async_trait]
impl crate::persona::room_roster_source::AircRosterReader for AircHandleAdapter {
    fn self_peer_id(&self) -> airc_core::PeerId {
        self.inner.peer_id()
    }

    async fn room_roster(
        &self,
        within: std::time::Duration,
        window: usize,
    ) -> Result<Vec<airc_lib::RoomMember>, AircError> {
        self.inner.room_roster(within, window).await
    }

    // #262: forward the CARDS read to the real airc identity join. Without
    // this override the adapter silently inherits the trait's identity-less
    // default and every roster name regresses to the provisional peer label
    // (glass-boxed live 2026-07-30 — the whole room went `peer-xxxx` for one
    // deploy cycle).
    async fn room_roster_cards(
        &self,
        within: std::time::Duration,
        window: usize,
    ) -> Result<Vec<airc_lib::RoomMemberCard>, AircError> {
        self.inner.room_roster_cards(within, window).await
    }
}

#[async_trait]
impl crate::persona::room_doctrine_source::AircDoctrineReader for AircHandleAdapter {
    async fn room_doctrine(
        &self,
    ) -> Result<Option<airc_core::doctrine::RoomDoctrinePublished>, AircError> {
        self.inner.room_doctrine().await
    }
}

#[async_trait]
impl crate::persona::wall_source::WallReader for AircHandleAdapter {
    async fn wall_posts(
        &self,
    ) -> Result<Vec<airc_core::doctrine::WallPostPublished>, AircError> {
        // Whole board (all categories); the source filters/labels per post.
        self.inner.wall_posts(None).await
    }
}

#[async_trait]
impl crate::persona::active_work_source::AircWorkReader for AircHandleAdapter {
    async fn active_claims(&self) -> Result<Vec<airc_lib::WorkCard>, AircError> {
        let status = self
            .inner
            .work_roster_status(airc_lib::WorkRosterQuery::default())
            .await?;
        let me = self.inner.peer_id();
        Ok(status
            .rows
            .into_iter()
            .find(|r| r.peer == me)
            .map(|r| r.active_claims)
            .unwrap_or_default())
    }
}

#[async_trait]
impl crate::persona::room_board_source::RoomBoardReader for AircHandleAdapter {
    /// The current room's WHOLE work board — delegates to the inner airc
    /// handle's single board fold (same read the desktop-app kanban projector
    /// makes).
    async fn work_board(&self) -> Result<airc_work::BoardSnapshot, AircError> {
        crate::persona::room_board_source::RoomBoardReader::work_board(self.inner.as_ref()).await
    }

    /// Delegates to the inner airc handle's alias store — the same durable
    /// lookup the operator CLI uses, so a card holder reads as a person here
    /// too. Delegation, never a "no names" stub: an adapter that quietly
    /// dropped resolution would restore the hex-only board this exists to fix.
    async fn peer_names(
        &self,
        peers: &[airc_core::PeerId],
    ) -> std::collections::HashMap<airc_core::PeerId, String> {
        crate::persona::room_board_source::RoomBoardReader::peer_names(self.inner.as_ref(), peers)
            .await
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
