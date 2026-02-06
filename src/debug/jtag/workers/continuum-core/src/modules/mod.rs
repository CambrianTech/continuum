/// Service Modules — ServiceModule implementations for each domain.
///
/// Each module wraps existing domain logic behind the ServiceModule trait.
/// The runtime routes commands and events to the correct module automatically.
///
/// Phase 1: health (trivial outlier — validates interface)
/// Phase 2: cognition, channel (per-persona DashMap — most different outlier)
/// Phase 3: voice, code, memory, models (remaining core domains)
/// Phase 4: data, embedding, inference, search, training, logger (absorb external workers)

pub mod health;
pub mod cognition;
pub mod channel;
pub mod models;
pub mod memory;
pub mod voice;
pub mod code;
pub mod rag;
