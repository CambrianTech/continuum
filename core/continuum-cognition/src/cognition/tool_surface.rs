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
//! The activity declares the desired verbs or subtrees (code/**), else her core hands: the verbs
//! whose commands declare `Disclosure::Core`, the way they declare their access level. The rest of
//! her hands are one `commands/help` away, and her learned head adds the ones she uses.
//! The shared prompt allocation grants a token budget; this owner selects a whole level.
//! The faculty retains that selection (its verbs, not only its level) for the epoch, so every
//! later act of the epoch sends the same schemas whatever its affordances became.
//!
//! The order is canonical (by command name), fixed at [`ToolSurface::build`]. Nothing here
//! takes fewer abilities: the system prompt's menu lists every category, `commands/help`
//! gives any verb's call, and the executor dispatches every authorized name.

use std::borrow::Cow;

use continuum_base::token_estimate::estimate_tool_schema_tokens;
use continuum_base::tool_disclosure::Disclosure;

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
    /// Her core hands: the working set of an activity that declares none (the ordinary room),
    /// every verb whose command declares `Core`. The rest of her hands are one `commands/help`
    /// away, so this is what she wants, not a truncation of it (Joel, 2026-11-04: "Core hands in
    /// prompt, rest on demand").
    Hands,
    /// The activity's recipe declares affordances and they selected the surface (S1), with
    /// every verb that declares `Always` beside them. Not a truncation: the recipe is the rule.
    RoomAffordances,
    /// Her learned head for this epoch ([`crate::cognition::tool_head`]), on a server that takes
    /// `tail_tools`: the activity surface her grant bought (her core hands, or the room's recipe) with the verbs her head holds beyond it appended. Learning only ADDS: a head that replaced her hands left a
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
    /// The canonical name of each entry in `authorized`, same order.
    authorized_names: Vec<String>,
    /// Every verb she is authorized to call, native or not, in canonical order and wire dialect:
    /// what her head chooses from and what `commands/help` loads into her tail. Built from the
    /// native registry alone, a help-loaded verb that was not native was silently dropped, and
    /// help's "callable on your next act" was false (the M5, 10-30: Atlas, 35 help calls).
    authorized: Offer,
    /// Her hands: every verb whose command declares `Core` or `OnDemand`.
    hands: Offer,
    /// The verbs that declare `Always`: in every surface, a room's declared one included.
    always: Vec<String>,
    /// Her core hands: the verbs whose commands declare `Core`, in her prompt every act.
    core: Offer,
}

impl ToolSurface {
    /// Prices for the one allocation, independent of its eventual grant: a declared activity's
    /// recipe, else her core hands, each indivisible (silently replacing either's verbs changes
    /// what she can do). What her learned head adds is [`Self::claim_with_head`]'s want.
    pub fn claim_tokens(&self, declared: &[String]) -> (usize, usize) {
        let tokens = self.declared(declared).map_or(self.core.tokens, |offer| offer.tokens); // map_or: an activity that declares nothing claims her core hands
        (tokens, tokens)
    }

    /// Her core hands (or a declared activity's whole recipe) plus the verbs her learned head
    /// retains, counted as a union: what she claims in her prompt. A verb declared `OnDemand` that
    /// she has not learned is one `commands/help` away (Joel, 2026-11-04: "Core hands in prompt,
    /// rest on demand"); claimed whole, her hands took 4,567 of Atlas's 33,451 window ahead of his
    /// view, which turned over on 11 of 12 acts (the M5, 2026-11-04 08:41Z). No grant is an input:
    /// focus cannot teach the serving planner its own cap.
    pub fn claim_with_head(&self, declared: &[String], held: &[String]) -> (usize, usize) {
        let (floor, _) = self.claim_tokens(declared);
        let recipe = self.declared(declared);
        let base = recipe.as_ref().map_or(&self.core.specs, |offer| &offer.specs);
        let wanted: Vec<_> = self.authorized_names.iter().zip(&self.authorized.specs)
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
        Ok(SelectedSurface::borrowed(&self.core.specs, self.core.tokens, SurfaceReason::Hands, self.core.tokens))
    }

    /// Build every surface from the native registry, sorted by command name so the bytes
    /// on the wire never depend on registration order, then renamed into the model's dialect.
    pub fn build(
        mut offers: Vec<OfferedTool>,
        mut authorized: Vec<NativeToolSpec>,
        to_wire: impl Fn(NativeToolSpec) -> NativeToolSpec,
    ) -> Self {
        offers.sort_by(|a, b| a.spec.name.cmp(&b.spec.name));
        let raw: Vec<NativeToolSpec> = offers.iter().map(|offer| offer.spec.clone()).collect();
        // Her natives are always callable, whatever list `with_tools` was given: the authorized
        // set is her natives plus every other authorized verb, and where both name one verb the
        // native spec wins, so a verb in her hands is the same spec in the authorized set.
        authorized.retain(|other| !raw.iter().any(|native| native.name == other.name));
        authorized.extend(raw.iter().cloned());
        authorized.sort_by(|a, b| a.name.cmp(&b.name));
        let wire = |specs: Vec<NativeToolSpec>| Offer::of(specs.into_iter().map(&to_wire).collect());
        Self {
            authorized_names: authorized.iter().map(|spec| spec.name.clone()).collect(),
            authorized: wire(authorized),
            names: raw.iter().map(|spec| spec.name.clone()).collect(),
            hands: wire(hands_surface(&offers)),
            always: offers.iter().filter(|offer| offer.disclosure.in_every_surface()).map(|offer| offer.spec.name.clone()).collect(),
            core: wire(core_hands(&offers)),
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
            .authorized_names
            .iter()
            .zip(self.authorized.specs.iter())
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
            .filter_map(|spec| self.authorized_names.iter().zip(&self.authorized.specs).find(|(_, wire)| wire.name == spec.name))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// An epoch's retained selection rebuilt from [`Self::names_of`]: the same schemas, in the
    /// same order, charged the level the epoch's start chose. A name this surface no longer
    /// offers (the registry was rebuilt) has no schema to send and is not sent.
    pub fn retained(&self, names: &[String], reason: SurfaceReason, tokens: usize) -> SelectedSurface<'_> {
        let specs: Vec<NativeToolSpec> = names
            .iter()
            .filter_map(|name| self.authorized_names.iter().zip(&self.authorized.specs).find(|(canonical, _)| *canonical == name))
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
            self.authorized_names
                .iter()
                .zip(self.authorized.specs.iter())
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

    /// The verbs an activity declares (a command name, or a subtree `code/**`), with every verb
    /// that declares `Always` beside them (the discovery pair, her verdict verbs). A room's
    /// recipe is the rule of that room's prompt, never of her mind: the rest of her hands stay
    /// one `commands/help` away in it. `None` when it declares nothing, or nothing that is a
    /// native verb here.
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
                named || self.always.contains(name)
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
    /// never re-derive the cap that produced it. Every level sends what it wants; her learned head
    /// raises the demand past what it sends when it reserves the next epoch's head
    /// ([`Self::with_demand`], [`Self::reserving`]).
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

/// One natively offered verb and where its command declares it sits in her hands
/// (`CommandSpec::DISCLOSURE`): the value the tool carries, like its permissions, never a name
/// list this surface keeps (Joel, 2026-11-04: "values associated with the tool just like perms").
#[derive(Clone, Debug)]
pub struct OfferedTool {
    pub spec: NativeToolSpec,
    pub disclosure: Disclosure,
}

impl OfferedTool {
    /// A verb with no command behind it (the verdict verbs): the turn's own protocol, in every
    /// surface she is offered.
    pub fn always(spec: NativeToolSpec) -> Self {
        Self { spec, disclosure: Disclosure::Always }
    }
}

/// Her core hands: the verbs whose commands declare `Core`, in her prompt every act. The rest of
/// her hands are one `commands/help` away and join her prompt once her learned head holds them.
pub fn core_hands(offers: &[OfferedTool]) -> Vec<NativeToolSpec> {
    offers.iter().filter(|offer| offer.disclosure.in_core()).map(|offer| offer.spec.clone()).collect()
}

/// Her HANDS: every verb whose command declares itself among them (`Core` or `OnDemand`); a
/// `Withheld` verb stays callable by name and never joins them.
pub fn hands_surface(offers: &[OfferedTool]) -> Vec<NativeToolSpec> {
    offers.iter().filter(|offer| offer.disclosure.in_hands()).map(|offer| offer.spec.clone()).collect()
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

    // what this catches: a larger grant buying more of her hands into the prompt (the rest are on
    // demand, Joel 2026-11-04), a grant below her core hands sending fewer than they, or the same
    // inputs giving different bytes.
    #[test]
    fn a_grant_buys_her_core_hands_never_more_and_never_fewer() {
        let surface = built(REGISTRY.iter());
        let (floor, want) = surface.claim_tokens(&[]);
        assert!(floor > 0 && floor == want && want < surface.hands_tokens(), "her core hands are her whole claim");
        let focused = surface.select_granted(&[], surface.hands_tokens() * 4).expect("core hands fit");
        let reduced = surface.select_granted(&[], floor).expect("core hands fit");
        assert_eq!((focused.reason, focused.tokens, focused.demand_tokens()), (SurfaceReason::Hands, floor, floor));
        assert_eq!(wire(&focused), wire(&reduced), "an ample grant sends her core hands, not her whole hands");
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
        let surface = built(REGISTRY.iter());
        let (floor, want) = surface.claim_with_head(&[], &[]);
        assert_eq!(surface.claim_with_head(&[], &["code/read".into()]), (floor, want));
        let held = vec!["activity/archive".into()];
        let (_, extended) = surface.claim_with_head(&[], &held);
        assert_eq!(extended, want + tool_surface_tokens_of(&[spec("activity/archive")]));
        assert_eq!(surface.claim_with_head(&[], &[held[0].clone(), held[0].clone()]), (floor, extended));
        let selected = surface.select_granted(&[], floor).expect("core fits").with_demand(extended);
        assert_eq!((selected.tokens, selected.demand_tokens()), (floor, extended));
    }

    // what this catches: `commands/help` loading a verb that is not native and the tail silently
    // dropping it, so help's "callable on your next act" was false (the M5, 10-30: Atlas made 35
    // help calls for `chat/poll`, which never joined her tools).
    #[test]
    fn a_help_loaded_verb_that_is_not_native_rides_the_tail() {
        let native: Vec<_> = REGISTRY.iter().map(offer).collect();
        let mut authorized: Vec<_> = native.iter().map(|o| o.spec.clone()).collect();
        authorized.push(spec("chat/poll"));
        let surface = ToolSurface::build(native, authorized.clone(), |s| s);
        let floor = surface.hands().to_vec();
        let selected = surface.learned(&floor, &[], &["chat/poll".to_string()], 0);
        assert!(selected.tail.iter().any(|t| t.name == "chat/poll"), "{:?}", selected.tail);
        assert!(!surface.hands().iter().any(|t| t.name == "chat/poll"), "loading never widens her hands");
        assert_eq!(surface.vocabulary().expect("vocabulary").len(), authorized.len());
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

    // Each fixture verb carries the disclosure its command declares, as the registry does.
    const REGISTRY: &[(&str, Disclosure)] = &[
        ("web/fetch", Disclosure::OnDemand), ("code/read", Disclosure::Core), ("activity/archive", Disclosure::Withheld),
        ("work/review", Disclosure::OnDemand), ("commands/help", Disclosure::Always), ("code/git/apply", Disclosure::Withheld),
        ("focus/continue", Disclosure::OnDemand), ("code/edit", Disclosure::Core), ("codex/run", Disclosure::Withheld),
        ("work/submit", Disclosure::Core), ("commands/list", Disclosure::Always), ("code/write", Disclosure::Core),
    ];

    fn offer(&(name, disclosure): &(&str, Disclosure)) -> OfferedTool {
        OfferedTool { spec: spec(name), disclosure }
    }

    fn built<'a>(order: impl Iterator<Item = &'a (&'static str, Disclosure)>) -> ToolSurface {
        let offers: Vec<_> = order.map(offer).collect();
        let specs = offers.iter().map(|o| o.spec.clone()).collect();
        ToolSurface::build(offers, specs, |s| s)
    }

    // what this catches: a verb's place in her hands decided by its name instead of the value its
    // command declares (Joel, 2026-11-04: "values associated with the tool just like perms"): a
    // `Core` verb outside any old subtree rides her prompt, an `OnDemand` verb under `code/` waits
    // one `commands/help` away and joins her claim only once her head holds it, and a `Withheld`
    // verb never joins her hands yet stays callable by name.
    #[test]
    fn the_declared_disclosure_places_a_verb_whatever_its_name() {
        let registry = [("mail/send", Disclosure::Core), ("code/rare", Disclosure::OnDemand), ("code/peer", Disclosure::Withheld), ("code/read", Disclosure::Core)];
        let surface = built(registry.iter());
        let core: Vec<&str> = surface.core.specs.iter().map(|s| s.name.as_str()).collect();
        let hands: Vec<&str> = surface.hands().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(core, ["code/read", "mail/send"]);
        assert_eq!(hands, ["code/rare", "code/read", "mail/send"]);
        assert!(surface.vocabulary().expect("vocabulary").id("code/peer").is_some(), "withheld stays callable by name");
        let (floor, want) = surface.claim_with_head(&[], &[]);
        assert_eq!((floor, want), (surface.core.tokens, surface.core.tokens), "an unlearned on-demand verb is not claimed");
        let (_, learned) = surface.claim_with_head(&[], &["code/rare".to_string()]);
        assert_eq!(learned, tool_surface_tokens_of(&[spec("code/rare"), spec("code/read"), spec("mail/send")]));
    }

    fn wire(selected: &SelectedSurface<'_>) -> String {
        serde_json::to_string(&selected.specs).expect("specs serialize")
    }

    // what this catches: Aiko's 7-21% reuse (2026-10-24). The bytes after her system prompt
    // must depend on the activity and the served window alone, in one canonical order: the
    // same inputs give the same schemas whatever order the registry was assembled in.
    #[test]
    fn the_same_activity_and_grant_give_the_same_bytes_in_canonical_order() {
        let forward = built(REGISTRY.iter());
        let backward = built(REGISTRY.iter().rev());
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
    // the prefix (`codex/run`), a room rule stripping a verb that declares `Always` (her discovery
    // pair), or a verb riding a declared room by its name (`focus/continue` declares `OnDemand`, so
    // it stays one `commands/help` away there, as in every room).
    #[test]
    fn a_declared_subtree_selects_that_subtree_with_every_always_verb() {
        let surface = built(REGISTRY.iter());
        let selected = surface.select_granted(&["code/**".to_string()], 87_296).expect("recipe fits");
        assert_eq!(selected.reason, SurfaceReason::RoomAffordances);
        let names: Vec<&str> = selected.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["code/edit", "code/git/apply", "code/read", "code/write", "commands/help", "commands/list"]
        );
        let unmatched = surface.select_granted(&["mail/send".to_string()], 87_296).expect("hands fit");
        assert_eq!(unmatched.reason, SurfaceReason::Hands, "a rule binding nothing offers her hands, never nothing");
    }

    // what this catches: an on-demand verb riding her prompt unlearned, her discovery pair left
    // out of her core hands (the rest would be unreachable), or a zero grant sending anything.
    #[test]
    fn her_core_hands_carry_the_discovery_pair_and_leave_the_rest_on_demand() {
        let surface = built(REGISTRY.iter());
        let sent = surface.select_granted(&[], surface.hands_tokens()).expect("core fits");
        let names: Vec<&str> = sent.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"commands/list") && names.contains(&"commands/help"), "{names:?}");
        assert!(!names.contains(&"web/fetch") && !names.contains(&"focus/continue"), "{names:?}");
        assert_eq!(surface.select_granted(&[], 0).err(), Some(surface.claim_tokens(&[]).0), "zero grant cannot buy core hands");
    }

    // what this catches: the learned head REPLACING her hands (the M5, 2026-10-30: Aris's head
    // was list_files, read_file, shell-poll and help, so 57 acts made 0 writes, and a resident who
    // has not edited lately can never edit again); a held verb dropped, a loaded verb landing in
    // the head or duplicating one there, the head moving between two acts on the same inputs, or
    // the budget charged less than the next epoch's head a turnover in this act would adopt.
    #[test]
    fn a_learned_head_adds_to_her_floor_loads_the_rest_in_the_tail_and_reserves_the_next() {
        let surface = built(REGISTRY.iter());
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
