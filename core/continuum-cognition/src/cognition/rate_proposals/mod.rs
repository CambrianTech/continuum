//! `cognition::rate_proposals` — Rust implementation of peer-review proposal rating.
//!
//! Migrating `system/user/server/modules/cognition/ProposalRatingAdapter.ts` (252 LOC)
//! to Rust per the oxidization mission (continuum#1289 / #1248 umbrella). Joel
//! 2026-05-15: "mission to eliminate slop and slowly oxidize this project (turn to rust)."
//!
//! ## What's in this PR (PR-1)
//!
//! Pure-functions-first slice — types + prompt builder + parser. No IPC wiring,
//! no AI-call integration, no TS shim changes. Each piece is fully tested in
//! Rust against fixture inputs the TS version generated, so behavior parity
//! is provable before the IPC layer lands.
//!
//! ## What's coming (PR-2 / PR-3)
//!
//! - PR-2: IPC command `cognition/rate-proposals` that wires the existing
//!   `AIProviderRegistry::select` + `adapter.generate_text` chain to the
//!   prompt+parser shipped here. Ts-rs export of the request/response types.
//! - PR-3: TS shim collapse — `ProposalRatingAdapter.ts` becomes a thin
//!   `Commands.execute('cognition/rate-proposals', ...)` shim. ESLint baseline
//!   drops by the deletion line count.

pub mod orchestrator;
pub mod parser;
pub mod prompt;
pub mod types;

pub use orchestrator::{rate_proposals_with_ai, RateProposalsRequest, RateProposalsResponse};
pub use parser::{parse_ratings_from_ai_response, ParseConfig};
pub use prompt::build_rating_prompt;
pub use types::{ProposalRating, RatingContext, RatingMessage, ResponseProposal};
