//! Bridge Client — proxies LiveKit operations to livekit-bridge via Unix socket IPC.
//!
//! Drop-in replacement for LiveKitAgentManager. Same public API, but sends commands
//! to the livekit-bridge process over a Unix socket instead of linking webrtc-sys.
//!
//! Bidirectional: sends commands (request/response), receives pushed events
//! (audio frames from human participants for VAD/STT processing).

use crate::time::EpochMs;
use continuum_bridge_protocol::{BridgeCommand, BridgeEvent, BridgeResponse};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
// Same framed media protocol on both platforms: Unix socket or Windows loopback.
#[cfg(windows)]
use std::net::TcpStream as UnixStream;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use crate::{clog_info, clog_warn};

/// Re-export for compatibility.
pub use continuum_bridge_protocol::{ParticipantMetadata, ParticipantRole};

mod transcription_entry;
pub use transcription_entry::*;
mod live_kit_agent_manager;
pub use live_kit_agent_manager::*;
mod reader_loop;
pub use reader_loop::*;
