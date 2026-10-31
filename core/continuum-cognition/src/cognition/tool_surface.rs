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
//! The activity declares the desired verbs or subtrees (code/**), else her hands.
//! The shared prompt allocation grants a token budget; this owner selects a whole level.
//! The faculty retains that selection (its verbs, not only its level) for the epoch, so every
//! later act of the epoch sends the same schemas whatever its affordances became.
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
    /// The shared allocation grant cannot hold her extended hands (the web, push and
    /// PR, rooms and chat, her focus verbs), so her core hands go out. The one arm that is a
    /// truncation, so the one whose demand exceeds what it sends.
    CoreHandsForWindow,
    /// The activity's recipe declares affordances and they selected the surface (S1), with
    /// the discovery pair and her focus verbs beside them. Not a truncation: the recipe is
    /// the rule.
    RoomAffordances,
    /// Her learned head for this epoch ([`crate::cognition::tool_head`]), on a server that takes
    /// `tail_tools`: the activity surface her grant bought (her core hands on a tight window, her
    /// hands, or the room's recipe) with the verbs her head holds beyond it appended. Learning only ADDS: a head that replaced her hands left a
    /// resident who had not edited lately with no `code/edit` to edit with (the M5, 2026-10-30:
    /// Aris 4 tools, 57 acts, 0 writes). Every other verb is one `commands/help` away and
    /// callable from the tail.
    LearnedHead,
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
    /// Prices for the one allocation, independent of its eventual grant. Explicit
    /// activity recipes are indivisible: silently replacing their verbs changes
    /// what the activity can do. Ordinary hands have the core as their floor.
    pub fn claim_tokens(&self, declared: &[String]) -> (usize, usize) {
        match self.declared(declared) {
            Some(offer) => (offer.tokens, offer.tokens),
            None => (self.core.tokens, self.hands.tokens),
        }
    }

    /// Full activity surface plus the retained learned verbs, counted as a union.
    /// No grant is an input: focus cannot teach the serving planner its own cap.
    pub fn claim_with_head(&self, declared: &[String], held: &[String]) -> (usize, usize) {
        let (floor, _) = self.claim_tokens(declared);
        let recipe = self.declared(declared);
        let base = recipe.as_ref().map_or(&self.hands.specs, |offer| &offer.specs);
        let wanted: Vec<_> = self.names.iter().zip(&self.registry.specs)
            .filter(|(name, spec)| held.contains(name) || base.iter().any(|b| b.name == spec.name))
            .map(|(_, spec)| spec.clone()).collect();
        (floor, tool_surface_tokens_of(&wanted))
    }

    /// Select a whole schema level from an allocation grant, never a window share.
    /// `Err(floor)` reports capacity instead of emitting schemas beyond the grant.
    /// The epoch owner retains this selection until the next epoch boundary.
    pub fn select_granted(&self, declared: &[String], grant_tokens: usize) -> Result<SelectedSurface<'_>, usize> {
        if self.registry.specs.is_empty() {
            return Ok(SelectedSurface::borrowed(&[], 0, SurfaceReason::NoTools, 0));
        }
        if let Some(offer) = self.declared(declared) {
            let tokens = offer.tokens;
            if tokens > grant_tokens { return Err(tokens); }
            return Ok(SelectedSurface {
                specs: Some(Cow::Owned(offer.specs)), tokens,
                reason: SurfaceReason::RoomAffordances, demand: tokens, tail: Vec::new(),
            });
        }
        if self.core.tokens > grant_tokens { return Err(self.core.tokens); }
        if self.hands.tokens <= grant_tokens {
            Ok(SelectedSurface::borrowed(&self.hands.specs, self.hands.tokens, SurfaceReason::Hands, self.hands.tokens))
        } else {
            Ok(SelectedSurface::borrowed(&self.core.specs, self.core.tokens, SurfaceReason::CoreHandsForWindow, self.hands.tokens))
        }
    }

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

    /// Every native verb as the learned head chooses from it: its canonical name, its
    /// description, and what its schema costs on the wire.
    pub fn vocabulary(&self) -> Result<tool_working_set::Vocabulary, String> {
        let schemas = self
            .names
            .iter()
            .zip(self.registry.specs.iter())
            .map(|(name, spec)| {
                let tokens = u32::try_from(tool_surface_tokens_of(std::slice::from_ref(spec)))
                    .map_err(|e| format!("{name}: schema size: {e}"))?;
                Ok(tool_working_set::vocab::ToolSchema { name: name.clone(), description: spec.description.clone(), tokens })
            })
            .collect::<Result<Vec<_>, String>>()?;
        tool_working_set::Vocabulary::new(schemas)
    }

    /// The canonical names of what `selected` sends in its head, in the order it sends them:
    /// what an epoch retains so its later acts send the same schemas ([`Self::retained`]).
    pub fn names_of(&self, selected: &SelectedSurface<'_>) -> Vec<String> {
        let Some(sent) = selected.specs.as_deref() else {
            return Vec::new();
        };
        sent.iter()
            .filter_map(|spec| self.names.iter().zip(&self.registry.specs).find(|(_, wire)| wire.name == spec.name))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// An epoch's retained selection rebuilt from [`Self::names_of`]: the same schemas, in the
    /// same order, charged the level the epoch's start chose. A name this surface no longer
    /// offers (the registry was rebuilt) has no schema to send and is not sent.
    pub fn retained(&self, names: &[String], reason: SurfaceReason, tokens: usize) -> SelectedSurface<'_> {
        let specs: Vec<NativeToolSpec> = names
            .iter()
            .filter_map(|name| self.names.iter().zip(&self.registry.specs).find(|(canonical, _)| *canonical == name))
            .map(|(_, spec)| spec.clone())
            .collect();
        SelectedSurface {
            specs: (reason != SurfaceReason::NoTools).then_some(Cow::Owned(specs)),
            tokens,
            reason,
            demand: tokens,
            tail: Vec::new(),
        }
    }

    /// Her learned head: `floor` (the activity surface her grant bought, from
    /// [`Self::select_granted`]) as it is, then the `held` verbs it lacks appended in canonical
    /// order, so the floor's bytes lead whatever her head learns and learning only adds; the
    /// `loaded` verbs the head lacks ride the tail. The budget is charged `reserve_tokens` when
    /// that exceeds the head (the next epoch's head, so a turnover in this act fits what it adopts).
    pub fn learned(&self, floor: &[NativeToolSpec], held: &[String], loaded: &[String], reserve_tokens: usize) -> SelectedSurface<'_> {
        let mut specs = floor.to_vec();
        let pick = |wanted: &[String], have: &[NativeToolSpec]| -> Vec<NativeToolSpec> {
            self.names
                .iter()
                .zip(self.registry.specs.iter())
                .filter(|(name, spec)| wanted.contains(name) && !have.iter().any(|kept| kept.name == spec.name))
                .map(|(_, spec)| spec.clone())
                .collect()
        };
        let extras = pick(held, &specs);
        specs.extend(extras);
        let tail = pick(loaded, &specs);
        let tokens = tool_surface_tokens_of(&specs);
        SelectedSurface {
            specs: Some(Cow::Owned(specs)),
            tokens: tokens.max(reserve_tokens),
            reason: SurfaceReason::LearnedHead,
            demand: tokens,
            tail,
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
#[derive(Debug, Clone)]
pub struct SelectedSurface<'a> {
    /// Exactly what goes into the request. `None` only when there are no tools. Borrowed
    /// from the built surfaces, owned only when an activity's declaration selected it.
    pub specs: Option<Cow<'a, [NativeToolSpec]>>,
    /// Cost of what is SENT: the number the message budget must use.
    pub tokens: usize,
    pub reason: SurfaceReason,
    demand: usize,
    /// Verbs loaded into the tail: callable through the request's `tail_tools`, never in the head.
    pub tail: Vec<NativeToolSpec>,
}

impl<'a> SelectedSurface<'a> {
    fn borrowed(specs: &'a [NativeToolSpec], tokens: usize, reason: SurfaceReason, demand: usize) -> Self {
        Self { specs: (reason != SurfaceReason::NoTools).then_some(Cow::Borrowed(specs)), tokens, reason, demand, tail: Vec::new() }
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

    /// The same selection, owning its schemas: what a prompt view carries to its request, so
    /// the request sends exactly what the allocation bought (one decision, card dec1a7ff).
    pub fn into_owned(self) -> SelectedSurface<'static> {
        SelectedSurface {
            specs: self.specs.map(|specs| Cow::Owned(specs.into_owned())),
            tokens: self.tokens,
            reason: self.reason,
            demand: self.demand,
            tail: self.tail,
        }
    }

    /// No surface: a turn refused for capacity before it could be offered one.
    pub fn none() -> Self {
        Self::borrowed(&[], 0, SurfaceReason::NoTools, 0)
    }

    pub(crate) fn with_demand(mut self, demand: usize) -> Self {
        self.demand = demand;
        self
    }

    /// Charge the budget at least `reserve_tokens` for this surface: the next epoch's head,
    /// which a turnover in this act would adopt, is larger than this one.
    pub fn reserving(mut self, reserve_tokens: usize) -> Self {
        self.tokens = self.tokens.max(reserve_tokens);
        self
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_grants_change_the_tool_level_without_changing_full_demand() {
        let surface = built(REGISTRY.iter().copied());
        let (floor, want) = surface.claim_tokens(&[]);
        assert!(floor > 0 && floor < want);
        let focused = surface.select_granted(&[], want).expect("whole hands fit");
        let reduced = surface.select_granted(&[], floor).expect("core hands fit");
        assert_eq!(focused.demand_tokens(), want);
        assert_eq!(reduced.demand_tokens(), want);
        assert_eq!((focused.tokens, reduced.tokens), (want, floor));
        assert_eq!(surface.select_granted(&[], floor - 1).err(), Some(floor));
        let again = surface.select_granted(&[], want).expect("same grant fits");
        assert_eq!(serde_json::to_string(&focused.specs).expect("schemas"),
            serde_json::to_string(&again.specs).expect("schemas"), "same epoch inputs keep prefix bytes");
        let declared = vec!["code/**".to_string()];
        let (floor, want) = surface.claim_tokens(&declared);
        assert_eq!(floor, want, "an explicit recipe is indivisible");
        assert_eq!(surface.select_granted(&declared, floor - 1).err(), Some(floor));
        assert_eq!(surface.select_granted(&declared, want).expect("recipe fits").demand_tokens(), want);
    }

    #[test]
    fn learned_want_is_a_union_not_the_granted_head_size() {
        let surface = built(REGISTRY.iter().copied());
        let (floor, want) = surface.claim_with_head(&[], &[]);
        assert_eq!(surface.claim_with_head(&[], &["code/read".into()]), (floor, want));
        let held = vec!["activity/archive".into()];
        let (_, extended) = surface.claim_with_head(&[], &held);
        assert_eq!(extended, want + tool_surface_tokens_of(&[spec("activity/archive")]));
        assert_eq!(surface.claim_with_head(&[], &[held[0].clone(), held[0].clone()]), (floor, extended));
        let selected = surface.select_granted(&[], floor).expect("core fits").with_demand(extended);
        assert_eq!((selected.tokens, selected.demand_tokens()), (floor, extended));
    }

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
        "focus/continue", "code/edit", "codex/run", "work/submit", "commands/list", "code/write",
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
    fn the_same_activity_and_grant_give_the_same_bytes_in_canonical_order() {
        let forward = built(REGISTRY.iter().copied());
        let backward = built(REGISTRY.iter().rev().copied());
        for declared in [Vec::new(), vec!["code/**".to_string()]] {
            for window in [forward.claim_tokens(&declared).0, forward.claim_tokens(&declared).1] {
                let (a, b) = (forward.select_granted(&declared, window).expect("grant fits"), backward.select_granted(&declared, window).expect("grant fits"));
                assert_eq!((a.reason, wire(&a)), (b.reason, wire(&b)), "declared {declared:?}, window {window}");
                assert_eq!(wire(&a), wire(&forward.select_granted(&declared, window).expect("grant fits")), "a second act is identical");
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
        let selected = surface.select_granted(&["code/**".to_string()], 87_296).expect("recipe fits");
        assert_eq!(selected.reason, SurfaceReason::RoomAffordances);
        let names: Vec<&str> = selected.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["code/edit", "code/git/apply", "code/read", "code/write", "commands/help", "commands/list", "focus/continue"]
        );
        let unmatched = surface.select_granted(&["mail/send".to_string()], 87_296).expect("hands fit");
        assert_eq!(unmatched.reason, SurfaceReason::Hands, "a rule binding nothing offers her hands, never nothing");
    }

    // what this catches: a window too small for her extended hands sending nothing, or
    // reporting the core it sent as all she wanted (the demand probe would hide the pressure).
    #[test]
    fn a_small_grant_sends_core_hands_and_demands_the_whole() {
        let surface = built(REGISTRY.iter().copied());
        let roomy = surface.select_granted(&[], surface.hands_tokens()).expect("hands fit");
        assert_eq!((roomy.reason, roomy.demand_tokens()), (SurfaceReason::Hands, roomy.tokens));
        let tight = surface.select_granted(&[], surface.claim_tokens(&[]).0).expect("core fits");
        assert_eq!(tight.reason, SurfaceReason::CoreHandsForWindow);
        assert_eq!(tight.demand_tokens(), surface.hands_tokens());
        assert!(tight.tokens < tight.demand_tokens());
        let names: Vec<&str> = tight.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"commands/list") && !names.contains(&"web/fetch"), "{names:?}");
        assert_eq!(surface.select_granted(&[], 0).err(), Some(surface.claim_tokens(&[]).0), "zero grant cannot buy core hands");
    }

    // what this catches: the learned head REPLACING her hands (the M5, 2026-10-30: Aris's head
    // was list_files, read_file, shell-poll and help, so 57 acts made 0 writes, and a resident who
    // has not edited lately can never edit again); a held verb dropped, a loaded verb landing in
    // the head or duplicating one there, the head moving between two acts on the same inputs, or
    // the budget charged less than the next epoch's head a turnover in this act would adopt.
    #[test]
    fn a_learned_head_adds_to_her_floor_loads_the_rest_in_the_tail_and_reserves_the_next() {
        let surface = built(REGISTRY.iter().copied());
        let floor = surface.select_granted(&[], surface.claim_tokens(&[]).0).expect("core fits");
        let floor_specs = floor.specs.as_deref().expect("core hands");
        let core: Vec<&str> = floor_specs.iter().map(|s| s.name.as_str()).collect();
        assert!(core.contains(&"code/edit") && core.contains(&"code/write"), "precondition: core hands hold code/edit and code/write: {core:?}");
        // Her head holds only what she used lately: neither code/edit nor code/write among it.
        let held = vec!["commands/help".to_string(), "activity/archive".to_string()];
        let loaded = vec!["web/fetch".to_string(), "code/read".to_string()];
        let head = surface.learned(floor_specs, &held, &loaded, 0);
        let names: Vec<&str> = head.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        let mut expected = core.clone();
        expected.push("activity/archive");
        assert_eq!(names, expected, "her floor whole and in order, then the held verbs it lacked");
        assert_eq!(head.tail.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["web/fetch"], "a loaded verb already in the head is not sent twice");
        assert_eq!(head.reason, SurfaceReason::LearnedHead);
        assert_eq!(wire(&head), wire(&surface.learned(floor_specs, &held, &loaded, 0)), "the same inputs give the same bytes");
        let reserved = surface.learned(floor_specs, &held, &loaded, 100_000);
        assert_eq!((reserved.tokens, reserved.demand_tokens()), (100_000, head.tokens));
        let vocab = surface.vocabulary().expect("vocabulary");
        assert_eq!(vocab.len(), REGISTRY.len());
        let edit = vocab.id("code/edit").expect("canonical names key the vocabulary");
        assert_eq!(vocab.tokens(edit) as usize, tool_surface_tokens_of(&[spec("code/edit")]));
    }
}
