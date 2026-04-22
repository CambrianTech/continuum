//! `Recipe` trait — the cognition-domain abstraction. Each recipe
//! shapes a host's signal (chat message, video frame, code diff,
//! game tick, …) into the cognition layer's `RespondInput` contract,
//! and optionally intercepts the persona's response to route it
//! somewhere other than "post text to chat" (sentinel dispatch,
//! game action, etc.).
//!
//! # Why this exists
//!
//! `respond()` today is implicitly chat-shaped. The host hands in a
//! `RespondInput` it built itself; if the host isn't a chat surface,
//! it has to re-derive the construction logic. That:
//!
//! - Doesn't generalize to non-chat hosts (Unreal, Vision Pro, raw
//!   Rust binaries embedding the persona).
//! - Forces every domain to reinvent media handling, system-prompt
//!   composition, history shaping.
//! - Couples cognition to chat semantics it shouldn't care about.
//!
//! `Recipe` makes the domain choice explicit and pluggable. Built-in
//! recipes (Chat, Vision, Code, Game) live alongside; new domains
//! register their own without touching cognition internals.
//!
//! # Outlier-validation discipline
//!
//! Per Joel's design rule (CLAUDE.md): the trait shape is only
//! proven when implementations span dimensions:
//!
//! | Recipe | History? | Media bytes? | Output | Trigger          |
//! |--------|----------|--------------|--------|------------------|
//! | Chat   | yes      | no           | text   | user message     |
//! | Vision | yes      | image        | text   | user message+img |
//! | Code   | yes      | optional     | text + sentinel-dispatch | user/tool |
//! | Game   | NO       | scene-graph  | structured action        | tick |
//!
//! If the trait fits all four without contortion, the abstraction
//! holds. If a single impl needs a leaky escape hatch, the trait is
//! wrong shape and we redesign instead of papering.
//!
//! # What's NOT here
//!
//! - Audio / live-video recipes — shipped in the Phase C PR once the
//!   paging substrate (mmproj init mutex, Metal OOM recovery, MtmdContext
//!   pool) lands. Building them on this trait now would either ship a
//!   stub (no Metal recovery → bricks the host) or force premature
//!   substrate decisions.
//! - Generators — once the four-impl pattern stabilizes, a generator
//!   for new recipes is the right next move (per the OOP+generator
//!   philosophy in CLAUDE.md). Not yet — N=4 is the minimum for
//!   "repeatable pattern" to be meaningful.

use crate::cognition::tool_executor::types::MediaItemLite;
use crate::cognition::{PersonaSlot, RecentMessage};
use crate::model_registry::Capability;
use crate::persona::response::{PersonaResponse, RespondInput};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use ts_rs::TS;
use uuid::Uuid;

// ─── Modality ────────────────────────────────────────────────────────

/// What kind of input a recipe consumes. Open vocabulary, kebab-case
/// strings — recipes (especially host-registered ones) may invent new
/// kinds without enum churn. Standard kinds have associated constants
/// (see `ModalityKind::text()` etc.) for discoverability + typo safety.
///
/// Wire format: bare string (`"text"`, `"image"`, `"scene-graph"`),
/// not a tagged JSON object. ts-rs export is the same — TypeScript
/// sees `string`, with the standard kinds documented as a union for
/// IDE hints in the host code.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/recipe/ModalityKind.ts"
)]
#[serde(transparent)]
pub struct ModalityKind(pub String);

impl ModalityKind {
    pub fn text() -> Self {
        Self("text".to_string())
    }
    pub fn image() -> Self {
        Self("image".to_string())
    }
    pub fn audio() -> Self {
        Self("audio".to_string())
    }
    pub fn video_frame() -> Self {
        Self("video-frame".to_string())
    }
    pub fn scene_graph() -> Self {
        Self("scene-graph".to_string())
    }
    pub fn file_diff() -> Self {
        Self("file-diff".to_string())
    }
    pub fn tick() -> Self {
        Self("tick".to_string())
    }
}

// ─── Signal ──────────────────────────────────────────────────────────

/// Hint about what kind of event produced this signal. Recipes may
/// use it for routing decisions (e.g., GameRecipe ignores ChatMessage,
/// only acts on FrameUpdate or AutonomousTick).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/recipe/SignalKind.ts")]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SignalKind {
    /// Chat message authored by a user or a persona in a room.
    ChatMessage,
    /// Tool/sentinel completion event — recipe may want to react to
    /// the result.
    ToolResult { tool_name: String },
    /// Tick from the autonomous loop — no external trigger, recipe
    /// decides if there's anything to do.
    AutonomousTick,
    /// Game / AR engine frame update.
    FrameUpdate,
    /// File / diff context for code work.
    CodeContext,
    /// Open-vocab kind for host extensions Rust hasn't seen.
    Custom { name: String },
}

/// Who emitted the signal — used for system-prompt composition + for
/// recipes that filter by originator (e.g., a recipe that only
/// responds to humans, not other personas).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/recipe/SignalOriginator.ts"
)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SignalOriginator {
    User {
        #[ts(type = "string")]
        user_id: Uuid,
    },
    Persona {
        #[ts(type = "string")]
        persona_id: Uuid,
    },
    Tool {
        tool_name: String,
    },
    GameEngine,
    System,
}

/// Input to a `Recipe::build_input` call. The host's raw event,
/// pre-cognition. Open enough that ANY domain (chat, voice, video,
/// code, game, AR) emits the same shape.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/recipe/Signal.ts")]
#[serde(rename_all = "camelCase")]
pub struct Signal {
    /// Hint about the signal's nature. Recipes use it for routing.
    pub kind: SignalKind,
    /// Text payload of the signal. Empty when purely media-driven
    /// (video frame, scene-graph blob without commentary).
    pub text: String,
    /// Attached media (images, audio, video frames, scene-graph blobs).
    /// Empty for pure-text signals.
    pub media: Vec<MediaItemLite>,
    /// Who emitted the signal.
    pub originator: SignalOriginator,
    /// Wall-clock time the signal was created (ms since UNIX_EPOCH).
    #[ts(type = "number")]
    pub timestamp_ms: u64,
    /// Optional message / event ID. Used for joining captures with
    /// host-side records (chat message ID, frame number, etc.).
    #[ts(optional, type = "string")]
    pub message_id: Option<Uuid>,
}

// ─── PersonaContext ──────────────────────────────────────────────────

/// Per-persona stable state needed by every recipe — identity, model,
/// capabilities, recent history, room membership. Built once per turn
/// by the host and handed to the recipe; recipes must not mutate it.
///
/// Capabilities are `Vec<Capability>` on the wire (ts-rs friendlier
/// than HashSet); the trait converts to a HashSet at use site for
/// O(1) membership checks. Conversion happens once per
/// `build_input` call — negligible vs the inference work that
/// follows.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/recipe/PersonaContext.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct PersonaContext {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub display_name: String,
    pub specialty: String,
    /// The persona's render-time model id. Recipes use it directly
    /// (no global lookup); same single-source-of-truth principle as
    /// the IPC handler's `respond_input_from_value`.
    pub model: String,
    /// Resolved capability vocabulary for the persona's model. Caller
    /// declares; Rust consumes. Recipes may switch behavior on cap
    /// presence (VisionRecipe checks for `Capability::Vision`).
    pub capabilities: Vec<Capability>,
    /// Persona's RAG-built identity / system prompt.
    pub system_prompt: String,
    /// Recent conversation history (most-recent last). May be empty
    /// for recipes that don't use chat history (GameRecipe).
    pub recent_history: Vec<RecentMessage>,
    /// Specialty identifiers in the room (for shared analysis).
    pub known_specialties: Vec<String>,
    /// Optional room id — present for chat-room recipes, absent for
    /// game/AR/embedded hosts that have no concept of "room".
    #[ts(optional, type = "string")]
    pub room_id: Option<Uuid>,
    /// Live-voice context flag — affects prompt assembly response
    /// style. Default false for non-voice signals.
    pub is_voice: bool,
}

// ─── Recipe outcome ──────────────────────────────────────────────────

/// What the recipe wants the host to do with the persona's response
/// after `respond()` returns. Default is `Forward` — host posts /
/// uses the response as-is. Recipes may substitute or intercept.
///
/// Examples:
/// - `Forward` — ChatRecipe (default): post the Spoke text to chat.
/// - `Substitute` — GameRecipe: convert Spoke text to a structured
///   `GameAction { kind, target }` and hand THAT to the host instead.
/// - `Intercepted` — CodeRecipe spotting a sentinel-dispatch marker:
///   route to the sentinel system, drop the original Spoke (or post
///   a brief "I dispatched X" instead).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/recipe/RecipeOutcome.ts"
)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum RecipeOutcome {
    /// Pass the response through unchanged. Host posts it.
    Forward,
    /// Replace the response with this one. Host posts the substitute.
    Substitute { response: PersonaResponse },
    /// Recipe dispatched to a different system; host should drop the
    /// original response. Reason recorded for trace + observability.
    Intercepted { reason: String },
}

// ─── The trait ────────────────────────────────────────────────────────

/// A recipe shapes a host signal into cognition input and (optionally)
/// post-processes the response. Implementations are stateless value
/// objects; the registry stores them as `Arc<dyn Recipe>`.
pub trait Recipe: Send + Sync {
    /// Stable identifier — registry key, trace metadata, observability.
    /// MUST be unique within a `RecipeRegistry`.
    fn name(&self) -> &'static str;

    /// Modalities this recipe consumes. Used by autonomous loop /
    /// host routing logic to pick the right recipe for a given signal.
    /// A recipe declaring `&[ModalityKind::TEXT]` shouldn't be passed
    /// a video frame; a recipe declaring `&[ModalityKind::SCENE_GRAPH]`
    /// shouldn't be passed a chat message.
    fn modalities(&self) -> &[ModalityKind];

    /// Build the cognition layer's `RespondInput` from the signal +
    /// persona context. The recipe is the ONLY place that knows how
    /// to project a domain event into cognition's contract.
    ///
    /// Returns `Err` when the signal is unusable for this recipe
    /// (e.g., GameRecipe given a chat message → reject loudly so
    /// the host's routing bug surfaces here, not as silent garbage
    /// output downstream).
    fn build_input(
        &self,
        signal: &Signal,
        ctx: &PersonaContext,
    ) -> Result<RespondInput, String>;

    /// Post-cognition hook. Default `Forward` — recipe doesn't care
    /// about the response shape, host handles it. Override when the
    /// recipe needs to substitute (GameRecipe → structured action) or
    /// intercept (CodeRecipe → sentinel dispatch).
    ///
    /// Called AFTER `respond()` has produced the response and AFTER
    /// the recorder captured it. Side effects in this hook are part
    /// of the recipe's contract, not part of cognition.
    fn validate_output(
        &self,
        _response: &PersonaResponse,
        _ctx: &PersonaContext,
    ) -> RecipeOutcome {
        RecipeOutcome::Forward
    }
}

// ─── Registry ────────────────────────────────────────────────────────

/// Maps recipe `name()` → instance. Hosts register the recipes they
/// want available; the autonomous loop / IPC handler looks them up
/// by name when dispatching. Multiple registries can coexist (e.g.,
/// a chat-only build registers Chat + Vision, an AR build registers
/// Game + a custom AR recipe).
#[derive(Default)]
pub struct RecipeRegistry {
    recipes: HashMap<&'static str, Arc<dyn Recipe>>,
}

impl RecipeRegistry {
    pub fn new() -> Self {
        Self {
            recipes: HashMap::new(),
        }
    }

    /// Register a recipe. If the name collides with an existing entry,
    /// the new recipe replaces it AND a warning is logged. Collisions
    /// are likely a host configuration bug (two modules registering
    /// the same name); silently overwriting hides them.
    pub fn register(&mut self, recipe: Arc<dyn Recipe>) {
        let name = recipe.name();
        if self.recipes.contains_key(name) {
            crate::runtime::logger("recipe").warn(&format!(
                "RecipeRegistry: '{name}' already registered; replacing. \
                 Two registrations under the same name suggests a host \
                 configuration bug — check init_default + custom register calls."
            ));
        }
        self.recipes.insert(name, recipe);
    }

    /// Look up a recipe by name. Returns `None` when the name is
    /// unknown — caller decides whether to fall back (e.g., to a
    /// default recipe) or error.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Recipe>> {
        self.recipes.get(name).cloned()
    }

    /// All registered recipe names. Useful for observability and for
    /// IPC commands that expose "what recipes does this host support?".
    pub fn list(&self) -> Vec<&'static str> {
        self.recipes.keys().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }
}

// ─── PersonaContext convenience ──────────────────────────────────────

impl PersonaContext {
    /// Build the `PersonaSlot` the cognition layer expects from this
    /// context. Convenience so individual recipes don't repeat it in
    /// their `build_input`.
    pub fn slot(&self) -> PersonaSlot {
        PersonaSlot {
            persona_id: self.persona_id,
            specialty: self.specialty.clone(),
            display_name: self.display_name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Trait-shape tests. Pure; no I/O, no async. Validates the
    //! registry primitives + verifies the trait can be implemented
    //! by a stub. Per-recipe tests live alongside each implementation.
    use super::*;

    /// Stub recipe for trait-shape testing. Doesn't do real work.
    struct StubRecipe {
        name: &'static str,
        modalities: Vec<ModalityKind>,
    }

    impl Recipe for StubRecipe {
        fn name(&self) -> &'static str {
            self.name
        }
        fn modalities(&self) -> &[ModalityKind] {
            &self.modalities
        }
        fn build_input(
            &self,
            _signal: &Signal,
            ctx: &PersonaContext,
        ) -> Result<RespondInput, String> {
            Ok(RespondInput {
                persona: ctx.slot(),
                room_id: ctx.room_id.unwrap_or(Uuid::nil()),
                message_id: Uuid::nil(),
                message_text: String::new(),
                recent_history: ctx.recent_history.clone(),
                known_specialties: ctx.known_specialties.clone(),
                system_prompt: ctx.system_prompt.clone(),
                model: ctx.model.clone(),
                is_voice: ctx.is_voice,
                message_media: Vec::new(),
                capabilities: ctx.capabilities.iter().copied().collect(),
            })
        }
    }

    fn stub(name: &'static str, modalities: Vec<ModalityKind>) -> Arc<dyn Recipe> {
        Arc::new(StubRecipe { name, modalities })
    }

    fn empty_ctx() -> PersonaContext {
        PersonaContext {
            persona_id: Uuid::nil(),
            display_name: String::new(),
            specialty: String::new(),
            model: String::new(),
            capabilities: Vec::new(),
            system_prompt: String::new(),
            recent_history: vec![],
            known_specialties: vec![],
            room_id: None,
            is_voice: false,
        }
    }

    /// What this catches: empty registry returns None on lookup. The
    /// "no recipes registered yet" baseline.
    #[test]
    fn empty_registry_lookup_returns_none() {
        let reg = RecipeRegistry::new();
        assert!(reg.get("anything").is_none());
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    /// What this catches: register + get round-trip. The trivial
    /// "the registry actually stores things" test.
    #[test]
    fn register_and_get_round_trips() {
        let mut reg = RecipeRegistry::new();
        reg.register(stub("test", vec![ModalityKind::text()]));
        let got = reg.get("test").expect("registered recipe should be findable");
        assert_eq!(got.name(), "test");
        assert_eq!(got.modalities(), &[ModalityKind::text()]);
    }

    /// What this catches: multiple distinct recipes coexist in one
    /// registry. The "I can mix Chat + Vision + Code" baseline.
    #[test]
    fn multiple_recipes_coexist() {
        let mut reg = RecipeRegistry::new();
        reg.register(stub("a", vec![ModalityKind::text()]));
        reg.register(stub("b", vec![ModalityKind::image()]));
        reg.register(stub("c", vec![ModalityKind::scene_graph()]));
        assert_eq!(reg.len(), 3);
        let mut names = reg.list();
        names.sort();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    /// What this catches: re-registering under the same name
    /// REPLACES (not silently ignores). Catches the host-config bug
    /// where two init paths register the same recipe — the second
    /// one wins, and the warning makes it visible.
    #[test]
    fn duplicate_registration_replaces_with_warning() {
        let mut reg = RecipeRegistry::new();
        reg.register(stub("dup", vec![ModalityKind::text()]));
        reg.register(stub("dup", vec![ModalityKind::image()]));
        assert_eq!(reg.len(), 1);
        let got = reg.get("dup").unwrap();
        // Second registration's modality (image) wins.
        assert_eq!(got.modalities(), &[ModalityKind::image()]);
    }

    /// What this catches: the trait can be implemented by a value-
    /// object stub WITHOUT needing async runtime, registry context,
    /// or any global state. Trait stays pure-fn-shaped — easy to
    /// test, easy to mock, easy to embed.
    #[test]
    fn stub_recipe_builds_input_from_context_alone() {
        let r = stub("stub", vec![ModalityKind::text()]);
        let mut ctx = empty_ctx();
        ctx.display_name = "Test".to_string();
        ctx.specialty = "general".to_string();
        ctx.model = "test-model".to_string();
        ctx.system_prompt = "you are helpful".to_string();
        ctx.known_specialties = vec!["general".to_string()];
        let signal = Signal {
            kind: SignalKind::ChatMessage,
            text: "hi".to_string(),
            media: vec![],
            originator: SignalOriginator::System,
            timestamp_ms: 0,
            message_id: None,
        };
        let input = r.build_input(&signal, &ctx).expect("stub should build");
        assert_eq!(input.persona.display_name, "Test");
        assert_eq!(input.model, "test-model");
        assert_eq!(input.system_prompt, "you are helpful");
    }

    /// What this catches: default `validate_output` returns Forward
    /// — recipes that don't override it pass responses through
    /// unchanged. ChatRecipe relies on this; if the default ever
    /// changes to Intercepted or Substitute, the entire chat path
    /// silently breaks.
    #[test]
    fn default_validate_output_is_forward() {
        let r = stub("stub", vec![ModalityKind::text()]);
        let response = PersonaResponse::Spoke {
            persona_id: Uuid::nil(),
            text: "ok".to_string(),
            model_used: "test".to_string(),
            inference_ms: 1,
            total_ms: 2,
            think_blocks_emitted: 0,
        };
        let outcome = r.validate_output(&response, &empty_ctx());
        assert!(matches!(outcome, RecipeOutcome::Forward));
    }

    /// What this catches: Signal serializes through serde cleanly
    /// (catches a missing `Serialize` derive on a nested type, a
    /// renamed field, or a wire shape that drifted). The replay
    /// harness depends on Signal round-tripping through JSON.
    #[test]
    fn signal_round_trips_through_serde() {
        let signal = Signal {
            kind: SignalKind::ChatMessage,
            text: "hello".to_string(),
            media: vec![],
            originator: SignalOriginator::User { user_id: Uuid::nil() },
            timestamp_ms: 1234,
            message_id: Some(Uuid::nil()),
        };
        let json = serde_json::to_string(&signal).expect("serializes");
        let back: Signal = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back.text, "hello");
        assert_eq!(back.timestamp_ms, 1234);
        assert!(matches!(back.kind, SignalKind::ChatMessage));
    }

    /// What this catches: stub has access to `ctx.slot()` convenience
    /// — should produce a PersonaSlot whose fields mirror the
    /// PersonaContext. If `slot()` ever drops a field or adds drift,
    /// every recipe's `build_input` silently produces wrong cognition
    /// input.
    #[test]
    fn persona_context_slot_mirrors_fields() {
        let mut ctx = empty_ctx();
        ctx.persona_id = Uuid::nil();
        ctx.specialty = "vision".to_string();
        ctx.display_name = "Vision AI".to_string();
        let slot = ctx.slot();
        assert_eq!(slot.persona_id, ctx.persona_id);
        assert_eq!(slot.specialty, ctx.specialty);
        assert_eq!(slot.display_name, ctx.display_name);
    }
}
