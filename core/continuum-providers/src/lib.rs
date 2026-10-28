//! Shared provider registration for resident cores and serving clients.
//! Callers supply their command surface and governed budget; this crate does
//! not depend on core or the serving orchestrator.
pub use continuum_engine::{ai, inference, model_registry};
pub use continuum_kernel::{gpu, runtime, sdk_codegen};
pub mod ai_provider;
pub use ai_provider::{AIProviderModule, ProviderServices};
