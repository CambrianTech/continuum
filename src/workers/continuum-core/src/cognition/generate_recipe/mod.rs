//! `cognition::generate_recipe` — Rust implementation of LLM-driven recipe generation.
//!
//! Migrating `commands/recipe/generate/server/RecipeGenerateServerCommand.ts` (371 LOC)
//! to Rust per the oxidization mission (continuum#1295 / #1248 umbrella). Same shape
//! as #1289 (ProposalRatingAdapter): pure-functions slice first, IPC handler in PR-2,
//! TS shim collapse in PR-3.
//!
//! ## What's in PR-1 (this slice)
//!
//! - `types.rs`     — RecipeTemplateInfo, RecipeGenerateHints, RecipeGenerationRequest,
//!   RecipeGenerationResponse (ts-rs camelCase exports)
//! - `prompt.rs`    — build_recipe_system_prompt + build_recipe_user_prompt mirror the
//!   TS buildSystemPrompt/buildUserPrompt byte-for-byte
//! - `parser.rs`    — parse_recipe_from_ai_response extracts the JSON envelope
//! - `validator.rs` — validate_recipe_structure does structural validation (uniqueId
//!   format, required fields, valid enums, role schema, in-request duplicate check).
//!   Does NOT do filesystem collision check; that stays TS-side because it's pure FS
//!   state.
//!
//! ## What's coming (PR-2 / PR-3)
//!
//! - PR-2: IPC command `cognition/generate-recipe` wiring `AIProviderRegistry::generate_text`
//!   to PR-1's prompt+parser+validator.
//! - PR-3: TS shim collapse — RecipeGenerateServerCommand.ts becomes a thin shim that
//!   gathers templates + existing recipe IDs, calls Rust, then does FS collision check
//!   + file I/O on the success path.
//!
//! ## Why pure-functions-first
//!
//! Same outlier-validation strategy that worked for rate_proposals (#1289 → PR
//! #1290+#1291+#1293): proving the prompt+parser+validator match TS byte-for-byte
//! BEFORE the IPC layer lands means PR-2 is a wiring change, not a logic change.
//!
//! ## Why no fallback
//!
//! Per #1262 (no-CPU-fallback audit), the TS path's silent error-on-malformed-JSON
//! returns `{ success: false, error: '...' }`. The Rust path returns `Err` — the
//! JTAG shim can choose to surface that as the same TS error envelope (preserving
//! CommandBase contract) without losing diagnostic info.

pub mod orchestrator;
pub mod parser;
pub mod prompt;
pub mod types;
pub mod validator;

pub use orchestrator::{generate_recipe_with_ai, GenerateRecipeOrchestratorParams};
pub use parser::{parse_recipe_from_ai_response, ParseError};
pub use prompt::{build_recipe_system_prompt, build_recipe_user_prompt};
pub use types::{
    RecipeDefinitionShape, RecipeGenerateHints, RecipeGenerationRequest,
    RecipeGenerationResponse, RecipeTemplateInfo,
};
pub use validator::{validate_recipe_structure, ValidationError};
