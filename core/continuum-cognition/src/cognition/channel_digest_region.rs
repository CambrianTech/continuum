//! What a mind's channel digest is built from and published into: the persona/channel source and the
//! shared digest buffer that grounding, nav and vitals read.
//!
//! The digest itself is built by her channel digest component (continuum-mind,
//! `persona::senses::digest`), woken when her room moves; the governor-ticked region that pre-staged it
//! on a clock is gone (object model 5.7). Building a digest runs no inference: element embeddings are
//! lazy ([`crate::cognition::channel_element::ChannelElement::embedding`]).

use std::sync::Arc;

use uuid::Uuid;

use crate::cognition::channel_digest::ChannelDigest;
use crate::cognition::conversation::transcript_reader::AircTranscriptReader;
use crate::runtime::ready_buffer::DashMapReadyBuffer;


/// A persona's transcript reader and the room (channel) to digest. Implemented on
/// `PersonaAircRuntimeRegistry` in production; stubbed in tests so no live airc daemon is needed.
pub trait PersonaChannelReader: Send + Sync {
    /// The live personas.
    fn live_personas(&self) -> Vec<Uuid>;
    /// A persona's transcript reader and the room (channel) to digest, if live. `None` when the
    /// persona has no live runtime.
    fn reader_and_room(&self, persona_id: Uuid) -> Option<(Arc<dyn AircTranscriptReader>, Uuid)>;
}

/// The shared ready buffer: (persona, room) to the freshest digest.
pub type DigestBuffer = DashMapReadyBuffer<(Uuid, Uuid), Arc<ChannelDigest>>;
