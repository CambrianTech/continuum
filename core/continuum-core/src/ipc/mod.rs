use crate::code::{FileEngine, ShellSession};
use crate::gpu::GpuMemoryManager;
use crate::modules::ai_provider::AIProviderModule;
use crate::modules::airc::AircModule;
use crate::modules::airc_bridge_directive::AircBridgeDirectiveModule;
use crate::modules::airc_bridge_dispatch::AircBridgeDispatchModule;
use crate::modules::auth::ExternalWebviewAuthModule;
use continuum_live::consumers::avatar::AvatarModule;
use crate::modules::channel::{ChannelModule, ChannelState};
use crate::modules::code::{CodeModule, CodeState};
use crate::modules::cognition::{CognitionModule, CognitionState};
use crate::modules::dataset::DatasetModule;
use crate::modules::embedding::EmbeddingModule;
use crate::modules::events::EventsModule;
use crate::modules::forge::ForgeModule;
use crate::modules::gpu::GpuModule;
use crate::modules::grid::GridModule;
use crate::modules::health::HealthModule;
use crate::modules::launch_mode::LaunchModeModule;
use crate::modules::live::{VoiceModule, VoiceState};
use crate::modules::logger::LoggerModule;
use crate::modules::memory::{MemoryModule, MemoryState};
use crate::modules::models::ModelsModule;
use crate::modules::persona_allocator::PersonaAllocatorModule;
use crate::modules::rag::{RagModule, RagState};
use continuum_sentinel::SentinelModule;
use crate::modules::system_resources::SystemResourceModule;
use crate::modules::vision::VisionModule;
/// IPC server for continuum-core
///
/// Unix socket server that accepts JSON requests and returns JSON responses.
/// Follows the same pattern as logger worker - event-driven, no polling.
///
/// Architecture:
/// - One thread per connection (spawn on accept)
/// - Tokio async for concurrent request handling
/// - JSON protocol (JTAGRequest/JTAGResponse)
/// - Performance timing on every request
/// - Modular runtime routes commands through ServiceModule trait (Phase 1+)
use crate::persona::ChannelRegistry;
use crate::cognition::conversation::types::PersonaState;
use crate::rag::RagEngine;
use crate::runtime::{CommandResult, Runtime};
use crate::system_resources::SystemResourceMonitor;
use crate::{log_debug, log_error, log_info};
use dashmap::DashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
// Unix-domain socket is the primary IPC transport on Unix. On Windows there is
// no Unix socket; the server binds ONLY the TCP loopback listener (below) and
// local callers connect over TCP. These imports are Unix-only.
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

// ============================================================================
// Request/Response Protocol + Memory Diagnostics
// ============================================================================
// Split out of this file 2026-05-18 — see ipc/protocol.rs (InboxMessageRequest,
// Response) and ipc/diagnostics.rs (per-command RSS tracking). Re-exported
// here so existing call sites resolve unchanged.
pub mod command_ingress;
pub mod diagnostics;

pub mod endpoint_paths;

pub mod experience_resolver;


pub mod positron_canvas_source;

pub mod positron_dispatch;

pub mod positron_foundry_source;

pub mod positron_kanban_source;

pub mod positron_live_source;

pub mod positron_metrics_source;

pub mod positron_nav_source;

pub mod positron_presence;

pub mod positron_serving_source;

pub mod positron_source;

pub mod positron_wall_source;

pub mod protocol;

pub mod provider_bridge;

pub mod recipe_room_purpose;



pub mod vitals_emitter;

pub mod ws;

use diagnostics::{current_rss_mb, dump_memory_report, log_command_rss_delta};

pub use continuum_mind::commands::cognition::inbox_message_request::InboxMessageRequest;

use protocol::Response;

// See modules/health.rs, cognition.rs, channel.rs, voice.rs, code.rs, memory.rs,
// models.rs, data.rs, logger.rs, search.rs, embedding.rs, rag.rs for command handlers.

mod prepare_unix_socket_path;
use prepare_unix_socket_path::*;
mod start_server;
#[cfg(test)]
pub(crate) use start_server::install_kernel_extensions;
pub use start_server::*;

#[cfg(test)]
mod tests;
