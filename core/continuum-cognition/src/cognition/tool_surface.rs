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
//! whose commands declare `Disclosure::Core`, the way they declare their access level. The cards
//! she holds add the verbs whose preconditions hold on them (`work_preconditions`,
//! STATE-DRIVEN-SURFACE.md): a
//! reviewer holding a review card has `work/review` in hand whatever the room declares. The rest
//! of her hands are one `commands/help` away, and her learned head adds the ones she uses.
//! This owner decides WHICH verbs are her hands; the act's one allocation decides how much of each
//! she is shown (whole, summary or pointer, `levels.rs`), so a small window collapses verbs instead
//! of refusing the turn. The faculty retains the selection (its verbs and their levels) for the
//! epoch of her head, so every later act sends the same schemas whatever its affordances became.
//!
//! The order is canonical (by command name), fixed at [`ToolSurface::build`]. Nothing here
//! takes fewer abilities: the system prompt's menu lists every category, `commands/help`
//! gives any verb's call, and the executor dispatches every authorized name.

use std::borrow::Cow;

mod levels;
pub use levels::{expand_handle, Floor, ToolSet, ToolsLeveled, VerbLevels};

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

/// Her working set before her learned head ([`ToolSurface::activity`]): her core hands as built,
/// or a set selected for this activity and the cards she holds.
enum Activity<'a> {
    Core(&'a Offer),
    Owned(Offer, SurfaceReason),
}

impl Activity<'_> {
    fn offer(&self) -> &Offer {
        match self {
            Activity::Core(offer) => offer,
            Activity::Owned(offer, _) => offer,
        }
    }

    fn tokens(&self) -> usize {
        self.offer().tokens
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
    /// The canonical name of each entry in `hands`, same order.
    hands_names: Vec<String>,
    /// The verbs that declare `Always`: in every surface, a room's declared one included.
    always: Vec<String>,
    /// Her core hands: the verbs whose commands declare `Core`, in her prompt every act.
    core: Offer,
}

impl ToolSurface {
    /// Her working set for this act: a declared activity's recipe, else her core hands, with the
    /// moves her held cards give her (`held`, command names and subtrees from
    /// `work_preconditions::in_hand`). No grant decides it: which verbs are her hands is this
    /// owner's; how much of each verb she is shown is the act's one allocation's ([`Self::tool_set`]).
    pub fn select(&self, declared: &[String], held: &[String]) -> SelectedSurface<'_> {
        if self.registry.specs.is_empty() {
            return SelectedSurface::borrowed(&[], 0, SurfaceReason::NoTools, 0);
        }
        let activity = self.activity(declared, held);
        let tokens = activity.tokens();
        match activity {
            Activity::Core(core) => SelectedSurface::borrowed(&core.specs, tokens, SurfaceReason::Hands, tokens),
            Activity::Owned(offer, reason) => SelectedSurface { specs: Some(Cow::Owned(offer.specs)), tokens, reason, demand: tokens, tail: Vec::new() },
        }
    }

    /// `selected` as a budget consumer: each verb at its levels (`levels.rs`), ranked by which keeps
    /// detail longest, from declared and measured data only: a verb that declares `Always` (her
    /// discovery pair and verdict verbs, the turn's protocol), then the moves her held cards need
    /// now (`moves`), then the verbs her learned head holds (`head`: her measured use), then the
    /// rest. A verb nothing expands (`Always`) floors at its summary.
    pub fn tool_set(&self, selected: &SelectedSurface<'_>, moves: &[String], head: &[String]) -> ToolSet {
        let finder = self
            .authorized_names
            .iter()
            .zip(&self.authorized.specs)
            .find(|(name, _)| name.as_str() == crate::cognition::tool_head::FINDER)
            .map_or(crate::cognition::tool_head::FINDER, |(_, spec)| spec.name.as_str()); // map_or: a surface without the finder names it canonically
        let names = self.names_of(selected);
        let specs = selected.specs.as_deref().unwrap_or(&[]); // unwrap_or: no surface sends no verbs
        ToolSet::new(
            names
                .into_iter()
                .zip(specs)
                .map(|(name, spec)| {
                    let always = self.always.contains(&name);
                    let rank = if always {
                        3
                    } else if moves.iter().any(|pattern| declares(pattern, &name)) {
                        2
                    } else {
                        u8::from(head.contains(&name))
                    };
                    let floor = if always { Floor::Summary } else { Floor::Pointer };
                    let levels = VerbLevels::of(spec, &name, finder, floor);
                    (name, levels, rank)
                })
                .collect(),
        )
    }

    /// Her working set before her learned head: the room's declared verbs (with every `Always`
    /// verb), else her core hands, then the verbs her held cards' moves name that the base lacks,
    /// in one canonical order. A held move is in hand whatever its command declares about her
    /// prompt (`OnDemand`), because her card's state needs it now (Benchy on the M5, 2026-11-04:
    /// a held review card, `work/review` one `commands/help` away, 4 help calls and 5 refused
    /// submits); a `Withheld` verb never joins. Stable while her cards' moves are.
    fn activity(&self, declared: &[String], held: &[String]) -> Activity<'_> {
        let recipe = self.declared(declared);
        let (base, reason) = match &recipe {
            Some(offer) => (&offer.specs, SurfaceReason::RoomAffordances),
            None => (&self.core.specs, SurfaceReason::Hands),
        };
        let extras: Vec<&String> = self
            .hands_names
            .iter()
            .zip(&self.hands.specs)
            .filter(|(name, spec)| held.iter().any(|pattern| declares(pattern, name)) && !base.iter().any(|b| b.name == spec.name))
            .map(|(name, _)| name)
            .collect();
        if extras.is_empty() {
            return match recipe {
                Some(offer) => Activity::Owned(offer, reason),
                None => Activity::Core(&self.core),
            };
        }
        let specs: Vec<NativeToolSpec> = self
            .names
            .iter()
            .zip(&self.registry.specs)
            .filter(|(name, spec)| extras.contains(name) || base.iter().any(|b| b.name == spec.name))
            .map(|(_, spec)| spec.clone())
            .collect();
        Activity::Owned(Offer::of(specs), reason)
    }

    /// Build every surface from the native registry in ONE canonical order, so the bytes on the
    /// wire never depend on registration order, then renamed into the model's dialect. The order is
    /// by how widely a verb is held, then by name: the verbs in every surface, then her core hands,
    /// then the on-demand verbs, then the withheld. Every selection lists its verbs in this order,
    /// so a smaller set is a PREFIX of a larger one and every mind's tool list shares its head with
    /// every other mind's (the M5, 2026-11-06: Benchy's 30 verbs and the others' 39 diverged at verb
    /// 18, where name order put the on-demand `focus/*` between the core hands, so the tools head
    /// four minds send was shared only through 12,969 of 33,340 chars).
    pub fn build(
        mut offers: Vec<OfferedTool>,
        mut authorized: Vec<NativeToolSpec>,
        to_wire: impl Fn(NativeToolSpec) -> NativeToolSpec,
    ) -> Self {
        let held_by = |disclosure: Disclosure| match disclosure {
            Disclosure::Always => 0,
            Disclosure::Core => 1,
            Disclosure::OnDemand => 2,
            Disclosure::Withheld => 3,
        };
        offers.sort_by(|a, b| held_by(a.disclosure).cmp(&held_by(b.disclosure)).then_with(|| a.spec.name.cmp(&b.spec.name)));
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
            hands_names: offers.iter().filter(|offer| offer.disclosure.in_hands()).map(|offer| offer.spec.name.clone()).collect(),
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

    /// Her learned head: `floor` (her activity's surface, from [`Self::select`]) as it is, then the
    /// `held` verbs it lacks appended in canonical order, so the floor's bytes lead whatever her
    /// head learns and learning only adds; the `loaded` verbs the head lacks ride the tail.
    pub fn learned(&self, floor: &[NativeToolSpec], held: &[String], loaded: &[String]) -> SelectedSurface<'_> {
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
            tokens,
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
    /// never re-derive the cap that produced it: every verb whole, whatever level the allocation
    /// showed it at ([`Self::with_demand`], [`Self::at_levels`]).
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

    /// The same verbs at the levels the act's allocation chose (`levels.rs`): what is sent and what
    /// it costs. Demand stays every verb whole.
    pub fn at_levels(mut self, set: &ToolSet, levels: &[usize]) -> Self {
        if self.reason == SurfaceReason::NoTools {
            return self;
        }
        let (specs, tokens) = set.render(levels);
        self.specs = Some(Cow::Owned(specs));
        self.tokens = tokens;
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

    // what this catches: an ample window buying more of her hands into the prompt (the rest are on
    // demand, Joel 2026-11-04), or the same inputs giving different bytes. Which verbs are her
    // hands is this owner's; no grant enters the choice.
    #[test]
    fn her_selection_is_her_core_hands_whatever_the_window() {
        let surface = built(REGISTRY.iter());
        let core = surface.select(&[], &[]);
        assert_eq!((core.reason, core.tokens, core.demand_tokens()), (SurfaceReason::Hands, surface.core.tokens, surface.core.tokens));
        assert!(core.tokens < surface.hands_tokens(), "her core hands, not her whole hands");
        assert_eq!(wire(&core), wire(&surface.select(&[], &[])), "same epoch inputs keep prefix bytes");
        let declared = vec!["code/**".to_string()];
        let recipe = surface.select(&declared, &[]);
        assert_eq!((recipe.reason, recipe.demand_tokens()), (SurfaceReason::RoomAffordances, recipe.tokens));
    }

    // what this catches (Aiko on the M5, 2026-11-07: tools 11,377 + framing 2,795 refused her turn
    // on a 30,566 window): her hands with no level between whole and absent, so a small window
    // refused the turn. Every verb is a consumer whose floor is its pointer, except a verb nothing
    // expands (`Always`), whose floor is its summary; what keeps detail longest is declared or
    // measured (her `Always` verbs, her held card's moves, her learned head), never a name list.
    #[test]
    fn her_hands_collapse_to_pointers_and_what_she_uses_keeps_detail_longest() {
        let surface = built(REGISTRY.iter());
        let core = surface.select(&[], &[]);
        let set = surface.tool_set(&core, &[], &["code/write".to_string()]);
        let leveled = set.leveled(|tokens| tokens as u32);
        assert!(leveled.leveled.hard_limit() < leveled.leveled.want(), "a floor below every verb whole");
        let floor = set.levels_of(&leveled, &leveled.leveled.at_limit().rungs);
        for (name, level) in set.names.iter().zip(&floor) {
            let expected = if surface.always.contains(name) { 1 } else { 2 };
            assert_eq!(*level, expected, "{name}: at its floor");
        }
        let (specs, _) = set.render(&floor);
        let edit = specs.iter().find(|s| s.name == "code/edit").expect("code/edit is in her core hands"); // JUSTIFIED: test fixture
        assert_eq!(edit.description, expand_handle("commands/help", "code/edit"), "the pointer names how to open it");
        // The first reduction above the floor that raises a verb past its pointer raises the verb her
        // learned head holds, before the rest of her core hands.
        let first_rise = leveled
            .leveled
            .options()
            .into_iter()
            .rev()
            .map(|option| set.levels_of(&leveled, &option.rungs))
            .find(|levels| set.names.iter().zip(levels).any(|(name, &level)| !surface.always.contains(name) && level < 2))
            .expect("some reduction raises a verb"); // JUSTIFIED: test fixture
        let risen: Vec<&String> = set.names.iter().zip(&first_rise).filter(|(name, &level)| !surface.always.contains(name) && level < 2).map(|(name, _)| name).collect();
        assert_eq!(risen, ["code/write"], "her used verb rises before the rest");
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
        let selected = surface.learned(&floor, &[], &["chat/poll".to_string()]);
        assert!(selected.tail.iter().any(|t| t.name == "chat/poll"), "{:?}", selected.tail);
        assert!(!surface.hands().iter().any(|t| t.name == "chat/poll"), "loading never widens her hands");
        assert_eq!(surface.vocabulary().expect("vocabulary").len(), authorized.len());
    }

    fn spec(name: &str) -> NativeToolSpec {
        NativeToolSpec {
            name: name.to_string(),
            description: format!("{name} does one thing with the argument it is given: it reads it, checks it, and answers with what it found. It refuses an argument it cannot read, saying why."),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({ "path": { "type": "string", "description": "The path it reads, relative to the worktree root." } }),
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
        assert_eq!(hands, ["code/read", "mail/send", "code/rare"], "her core hands lead, the on-demand verbs follow");
        assert!(surface.vocabulary().expect("vocabulary").id("code/peer").is_some(), "withheld stays callable by name");
        assert_eq!(surface.select(&[], &[]).tokens, surface.core.tokens, "an unlearned on-demand verb is not selected");
        let floor = surface.select(&[], &[]);
        let learned = surface.learned(floor.specs.as_deref().unwrap_or(&[]), &["code/rare".to_string()], &[]); // JUSTIFIED: test fixture
        assert_eq!(learned.tokens, tool_surface_tokens_of(&[spec("code/read"), spec("mail/send"), spec("code/rare")]));
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
            let (a, b) = (forward.select(&declared, &[]), backward.select(&declared, &[]));
            assert_eq!((a.reason, wire(&a)), (b.reason, wire(&b)), "declared {declared:?}");
            assert_eq!(wire(&a), wire(&forward.select(&declared, &[])), "a second act is identical");
        }
        let names: Vec<&str> = forward.hands().iter().map(|s| s.name.as_str()).collect();
        assert!(in_canonical_order(&forward, &names), "canonical order: {names:?}");
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
        let selected = surface.select(&["code/**".to_string()], &[]);
        assert_eq!(selected.reason, SurfaceReason::RoomAffordances);
        let names: Vec<&str> = selected.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["commands/help", "commands/list", "code/edit", "code/read", "code/write", "code/git/apply"]
        );
        let unmatched = surface.select(&["mail/send".to_string()], &[]);
        assert_eq!(unmatched.reason, SurfaceReason::Hands, "a rule binding nothing offers her hands, never nothing");
    }

    // what this catches: an on-demand verb riding her prompt unlearned, or her discovery pair left
    // out of her core hands (the rest would be unreachable).
    #[test]
    fn her_core_hands_carry_the_discovery_pair_and_leave_the_rest_on_demand() {
        let surface = built(REGISTRY.iter());
        let sent = surface.select(&[], &[]);
        let names: Vec<&str> = sent.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"commands/list") && names.contains(&"commands/help"), "{names:?}");
        assert!(!names.contains(&"web/fetch") && !names.contains(&"focus/continue"), "{names:?}");
    }

    // what this catches: the learned head REPLACING her hands (the M5, 2026-10-30: Aris's head
    // was list_files, read_file, shell-poll and help, so 57 acts made 0 writes, and a resident who
    // has not edited lately can never edit again); a held verb dropped, a loaded verb landing in
    // the head or duplicating one there, or the head moving between two acts on the same inputs.
    #[test]
    fn a_learned_head_adds_to_her_floor_and_loads_the_rest_in_the_tail() {
        let surface = built(REGISTRY.iter());
        let floor = surface.select(&[], &[]);
        let floor_specs = floor.specs.as_deref().expect("core hands");
        let core: Vec<&str> = floor_specs.iter().map(|s| s.name.as_str()).collect();
        assert!(core.contains(&"code/edit") && core.contains(&"code/write"), "precondition: core hands hold code/edit and code/write: {core:?}");
        // Her head holds only what she used lately: neither code/edit nor code/write among it.
        let held = vec!["commands/help".to_string(), "activity/archive".to_string()];
        let loaded = vec!["web/fetch".to_string(), "code/read".to_string()];
        let head = surface.learned(floor_specs, &held, &loaded);
        let names: Vec<&str> = head.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect();
        let mut expected = core.clone();
        expected.push("activity/archive");
        assert_eq!(names, expected, "her floor whole and in order, then the held verbs it lacked");
        assert_eq!(head.tail.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["web/fetch"], "a loaded verb already in the head is not sent twice");
        assert_eq!(head.reason, SurfaceReason::LearnedHead);
        assert_eq!(wire(&head), wire(&surface.learned(floor_specs, &held, &loaded)), "the same inputs give the same bytes");
        let vocab = surface.vocabulary().expect("vocabulary");
        assert_eq!(vocab.len(), REGISTRY.len());
        let edit = vocab.id("code/edit").expect("canonical names key the vocabulary");
        assert_eq!(vocab.tokens(edit) as usize, tool_surface_tokens_of(&[spec("code/edit")]));
    }

    /// What she has in hand on her own card in `state` (a review card when `reviews` is set).
    fn moves(reviews: Option<airc_work::WorkCardId>, state: airc_work::CardState) -> Vec<String> {
        use crate::cognition::work_preconditions::{fixture_card, in_hand, FIXTURE_NOW_MS};
        let her = airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(1));
        in_hand(&fixture_card(state, Some(her), reviews, "card"), her, FIXTURE_NOW_MS).into_iter().map(str::to_owned).collect()
    }

    /// Whether `names` stand in the surface's one canonical order (how widely held, then by name).
    fn in_canonical_order(surface: &ToolSurface, names: &[&str]) -> bool {
        let at = |name: &&str| surface.names().iter().position(|known| known == name);
        names.windows(2).all(|pair| at(&pair[0]) < at(&pair[1]))
    }

    // what this catches: two minds whose tool sets differ (Benchy's 30 verbs, the others' 39 on the
    // M5, 2026-11-06) diverging inside the shared part of their tool list. Her core hands are a
    // PREFIX of her whole hands, so the smaller list is the head of the larger one.
    #[test]
    fn a_smaller_tool_set_is_the_head_of_a_larger_one() {
        let surface = built(REGISTRY.iter());
        let core = surface.select(&[], &[]);
        let core = sent_names(&core);
        let hands: Vec<&str> = surface.hands().iter().map(|s| s.name.as_str()).collect();
        assert!(core.len() < hands.len(), "precondition: her hands hold more than her core");
        assert_eq!(hands[..core.len()], core[..], "her core is the head of her hands");
    }

    fn sent_names<'a>(selected: &'a SelectedSurface<'_>) -> Vec<&'a str> {
        selected.specs.as_deref().into_iter().flatten().map(|s| s.name.as_str()).collect()
    }

    // what this catches: Benchy on the M5, 2026-11-04, holding review card 32d53734 in Claimed
    // with `work/review` one `commands/help` away (it declares `OnDemand` here): 4 help calls and
    // 5 `work/submit` calls refused with "nothing to submit". The moves her held card gives her
    // are in hand in every room, a room that declares its own recipe included, in canonical
    // order and priced in her claim; a code card puts `work/submit` and her code hands in hand
    // and never a `Withheld` verb.
    #[test]
    fn a_held_review_card_puts_the_verdict_verb_in_hand_whatever_it_declares() {
        use airc_work::{CardState, WorkCardId};
        let surface = built(REGISTRY.iter());
        let review = moves(Some(WorkCardId::from_u128(7)), CardState::Claimed);
        for declared in [Vec::new(), vec!["code/**".to_string()]] {
            let plain = surface.select(&declared, &[]);
            assert!(!sent_names(&plain).contains(&"work/review"), "precondition: on demand without a held card");
            let held = surface.select(&declared, &review);
            let names = sent_names(&held);
            assert!(names.contains(&"work/review"), "declared {declared:?}: {names:?}");
            assert!(in_canonical_order(&surface, &names), "canonical order: {names:?}");
            assert_eq!(held.demand_tokens(), held.tokens, "her demand prices what she is sent whole");
            assert_eq!(held.reason, plain.reason, "the room still decides why; her card adds to it");
        }
        let code = surface.select(&[], &moves(None, CardState::InProgress));
        let names = sent_names(&code);
        assert!(names.contains(&"work/submit") && names.contains(&"code/edit"), "{names:?}");
        assert!(!names.contains(&"code/git/apply") && !names.contains(&"work/review"), "{names:?}");
    }

    // what this catches: a surface that churns while her card's state is stable (every act would
    // re-prefill the prompt behind the tool schemas: Aiko's 7-21% reuse, 2026-10-24), or one that
    // does not change when what her cards' preconditions put in hand changes (the M5, 2026-11-04).
    // Same verbs in hand, same bytes; a transition that keeps them (Claimed to InProgress) changes
    // nothing; holding a different kind of card changes the set.
    #[test]
    fn the_surface_changes_only_when_her_cards_moves_do() {
        use airc_work::{CardState, WorkCardId};
        let surface = built(REGISTRY.iter());
        let reviewer = Some(WorkCardId::from_u128(7));
        let at = |state| surface.select(&[], &moves(reviewer, state));
        let claimed = wire(&at(CardState::Claimed));
        assert_eq!(claimed, wire(&at(CardState::Claimed)), "an unchanged state is byte-identical across acts");
        assert_eq!(claimed, wire(&at(CardState::InProgress)), "a transition that keeps her moves keeps her surface");
        let code = surface.select(&[], &moves(None, CardState::InProgress));
        assert_ne!(claimed, wire(&code), "a card whose preconditions put other verbs in hand changes her surface");
    }
}
