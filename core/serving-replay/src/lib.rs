//! serving-replay: slice S0 of `docs/architecture/SERVING-ENGINE.md` (plan line 3b).
//!
//! Replays act sequences against an engine behind [`target::ServingTarget`] and reports
//! per-act reuse, fresh prefill tokens, page-ins by kind, relaunches and wall time — in
//! seconds, with no residents and no core. Every later serving slice proves its gain here
//! before it reaches a live node. See the crate README for how to run it.

pub mod acts;
pub mod captures;
pub mod engine_process;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod fake;
pub mod http;
pub mod llama_server;
pub mod model;
pub mod replay;
pub mod report;
pub mod seated;
pub mod seats;
pub mod synthetic;
pub mod target;
