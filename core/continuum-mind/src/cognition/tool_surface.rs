//! A mind's native tool surface: the working set her activity declares, the same bytes on
//! every act until an epoch boundary (plan goal 10, card ecd5dff3). Its own owner, out of
//! the deliberation faculty.
//!
//! # Why it must not move between acts
//!
//! The engine reuses a mind's prompt by prefix, and the chat template renders the tool
//! schemas right after the system prompt. A surface that differs from the previous act
//! re-prefills everything after the system prompt: on the M5's 27B hybrid, a checkpoint
//! restore plus tens of thousands of tokens. Aiko, 2026-10-24: `delib.prompt.reuse_split`
//! diverged in `tools` (`edit_file` one act, `conclude_activity` the next) and kept 7-21%
//! of the prompt, because the surface was chosen per act from three things that move:
//! the turn kind (a work or initiative turn took her hands, a message turn the whole
//! registry), the held card (a review card added two verbs), and a tool share priced off
//! the prefill-affordable window, which follows the measured rate and the turn's audience
//! (6,986 to 11,998 tokens on one 87,296 window). She alternated between 23 and 51
//! schemas.
//!
//! # What decides it now
//!
//! Two inputs, both of which change only where her prompt turns over anyway:
//! - the ACTIVITY: the verbs or subtrees (`code/**`) its recipe declares, else her hands;
//! - the SERVED window (a startup fact; a relaunch drops the KV regardless): whether her
//!   extended hands fit its tool share, else her core hands.
//!
//! The order is canonical (by command name), fixed at [`ToolSurface::build`]. Nothing here
//! takes fewer abilities: the system prompt's menu lists every category, `commands/help`
//! gives any verb's call, and the executor dispatches every authorized name.

use std::borrow::Cow;

use continuum_base::token_estimate::estimate_tool_schema_tokens;

use crate::ai::types::NativeToolSpec;

/// WHY a turn's tool surface is what it is.
///
/// Carried beside the cost because the cost alone cannot be interpreted: see
/// [`SelectedSurface::demand_tokens`].
///
/// `Default` is `NoTools` so a default `ContextComposition` is coherent: zero
/// `tool_tokens` because no tools were offered, not a reason that disagrees with its cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurfaceReason {
    /// She has no native specs at all: nothing to offer, nothing to price.
    #[default]
    NoTools,
    /// Her hands: the working set of an activity that declares none (the ordinary room).
    Hands,
    /// The served window's tool share cannot hold her extended hands (the web, push and
    /// PR, rooms and chat, her focus verbs), so her core hands go out. The one arm that is a
    /// truncation, so the one whose demand exceeds what it sends.
    CoreHandsForWindow,
    /// The activity's recipe declares affordances and they selected the surface (S1), with
    /// the discovery pair and her focus verbs beside them. Not a truncation: the recipe is
    /// the rule.
    RoomAffordances,
}

/// One working set as it goes on the wire, priced once when it is built.
#[derive(Debug, Clone, Default)]
struct Offer {
    specs: Vec<NativeToolSpec>,
    tokens: usize,
}

impl Offer {
    fn of(specs: Vec<NativeToolSpec>) -> Self {
        let tokens = tool_surface_tokens_of(&specs);
        Self { specs, tokens }
    }
}

/// Her native tool surfaces, built once per tool assignment, in canonical order.
#[derive(Debug, Clone, Default)]
pub struct ToolSurface {
    /// The canonical command name (`code/edit`) of each entry in `registry`, same order:
    /// the specs are kept in the model's wire dialect (`edit_file`), so a selection keyed on
    /// command names (an activity's affordances) keys here.
    names: Vec<String>,
    /// Every native verb. Priced for the window sensor; the source a declared set selects from.
    registry: Offer,
    /// Her hands: what an activity that declares nothing offers.
    hands: Offer,
    /// Her hands without the extended verbs: what a window too small for them offers.
    core: Offer,
}

impl ToolSurface {
    /// Build every surface from the native registry, sorted by command name so the bytes
    /// on the wire never depend on registration order, then renamed into the model's dialect.
    pub fn build(mut raw: Vec<NativeToolSpec>, to_wire: impl Fn(NativeToolSpec) -> NativeToolSpec) -> Self {
        raw.sort_by(|a, b| a.name.cmp(&b.name));
        let wire = |specs: Vec<NativeToolSpec>| Offer::of(specs.into_iter().map(&to_wire).collect());
        Self {
            names: raw.iter().map(|spec| spec.name.clone()).collect(),
            hands: wire(hands_surface(&raw)),
            core: wire(core_hands(&raw)),
            registry: wire(raw),
        }
    }

    /// Every native verb, in canonical order and wire dialect.
    pub fn registry(&self) -> &[NativeToolSpec] {
        &self.registry.specs
    }

    /// What the whole native registry costs on the wire.
    pub fn registry_tokens(&self) -> usize {
        self.registry.tokens
    }

    /// The canonical command name of each [`Self::registry`] entry, same order.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Her hands, as an activity that declares nothing offers them on a window that holds them.
    pub fn hands(&self) -> &[NativeToolSpec] {
        &self.hands.specs
    }

    /// What her hands cost on the wire.
    pub fn hands_tokens(&self) -> usize {
        self.hands.tokens
    }

    /// The surface for an activity that declares `declared` on a lane serving
    /// `served_window` tokens. Nothing about the turn is an input, so two acts in the same
    /// activity on the same lane offer the same bytes.
    pub fn select(&self, declared: &[String], served_window: u32) -> SelectedSurface<'_> {
        if self.registry.specs.is_empty() {
            return SelectedSurface::borrowed(&[], 0, SurfaceReason::NoTools, 0);
        }
        if let Some(offer) = self.declared(declared) {
            let tokens = offer.tokens;
            return SelectedSurface {
                specs: Some(Cow::Owned(offer.specs)),
                tokens,
                reason: SurfaceReason::RoomAffordances,
                demand: tokens,
            };
        }
        // The SERVED window, never the prefill-affordable one: the schemas sit in the cached
        // prefix, so they are prefilled once per epoch, not once per act, and pricing them
        // against this turn's measured rate is what made the surface move.
        let share = super::context_budget::ContextBudget::from_window_unbounded(served_window).tool_surface_tokens();
        if self.hands.tokens <= share {
            SelectedSurface::borrowed(&self.hands.specs, self.hands.tokens, SurfaceReason::Hands, self.hands.tokens)
        } else {
            SelectedSurface::borrowed(&self.core.specs, self.core.tokens, SurfaceReason::CoreHandsForWindow, self.hands.tokens)
        }
    }

    /// The verbs an activity declares (a command name, or a subtree `code/**`), with the
    /// discovery pair and her own mind's verbs beside them: a room's recipe is the rule of
    /// that room, never of her mind (Kimi, 2026-10-05: "help works; nothing to invoke
    /// against"). `None` when it declares nothing, or nothing that is a native verb here.
    fn declared(&self, declared: &[String]) -> Option<Offer> {
        if declared.is_empty() {
            return None;
        }
        let mut granted = false;
        let specs: Vec<NativeToolSpec> = self
            .names
            .iter()
            .zip(self.registry.specs.iter())
            .filter(|(name, _)| {
                let named = declared.iter().any(|pattern| declares(pattern, name));
                granted |= named;
                named || name.starts_with("commands/") || name.starts_with("focus/")
            })
            .map(|(_, spec)| spec.clone())
            .collect();
        granted.then(|| Offer::of(specs))
    }
}

/// Whether `pattern` (a command name, or a subtree `code/**`) declares `name`.
fn declares(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix("/**") {
        Some(subtree) => name.strip_prefix(subtree).is_some_and(|rest| rest.starts_with('/')),
        None => pattern == name,
    }
}

/// The tool surface a turn sends, its cost, and why it is that surface.
///
/// One decision, consulted by both the budget math and the request builder, so the two can
/// never disagree about what is on the wire (card dec1a7ff).
pub struct SelectedSurface<'a> {
    /// Exactly what goes into the request. `None` only when there are no tools. Borrowed
    /// from the built surfaces, owned only when an activity's declaration selected it.
    pub specs: Option<Cow<'a, [NativeToolSpec]>>,
    /// Cost of what is SENT: the number the message budget must use.
    pub tokens: usize,
    pub reason: SurfaceReason,
    demand: usize,
}

impl<'a> SelectedSurface<'a> {
    fn borrowed(specs: &'a [NativeToolSpec], tokens: usize, reason: SurfaceReason, demand: usize) -> Self {
        Self { specs: (reason != SurfaceReason::NoTools).then_some(Cow::Borrowed(specs)), tokens, reason, demand }
    }

    /// What an UNTRUNCATED demand measurement prices for this turn.
    ///
    /// `delib.turn.demand` is the demand, not the send, so that a p95 of what-was-sent can
    /// never re-derive the cap that produced it. Only [`SurfaceReason::CoreHandsForWindow`]
    /// truncated anything: she wanted her hands and the window refused the extended ones, so
    /// her whole hands are the demand. Every other arm sent what was wanted.
    pub fn demand_tokens(&self) -> usize {
        self.demand
    }
}

/// The verbs a capable citizen's hands add over the core working set: the web, her rooms
/// and voice in them, her focus verbs, the push and PR verbs that land a change without an
/// operator (#4532), and the reviewer verbs. Offered whenever the served window's tool
/// share holds them; on an 8,192 window her newest line outranks them (the 8192 survival
/// check in core's `tool_surface_is_a_category_index_plus_discovery_pair`).
pub fn is_extended_hand(name: &str) -> bool {
    // `code/shell-poll` is NOT extended: it is the second half of `code/shell`, whose own
    // description says to poll a running execution with it (Idris, 2026-09-29).
    crate::ai::tool_dialect::reviewer_hand(name)
        || name.starts_with("web/")
        || name.starts_with("focus/")
        || name.starts_with("room/")
        || name.starts_with("chat/")
        || matches!(name, "code/git/add" | "code/git/push" | "code/github/pr-create" | "code/github/pr-comment")
}

/// Her hands without the extended verbs: what a window too small for the full set carries.
pub fn core_hands(raw: &[NativeToolSpec]) -> Vec<NativeToolSpec> {
    hands_surface(raw).into_iter().filter(|spec| !is_extended_hand(&spec.name)).collect()
}

/// Her HANDS: the subtrees she works with (code, work, git, cargo, tool, the discovery pair
/// `commands/`, the web, her rooms and chat, her own mind's `focus/` verbs), selected on the
/// COMMAND names before the wire dialect renames them.
///
/// The reviewer verbs (`work/review`, `work/submission`) are hands: they are part of
/// `work/**`, and offering them only while she held a review card made the card state an
/// input to the surface (residents are never gated on cards, turn kind or room; Joel,
/// 2026-10-23). A holder who misreads one gets the executor's own-card guidance
/// (`tool_dialect::withheld_from_hands`). `code/git/apply` stays one `commands/help` away:
/// it takes a PEER's diff, and offered among her hands four citizens called it with an
/// empty patch inside twenty minutes (2026-09-04).
pub fn hands_surface(raw: &[NativeToolSpec]) -> Vec<NativeToolSpec> {
    const SUBTREES: &[&str] =
        &["focus/", "room/", "chat/", "code/", "work/", "git/", "cargo/", "tool/", "commands/", "web/"];
    raw.iter()
        .filter(|spec| {
            let name = spec.name.as_str();
            let withheld = crate::ai::tool_dialect::withheld_from_hands(name).is_some()
                && !crate::ai::tool_dialect::reviewer_hand(name);
            !withheld && SUBTREES.iter().any(|subtree| name.starts_with(subtree))
        })
        .cloned()
        .collect()
}

/// Serialize and count a tool surface: paid once per [`ToolSurface::build`], never per turn.
pub fn tool_surface_tokens_of(specs: &[NativeToolSpec]) -> usize {
    specs
        .iter()
        .map(|spec| {
            let serialized = serde_json::to_string(spec).unwrap_or_default();
            estimate_tool_schema_tokens(&serialized)
        })
        .sum()
}

/// The prompt this lane affords (served window bounded by prefill t/s x TTFT), read beside
/// what the tool surface and framing cost. Emits `delib.tool_surface.over_affordable` when
/// they alone exceed it. Observation only: nothing here changes what is sent (card 3026b83e,
/// step 1). Returns the affordable token count for the turn's receipt.
pub(crate) fn over_affordable_receipt(
    persona: &str,
    surface_reason: SurfaceReason,
    tool_tokens: usize,
    framing_tokens: usize,
    context_window: u32,
) -> Option<usize> {
    let affordable_tokens =
        super::context_budget::ContextBudget::from_window(context_window).affordable_tokens();
    if let Some(affordable) = affordable_tokens.filter(|a| framing_tokens + tool_tokens > *a) {
        continuum_base::probe!(
            class = "delib.tool_surface.over_affordable",
            persona = %persona,
            surface_reason = ?surface_reason,
            tool_tokens,
            framing_tokens,
            affordable_tokens = affordable,
            context_window,
            "the tool surface and framing alone cost more than the prompt this lane can prefill inside its TTFT budget"
        );
    }
    affordable_tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> NativeToolSpec {
        NativeToolSpec {
            name: name.to_string(),
            description: format!("{name} does one thing"),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({ "path": { "type": "string" } }),
                required: None,
                definitions: None,
            },
        }
    }

    const REGISTRY: &[&str] = &[
        "web/fetch", "code/read", "activity/archive", "work/review", "commands/help", "code/git/apply",
        "focus/continue", "code/edit", "codex/run", "work/submit", "commands/list",
    ];

    fn built(order: impl Iterator<Item = &'static str>) -> ToolSurface {
        ToolSurface::build(order.map(spec).collect(), |s| s)
    }

    fn wire(selected: &SelectedSurface<'_>) -> String {
        serde_json::to_string(&selected.specs).expect("specs serialize")
    }

    // what this catches: Aiko's 7-21% reuse (2026-10-24). The bytes after her system prompt
    // must depend on the activity and the served window alone, in one canonical order: the
    // same inputs give the same schemas whatever order the registry was assembled in.
    #[test]
    fn the_same_activity_and_window_give_the_same_bytes_in_canonical_order() {
        let forward = built(REGISTRY.iter().copied());
        let backward = built(REGISTRY.iter().rev().copied());
        for declared in [Vec::new(), vec!["code/**".to_string()]] {
            for window in [0, 8_192, 87_296] {
                let (a, b) = (forward.select(&declared, window), backward.select(&declared, window));
                assert_eq!((a.reason, wire(&a)), (b.reason, wire(&b)), "declared {declared:?}, window {window}");
                assert_eq!(wire(&a), wire(&forward.select(&declared, window)), "a second act is identical");
            }
        }
        let names: Vec<&str> = forward.hands().iter().map(|s| s.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "canonical order");
        assert!(names.contains(&"work/review"), "the reviewer verbs are part of work/**: {names:?}");
        assert!(!names.contains(&"code/git/apply") && !names.contains(&"activity/archive"), "{names:?}");
    }

    // what this catches: a subtree declaration (`code/**`) granting a sibling that only shares
    // the prefix (`codex/run`), or a room rule stripping her discovery pair or focus verbs.
    #[test]
    fn a_declared_subtree_selects_that_subtree_with_her_discovery_and_focus_verbs() {
        let surface = built(REGISTRY.iter().copied());
        let selected = surface.select(&["code/**".to_string()], 87_296);
        assert_eq!(selected.reason, SurfaceReason::RoomAffordances);
        let names: Vec<&str> = selected.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["code/edit", "code/git/apply", "code/read", "commands/help", "commands/list", "focus/continue"]
        );
        let unmatched = surface.select(&["mail/send".to_string()], 87_296);
        assert_eq!(unmatched.reason, SurfaceReason::Hands, "a rule binding nothing offers her hands, never nothing");
    }

    // what this catches: a window too small for her extended hands sending nothing, or
    // reporting the core it sent as all she wanted (the demand probe would hide the pressure).
    #[test]
    fn a_window_too_small_for_her_extended_hands_sends_her_core_and_demands_the_whole() {
        let surface = built(REGISTRY.iter().copied());
        let roomy = surface.select(&[], 87_296);
        assert_eq!((roomy.reason, roomy.demand_tokens()), (SurfaceReason::Hands, roomy.tokens));
        let tight = surface.select(&[], 512);
        assert_eq!(tight.reason, SurfaceReason::CoreHandsForWindow);
        assert_eq!(tight.demand_tokens(), surface.hands_tokens());
        assert!(tight.tokens < tight.demand_tokens());
        let names: Vec<&str> = tight.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"commands/list") && !names.contains(&"web/fetch"), "{names:?}");
        assert_eq!(surface.select(&[], 0).reason, SurfaceReason::Hands, "an unknown window is never an invented one");
    }
}
