//! VoiceModule — wraps voice synthesis, transcription, and call management.
//!
//! Handles: voice/register-session, voice/on-utterance, voice/should-route-tts,
//!          voice/synthesize, voice/speak-in-call, voice/synthesize-handle,
//!          voice/play-handle, voice/discard-handle, voice/transcribe,
//!          voice/transcribe-with-adapter, voice/stt-list,
//!          voice/test-audio-generate,
//!          voice/inject-audio, voice/ambient-add, voice/ambient-inject,
//!          voice/ambient-remove, voice/poll-transcriptions,
//!          voice/set-cognitive-state,
//!          voice/snapshot-room, voice/snapshot-participant
//!
//! Priority: Realtime — voice operations are time-critical.

use continuum_base::time::EpochMs;
use crate::live::audio::buffer::AudioBufferPool;
use crate::live::audio::resource_lifecycle::AudioResourceLifecycle;
use crate::live::session::voice_service::VoiceService;
use crate::live::transport::bridge_client::LiveKitAgentManager;
use crate::live::{UtteranceEvent, VoiceParticipant};
use crate::logging::TimingGuard;
use crate::runtime::{CommandExecutor, CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::runtime::params::Params;
use crate::{log_error, log_info};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

mod voice_response_field_responder_ids;
pub use voice_response_field_responder_ids::*;
mod voice_module;

#[cfg(test)]
mod tests;
