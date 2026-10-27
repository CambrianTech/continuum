//! WebSocket Call Server
//!
//! Handles live audio/video calls over WebSocket.
//! Each call has multiple participants, audio is mixed with mix-minus.

use continuum_base::audio_constants::AUDIO_SAMPLE_RATE;
use crate::live::audio::capabilities::ModelCapabilityRegistry;
use crate::live::audio::mixer::{AudioMixer, ParticipantStream};
use crate::live::audio::router::{AudioRouter, RoutedParticipant};
use crate::live::audio::stt;
use crate::live::types::FrameKind;
use crate::live::video::source::{TestPatternSource, VideoSource};
use crate::runtime::handle::Handle;
use crate::live::audio::pcm::{
    base64_decode_i16, bytes_to_i16, i16_to_f32, is_silence,
};
use crate::{clog_error, clog_info, clog_warn};
use futures_util::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock, Semaphore};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use ts_rs::TS;

mod text_preview_length;
pub use text_preview_length::*;
mod call_manager;
mod call_manager_2;
mod handle_connection;
pub use handle_connection::*;

#[cfg(test)]
mod tests;
