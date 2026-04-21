//! Recipe-driven KV context sizing.
//!
//! Per §14 of docs/architecture/PERSONA-CONTEXT-PAGING.md: each task
//! type has a default context budget representing typical demand for
//! the median case. These ship as data here (the registry layer) so
//! adapters / tests / personas declare their needs and the adapter
//! sizes accordingly. No `with_context_length(magic_number)` calls in
//! adapter callers — they declare a recipe and the budget falls out.
//!
//! The budgets are SEEDS for allocation, not caps. The paging policy
//! (§14.2 of the doc) adjusts them up/down based on observed signals
//! at runtime. This module is the static-side of that loop — what the
//! recipe author declares as the starting point.

use serde::{Deserialize, Serialize};

/// What the persona is doing — drives the seed context budget.
///
/// Defaults match §14.1 of the design doc. New variants land here as
/// new task types emerge; the table stays the single source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Text chat — typical multi-party turn fits comfortably.
    Chat,
    /// Voice chat — text small, audio is its own bursty modality.
    VoiceChat,
    /// Video chat — text small, vision adds transient tokens per frame.
    VideoChat,
    /// Coding (small project) — one or two files in context.
    CodingSmall,
    /// Coding (large project / refactor) — many-file navigation.
    CodingLarge,
    /// Game NPC, idle — small persona-state, mostly cold.
    GameNpcIdle,
    /// Game NPC, in-conversation — promoted on player proximity.
    GameNpcEngaged,
    /// Sentinel, easy task — template-driven work.
    SentinelEasy,
    /// Sentinel, hard task — research / analysis work.
    SentinelHard,
    /// Academy student (learning) — reading + practice context.
    AcademyStudent,
}

impl TaskKind {
    /// Default seed context budget for this task kind, in tokens.
    /// The numbers come from §14.1 of the design doc — they represent
    /// the EXPECTED demand for the median case of this task. The
    /// paging policy adjusts at runtime; this is the starting point.
    pub fn default_seed_tokens(self) -> u32 {
        match self {
            TaskKind::Chat => 8 * 1024,
            TaskKind::VoiceChat => 8 * 1024,
            TaskKind::VideoChat => 8 * 1024,
            TaskKind::CodingSmall => 32 * 1024,
            TaskKind::CodingLarge => 128 * 1024,
            TaskKind::GameNpcIdle => 4 * 1024,
            TaskKind::GameNpcEngaged => 16 * 1024,
            TaskKind::SentinelEasy => 16 * 1024,
            TaskKind::SentinelHard => 64 * 1024,
            TaskKind::AcademyStudent => 32 * 1024,
        }
    }

    /// Default maximum the persona would ever scale to for this task.
    /// The paging policy may grow allocation up to this cap based on
    /// demand signals (§14.2 grow signals). Above this, the persona
    /// has to declare a different TaskKind or use Custom budgets.
    pub fn default_max_tokens(self) -> u32 {
        match self {
            // Chat-class: doesn't need to grow much.
            TaskKind::Chat | TaskKind::VoiceChat | TaskKind::VideoChat => 16 * 1024,
            // Coding: small can grow into medium territory; large covers
            // most refactor scenarios but caps at the model's typical max.
            TaskKind::CodingSmall => 64 * 1024,
            TaskKind::CodingLarge => 256 * 1024,
            // Game NPC: idle stays small; engaged can grow as conversation deepens.
            TaskKind::GameNpcIdle => 8 * 1024,
            TaskKind::GameNpcEngaged => 32 * 1024,
            // Sentinel: easy stays bounded; hard can scale into large research.
            TaskKind::SentinelEasy => 32 * 1024,
            TaskKind::SentinelHard => 128 * 1024,
            // Academy: reading-heavy, can grow with material complexity.
            TaskKind::AcademyStudent => 64 * 1024,
        }
    }
}

/// One persona's declared context need within a recipe. The persona
/// declares (or inherits from its task) a min (base, can't function
/// below) and max (won't ever need more for this task).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonaContextBudget {
    pub persona_label: String,
    pub task: TaskKind,
    pub min_tokens: u32,
    pub max_tokens: u32,
}

impl PersonaContextBudget {
    /// Construct from a task kind using the defaults. Recipe author
    /// can override min/max with the builder methods below.
    pub fn for_task(persona_label: impl Into<String>, task: TaskKind) -> Self {
        Self {
            persona_label: persona_label.into(),
            task,
            min_tokens: task.default_seed_tokens(),
            max_tokens: task.default_max_tokens(),
        }
    }

    /// Override the min (base requirement). Used when a specific
    /// persona-task pairing needs more headroom than the task default
    /// (e.g., a memory-NPC that always needs 16K even idle).
    pub fn with_min_tokens(mut self, n: u32) -> Self {
        self.min_tokens = n;
        // min can't exceed max — auto-bump max if caller raised the floor.
        if self.min_tokens > self.max_tokens {
            self.max_tokens = self.min_tokens;
        }
        self
    }

    /// Override the max. Used when a recipe author knows this persona
    /// will scale beyond the task default.
    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n.max(self.min_tokens);
        self
    }
}

/// A recipe's worth of persona budgets. The adapter reads this to
/// size KV at load time (sum of seeds bounded by hardware ceiling),
/// and the paging policy reads it later for per-persona adjust limits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecipeBudget {
    pub personas: Vec<PersonaContextBudget>,
}

impl RecipeBudget {
    pub fn new() -> Self {
        Self { personas: Vec::new() }
    }

    pub fn add_persona(mut self, budget: PersonaContextBudget) -> Self {
        self.personas.push(budget);
        self
    }

    /// Sum of declared minimum (seed) budgets. This is the total KV
    /// the adapter must reserve to even let every persona in the recipe
    /// function at all. The model's actual `n_ctx` should be at least
    /// this amount.
    pub fn sum_of_seed_tokens(&self) -> u32 {
        self.personas.iter().map(|p| p.min_tokens).sum()
    }

    /// Sum of declared maximums. Upper bound on what the recipe will
    /// ever ask for. Useful for the paging policy to know whether
    /// growth signals are even satisfiable on the current hardware.
    pub fn sum_of_max_tokens(&self) -> u32 {
        self.personas.iter().map(|p| p.max_tokens).sum()
    }

    /// Number of personas in the recipe. The adapter uses this to
    /// pick `n_seq_max` for the backend (one slot per persona).
    pub fn persona_count(&self) -> u32 {
        self.personas.len() as u32
    }

    /// True if the seed sum fits the given model's trained context.
    /// If false, the recipe overshoots and the adapter must either
    /// reject the load or shrink per-persona budgets proportionally.
    pub fn fits_in_model_context(&self, model_n_ctx_train: u32) -> bool {
        self.sum_of_seed_tokens() <= model_n_ctx_train
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: regression in a TaskKind's seed default value
    /// (someone bumps Chat from 8K to 16K thinking "more is better"
    /// without realizing it doubles per-persona KV cost). The defaults
    /// are documented in §14.1; changing them requires updating that
    /// section AND this test.
    ///
    /// Validated 2026-04-21: bumped Chat default to 16384, test fails
    /// with clear left/right diff; reverted, passes.
    #[test]
    fn task_kind_default_seeds_match_design_doc_section_14_1() {
        assert_eq!(TaskKind::Chat.default_seed_tokens(), 8 * 1024);
        assert_eq!(TaskKind::VoiceChat.default_seed_tokens(), 8 * 1024);
        assert_eq!(TaskKind::VideoChat.default_seed_tokens(), 8 * 1024);
        assert_eq!(TaskKind::CodingSmall.default_seed_tokens(), 32 * 1024);
        assert_eq!(TaskKind::CodingLarge.default_seed_tokens(), 128 * 1024);
        assert_eq!(TaskKind::GameNpcIdle.default_seed_tokens(), 4 * 1024);
        assert_eq!(TaskKind::GameNpcEngaged.default_seed_tokens(), 16 * 1024);
        assert_eq!(TaskKind::SentinelEasy.default_seed_tokens(), 16 * 1024);
        assert_eq!(TaskKind::SentinelHard.default_seed_tokens(), 64 * 1024);
        assert_eq!(TaskKind::AcademyStudent.default_seed_tokens(), 32 * 1024);
    }

    /// What this catches: regression in a TaskKind's max-cap (someone
    /// makes Chat max=4K, breaking growth-signal ability for chats
    /// that legitimately need more). Max must always >= seed.
    ///
    /// Validated 2026-04-21: set Chat max to 4*1024, test fails
    /// because max < seed for Chat; reverted, passes.
    #[test]
    fn task_kind_default_max_always_at_or_above_seed() {
        for task in [
            TaskKind::Chat, TaskKind::VoiceChat, TaskKind::VideoChat,
            TaskKind::CodingSmall, TaskKind::CodingLarge,
            TaskKind::GameNpcIdle, TaskKind::GameNpcEngaged,
            TaskKind::SentinelEasy, TaskKind::SentinelHard,
            TaskKind::AcademyStudent,
        ] {
            assert!(
                task.default_max_tokens() >= task.default_seed_tokens(),
                "{task:?}: max ({}) must be >= seed ({})",
                task.default_max_tokens(),
                task.default_seed_tokens(),
            );
        }
    }

    /// What this catches: PersonaContextBudget::for_task drops fields
    /// or pulls from the wrong task variant when constructing the
    /// budget. Min/max should come from the task's own defaults.
    ///
    /// Validated 2026-04-21: changed for_task to call .default_max
    /// twice (no min), test fails because min ends up = max not seed;
    /// reverted, passes.
    #[test]
    fn for_task_inherits_defaults_from_task_kind() {
        let b = PersonaContextBudget::for_task("Helper", TaskKind::Chat);
        assert_eq!(b.persona_label, "Helper");
        assert_eq!(b.task, TaskKind::Chat);
        assert_eq!(b.min_tokens, TaskKind::Chat.default_seed_tokens());
        assert_eq!(b.max_tokens, TaskKind::Chat.default_max_tokens());
    }

    /// What this catches: with_min_tokens silently allowing min > max,
    /// which would break invariants (paging policy asserts min<=max).
    /// Builder must auto-bump max when min is raised above it.
    ///
    /// Validated 2026-04-21: removed the auto-bump, test fails with
    /// max still = task default (smaller than new min); reverted.
    #[test]
    fn with_min_tokens_auto_bumps_max_to_preserve_invariant() {
        // Chat default: seed=8K, max=16K. Force min=64K — max should bump.
        let b = PersonaContextBudget::for_task("Big", TaskKind::Chat)
            .with_min_tokens(64 * 1024);
        assert_eq!(b.min_tokens, 64 * 1024);
        assert!(b.max_tokens >= b.min_tokens, "max must always >= min");
        assert_eq!(b.max_tokens, 64 * 1024);
    }

    /// What this catches: with_max_tokens silently allowing max < min,
    /// which is the inverse-invariant violation. Builder must clamp
    /// max to at least min.
    ///
    /// Validated 2026-04-21: changed `n.max(self.min_tokens)` to plain
    /// `n`, test fails because max ends up = 1024 (below default min);
    /// reverted.
    #[test]
    fn with_max_tokens_clamps_to_at_least_min() {
        let b = PersonaContextBudget::for_task("Clamp", TaskKind::CodingLarge)
            .with_max_tokens(1024);  // way below CodingLarge's 128K seed
        assert!(b.max_tokens >= b.min_tokens, "max must always >= min");
        assert_eq!(b.max_tokens, b.min_tokens);
    }

    /// What this catches: sum_of_seed_tokens off-by-one or wrong field
    /// (summing max instead of min). Recipe author needs accurate seed
    /// total to know what the adapter will actually allocate.
    ///
    /// Validated 2026-04-21: changed .min_tokens to .max_tokens in the
    /// sum, test fails with the much larger max-total; reverted.
    #[test]
    fn sum_of_seed_tokens_aggregates_min_not_max() {
        let recipe = RecipeBudget::new()
            .add_persona(PersonaContextBudget::for_task("A", TaskKind::Chat))   // min=8K
            .add_persona(PersonaContextBudget::for_task("B", TaskKind::Chat))   // min=8K
            .add_persona(PersonaContextBudget::for_task("C", TaskKind::CodingSmall)); // min=32K

        assert_eq!(recipe.sum_of_seed_tokens(), 8 * 1024 + 8 * 1024 + 32 * 1024);
        // Sanity: max-sum is bigger
        assert!(recipe.sum_of_max_tokens() > recipe.sum_of_seed_tokens());
    }

    /// What this catches: persona_count returning byte-len or wrong
    /// type. Adapter uses it for n_seq_max — wrong count = wrong
    /// allocation slot count.
    ///
    /// Validated 2026-04-21: returned 0 always, test fails with
    /// expected 5 vs got 0; reverted.
    #[test]
    fn persona_count_matches_added_personas() {
        let recipe = RecipeBudget::new()
            .add_persona(PersonaContextBudget::for_task("A", TaskKind::Chat))
            .add_persona(PersonaContextBudget::for_task("B", TaskKind::Chat))
            .add_persona(PersonaContextBudget::for_task("C", TaskKind::Chat))
            .add_persona(PersonaContextBudget::for_task("D", TaskKind::Chat))
            .add_persona(PersonaContextBudget::for_task("E", TaskKind::Chat));
        assert_eq!(recipe.persona_count(), 5);
    }

    /// What this catches: fits_in_model_context returning the wrong
    /// boolean (e.g., < instead of <=, or comparing max instead of
    /// seed). Adapter uses this to decide whether to load the recipe
    /// at all or reject with a clear error.
    ///
    /// Validated 2026-04-21: changed <= to <, test fails on the equal
    /// case; reverted.
    #[test]
    fn fits_in_model_context_uses_seed_sum_not_max_sum() {
        // 3 chat personas = 24K seeds, 48K maxes
        let recipe = RecipeBudget::new()
            .add_persona(PersonaContextBudget::for_task("A", TaskKind::Chat))
            .add_persona(PersonaContextBudget::for_task("B", TaskKind::Chat))
            .add_persona(PersonaContextBudget::for_task("C", TaskKind::Chat));

        // Model with exactly 24K context fits the seeds (equal allowed).
        assert!(recipe.fits_in_model_context(24 * 1024));
        // Model with 23K doesn't fit.
        assert!(!recipe.fits_in_model_context(23 * 1024));
        // Model with massive context fits trivially.
        assert!(recipe.fits_in_model_context(262144));
    }

    /// What this catches: empty recipe edge case — sum should be 0,
    /// fits_in should be true (nothing to fit), persona_count = 0.
    /// Trivial defaults must not panic or return surprising values.
    ///
    /// Validated 2026-04-21: changed sum to .last().min_tokens unwrap,
    /// test fails with panic on empty; reverted.
    #[test]
    fn empty_recipe_has_zero_sum_and_fits_anything() {
        let recipe = RecipeBudget::new();
        assert_eq!(recipe.sum_of_seed_tokens(), 0);
        assert_eq!(recipe.sum_of_max_tokens(), 0);
        assert_eq!(recipe.persona_count(), 0);
        assert!(recipe.fits_in_model_context(0));
        assert!(recipe.fits_in_model_context(262144));
    }
}
