//! AircRemoteInferenceAdapter — "the same command across the wire."
//!
//! Joel (2026-05-31): "grid inference and they're just the same
//! command just executed across the wire and airc substrate
//! delivered payloads."
//!
//! ### Architectural contract
//!
//! Implements `AIProviderAdapter` whose transport is airc instead
//! of llama.cpp. Callers see no difference between:
//!
//! ```ignore
//! // LOCAL — LlamaCppAdapter on Apple Silicon via Metal
//! let response = adapter.generate_text(request).await?;
//!
//! // REMOTE — AircRemoteInferenceAdapter routing over airc to a peer
//! let response = remote_adapter.generate_text(request).await?;
//! ```
//!
//! Both impls return `TextGenerationResponse`. Everything above the
//! adapter trait (handle store, lane coordinator, RAG inspection,
//! persona response, chat module, sentinel review) treats remote
//! and local identically.
//!
//! ### Module layout
//!
//! - [`protocol`] — wire types (`RemoteInferenceRequest`,
//!   `RemoteInferenceResponse`, `RemoteInferenceError`). Pure data
//!   + serde + ts-rs.
//! - [`transport`] — `AircInferenceTransport` trait (one method:
//!   `send_request`) + a stub for tests. Production impl that
//!   speaks to a live airc daemon is its own slice (task #108
//!   follow-up); the trait shape is stable.
//! - [`adapter`] — `AircRemoteInferenceAdapter` implementing
//!   `AIProviderAdapter`. Wraps any `AircInferenceTransport` Arc.
//!
//! ### Doctrine alignment
//!
//! - [[inference-is-an-adapter-always-in-the-loop]] — the remote
//!   adapter is a peer impl of the same trait; cloud / local /
//!   heuristic / remote-grid all expose the same surface.
//! - [[airc-headers-are-the-routing-layer]] — the wire envelope
//!   includes typed metadata (correlation_id, persona_id, peer
//!   target hint) so routing decisions happen on headers, not on
//!   payload inspection.
//! - [[host-the-seemingly-impossible]] — this is the substrate's
//!   structural answer for the Intel Mac and any
//!   constrained-locally host. Reflective work runs locally on
//!   the heuristic; real-model work routes over airc to whichever
//!   peer has capacity.

pub mod adapter;
pub mod protocol;
pub mod transport;

pub use adapter::{AircRemoteInferenceAdapter, AIRC_REMOTE_PROVIDER_ID};
pub use protocol::{RemoteInferenceError, RemoteInferenceRequest, RemoteInferenceResponse};
pub use transport::{
    AircInferenceTransport, AircLiveTransport, LocalAdapterTransport, StubInferenceTransport,
};
