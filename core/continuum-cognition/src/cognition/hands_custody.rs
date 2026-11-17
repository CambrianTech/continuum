//! The hands an epoch of her head froze, and what she has been told about their contracts since
//! (card 9cf06ade).
//!
//! The served model's template renders her tool schemas at the very head of the prompt, so one
//! changed schema re-prefills everything behind it: the #6102 deploy on the M5 (2026-11-17) changed
//! `work_note`'s description, the shared prefix broke at token 5,739, and Aiko's 119,972-token
//! prompt was recomputed in 435 s while Aris waited behind it. So an epoch of her head keeps the
//! exact schemas it sent ([`HeldHands::sent`]) and sends them on every act until her head's next
//! epoch, a restart included, and a verb whose contract moved since is told to her as a notice
//! before her next act ([`HeldHands::changes`]): she is never held to a contract she was not shown.
//!
//! What she was TOLD is tracked apart from what was FROZEN ([`HeldHands::told`]): a contract that
//! moves A -> B and back to A after she was told about B is a change again (restored to A), or
//! her newest notice would still say B.

use std::collections::BTreeSet;
use std::sync::Arc;

use continuum_base::tool_contract::ContractId;

use super::tool_surface::SurfaceReason;
use crate::ai::types::NativeToolSpec;

/// One epoch of her head: which verbs, at which levels, the exact schemas sent, and each verb's
/// contract as frozen and as last told.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct HeldHands {
    /// Which epoch of her head this is: a new set, new levels, or a new surface start a new one.
    pub(crate) generation: u64,
    /// Her verbs as canonical names, in the order sent.
    pub(crate) names: Vec<String>,
    /// Each verb's level, in sent order (`tool_surface::VerbLevels`: 0 whole).
    pub(crate) levels: Vec<usize>,
    pub(crate) reason: SurfaceReason,
    /// The held-card moves it was selected for (`HeldWork::move_commands`).
    pub(crate) moves: Vec<String>,
    /// Moves that changed since and whose set waits for her head's next epoch: said once.
    pub(crate) waiting: Vec<String>,
    /// The menu categories her framing shows expanded (`framing.rs`).
    pub(crate) menu: BTreeSet<String>,
    /// Each held verb whole as frozen, in sent order: what a level is priced from this epoch.
    pub(crate) whole: Arc<[NativeToolSpec]>,
    /// Exactly what her head sends at `levels`: rendered once at the freeze, then shared by every
    /// act's request of this epoch.
    pub(crate) sent: Arc<[NativeToolSpec]>,
    /// What `sent` costs.
    pub(crate) sent_tokens: usize,
    /// What her held verbs cost whole: her head's demand, priced once at the freeze.
    pub(crate) demand_tokens: usize,
    /// Each held verb's contract, aligned with `names`: as frozen, and as last told.
    pub(crate) contracts: Vec<VerbContract>,
}

/// One held verb's contract: as her head froze it, and what a served prompt last told her since.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct VerbContract {
    /// Its contract when frozen; `None` when the surface no longer offered it then.
    pub(crate) frozen: Option<ContractId>,
    #[serde(default)]
    pub(crate) told: Told,
}

/// What a SUBMITTED prompt last told her about a held verb's contract since her head froze it:
/// marked from the render receipt, never when a notice is made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Told {
    /// Nothing since the freeze: she knows the frozen contract.
    #[default]
    Nothing,
    /// This contract.
    Offered(ContractId),
    /// That the verb was removed.
    Removed,
}

impl VerbContract {
    /// A verb frozen under `frozen`, nothing told yet.
    pub(crate) fn frozen(frozen: Option<ContractId>) -> Self {
        Self { frozen, told: Told::Nothing }
    }

    /// The contract she knows: the newest a submitted prompt told her, else the frozen one.
    pub(crate) fn known(&self) -> Option<ContractId> {
        match self.told {
            Told::Nothing => self.frozen,
            Told::Offered(id) => Some(id),
            Told::Removed => None,
        }
    }
}

/// A held verb's contract as the live surface has it now.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LiveContract {
    /// It is offered under this contract, with this whole schema.
    Offered { id: ContractId, spec: NativeToolSpec },
    /// It is no longer offered to her: a call to it is refused, never run from the frozen schema.
    Removed,
}

impl LiveContract {
    pub(crate) fn id(&self) -> Option<ContractId> {
        match self {
            LiveContract::Offered { id, .. } => Some(*id),
            LiveContract::Removed => None,
        }
    }
}

/// A held verb whose live contract is not the one she knows: what her next act must be told.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContractChange {
    pub(crate) name: String,
    /// The contract she knows: the newest she was told, else the frozen one.
    pub(crate) known: Option<ContractId>,
    pub(crate) now: LiveContract,
}

impl ContractChange {
    /// The notice's key: one per transition, so a notice is appended once and a later transition
    /// appends a new one.
    pub(crate) fn key(&self) -> String {
        let now = self.now.id().map_or_else(|| "removed".to_string(), |id| id.hex()); // map_or_else: a removed verb has no contract
        format!("tool-contract/{}/{now}", self.name)
    }

    /// What she reads: which verb, that her head still shows the earlier schema until her next
    /// epoch, and the contract a call is now checked against.
    pub(crate) fn text(&self) -> String {
        let known = self.known.map_or_else(|| "removed".to_string(), |id| id.hex()); // map_or_else: she knew it removed
        match &self.now {
            LiveContract::Removed => format!(
                "My tool `{}` was removed (contract {known} -> removed). My tool list above still shows it until my head's next epoch; a call to it is refused.",
                self.name
            ),
            LiveContract::Offered { id, spec } => {
                let schema = &spec.input_schema;
                let required = schema.required.as_deref().map_or_else(String::new, |required| required.join(", ")); // map_or_else: a schema that requires nothing names nothing
                format!(
                    "My tool `{}` changed (contract {known} -> {}). My tool list above still shows the earlier schema until my head's next epoch; a call is checked against this one:\n{}\nparameters: {}\nrequired: {required}",
                    self.name,
                    id.hex(),
                    spec.description,
                    schema.properties,
                )
            }
        }
    }
}

impl HeldHands {
    /// The contract she knows for `canonical`, when her head holds that verb.
    pub(crate) fn held(&self, canonical: &str) -> Option<VerbContract> {
        self.names.iter().position(|name| name == canonical).and_then(|at| self.contracts.get(at).copied())
    }

    /// The menu categories her head holds expanded.
    pub(crate) fn menu(&self) -> &BTreeSet<String> {
        &self.menu
    }

    /// Every held verb whose live contract (`live`, by canonical name) is not the one she knows,
    /// in sent order. A verb restored to the contract she was frozen with after she was told about
    /// another one is a change again.
    pub(crate) fn changes(&self, live: impl Fn(&str) -> LiveContract) -> Vec<ContractChange> {
        self.names
            .iter()
            .zip(&self.contracts)
            .filter_map(|(name, contract)| {
                let now = live(name);
                let known = contract.known();
                (now.id() != known).then(|| ContractChange { name: name.clone(), known, now })
            })
            .collect()
    }

    /// A submitted prompt showed her `change` whole: she now knows its contract.
    pub(crate) fn tell(&mut self, change: &ContractChange) {
        let told = change.now.id().map_or(Told::Removed, Told::Offered); // map_or: told no contract is told it removed
        if let Some(contract) = self.names.iter().position(|name| *name == change.name).and_then(|at| self.contracts.get_mut(at)) {
            contract.told = told;
        }
    }
}

/// Her head's custody as a checkpoint holds it, beside her view.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum HandsCustody {
    /// No hands were recorded (a checkpoint written before this, or a mind with no tools yet).
    #[default]
    Unrecorded,
    Held(HeldHands),
    /// Recorded but undecodable: kept whole (`raw`, its JSON as read) and written back as it was,
    /// never read as absent; her act is refused until it is repaired (Codex's review of 9cf06ade).
    Corrupt { raw: serde_json::Value, error: String },
}

impl HandsCustody {
    pub(crate) fn is_unrecorded(&self) -> bool {
        matches!(self, HandsCustody::Unrecorded)
    }
}

/// A head her explicit adoption replaced (`persona/hands/adopt`): kept as it was, never erased, with
/// who asked, why, and the epoch of her head that replaced it (Codex's review of 9cf06ade).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SupersededHands {
    /// The custody it replaced, as it was read.
    pub(crate) prior: SupersededPrior,
    pub(crate) reason: String,
    pub(crate) by: String,
    /// The generation of her head that replaced it.
    pub(crate) generation: u64,
}

/// The custody an adoption replaced, kept as it was read.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) enum SupersededPrior {
    Unrecorded,
    Held(HeldHands),
    /// A record that did not decode, its JSON whole.
    Corrupt(serde_json::Value),
}

/// An adoption asked for and not yet frozen: what it replaces, and who asked why.
#[derive(Clone, Debug)]
pub(crate) struct PendingAdoption {
    pub(crate) prior: HandsCustody,
    pub(crate) reason: String,
    pub(crate) by: String,
}

impl PendingAdoption {
    /// The record it leaves once her new head is frozen at `generation`.
    pub(crate) fn superseded(self, generation: u64) -> SupersededHands {
        let prior = match self.prior {
            HandsCustody::Unrecorded => SupersededPrior::Unrecorded,
            HandsCustody::Held(hands) => SupersededPrior::Held(hands),
            HandsCustody::Corrupt { raw, .. } => SupersededPrior::Corrupt(raw),
        };
        SupersededHands { prior, reason: self.reason, by: self.by, generation }
    }
}

/// Why her acts are refused until her head is adopted (`persona/hands/adopt`), her view kept: never
/// a silent substitute of the live tools for the ones her engine holds (Codex's review of 9cf06ade).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum HandsRefusal {
    /// Her head's record does not decode: kept whole, written back as it was.
    Corrupt { raw: serde_json::Value, error: String },
    /// Her view was checkpointed with no head beside it, and no captured request of hers shows it.
    Unrecorded,
}

impl HandsRefusal {
    /// What she and her operator read.
    pub(crate) fn reason(&self) -> String {
        let why = match self {
            HandsRefusal::Corrupt { error, .. } => format!("her head's checkpointed custody does not decode ({error}); it is kept as it was"),
            HandsRefusal::Unrecorded => "her view was checkpointed with no head beside it and no captured request of hers shows that view".to_string(),
        };
        format!("{why}: her acts wait until her head is adopted (persona/hands/adopt), and her view is kept")
    }

    /// What a checkpoint keeps for it: a corrupt record as it was, else nothing recorded.
    pub(crate) fn custody(&self) -> HandsCustody {
        match self {
            HandsRefusal::Corrupt { raw, error } => HandsCustody::Corrupt { raw: raw.clone(), error: error.clone() },
            HandsRefusal::Unrecorded => HandsCustody::Unrecorded,
        }
    }
}

/// The checkpoint field: a recorded custody is written as recorded (a corrupt one as its raw bytes).
pub(crate) fn serialize<S: serde::Serializer>(custody: &HandsCustody, serializer: S) -> Result<S::Ok, S::Error> {
    use serde::Serialize;
    match custody {
        HandsCustody::Unrecorded => serializer.serialize_none(),
        HandsCustody::Held(hands) => hands.serialize(serializer),
        HandsCustody::Corrupt { raw, .. } => raw.serialize(serializer),
    }
}

/// The checkpoint field read back: absent is [`HandsCustody::Unrecorded`]; present but undecodable
/// is [`HandsCustody::Corrupt`], its bytes kept, and says so.
pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<HandsCustody, D::Error> {
    // Read as JSON first, so a record that does not decode as hands is still held whole.
    let recorded = <Option<serde_json::Value> as serde::Deserialize>::deserialize(deserializer)?;
    let Some(recorded) = recorded.filter(|value| !value.is_null()) else {
        return Ok(HandsCustody::Unrecorded);
    };
    Ok(match <HeldHands as serde::Deserialize>::deserialize(&recorded) {
        Ok(hands) => HandsCustody::Held(hands),
        Err(error) => {
            continuum_base::probe!(
                class = "tools.custody.corrupt",
                error = %error,
                "her head's recorded custody does not decode: it is kept whole and her act is refused until it is repaired"
            );
            HandsCustody::Corrupt { raw: recorded, error: error.to_string() }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_base::tool_contract::ContractRevision;

    fn spec(name: &str, description: &str) -> NativeToolSpec {
        NativeToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".into(),
                properties: serde_json::json!({}),
                required: None,
                definitions: None,
            },
        }
    }

    fn id(text: &str) -> ContractId {
        ContractId::of(ContractRevision::FIRST, text.as_bytes())
    }

    fn held(name: &str, frozen: &str) -> HeldHands {
        let whole: Arc<[NativeToolSpec]> = vec![spec(name, frozen)].into();
        HeldHands {
            generation: 1,
            names: vec![name.to_string()],
            levels: vec![0],
            reason: SurfaceReason::Hands,
            moves: Vec::new(),
            waiting: Vec::new(),
            menu: BTreeSet::new(),
            sent: Arc::clone(&whole),
            whole,
            sent_tokens: 10,
            demand_tokens: 10,
            contracts: vec![VerbContract::frozen(Some(id(frozen)))],
        }
    }

    fn offered(text: &str) -> LiveContract {
        LiveContract::Offered { id: id(text), spec: spec("work/note", text) }
    }

    // what this catches (Codex's review of 9cf06ade): A -> B -> A dropping the transition after she
    // was told about B, leaving her newest notice saying B while calls are checked against A.
    #[test]
    fn a_contract_restored_after_she_was_told_its_change_is_a_change_again() {
        let mut hands = held("work/note", "A");
        assert!(hands.changes(|_| offered("A")).is_empty(), "unchanged: nothing to tell");
        let to_b = hands.changes(|_| offered("B"));
        assert_eq!(to_b.len(), 1, "A -> B is told");
        assert!(hands.changes(|_| offered("A")).is_empty(), "B never told: back at A, she still knows A");
        hands.tell(&to_b[0]);
        assert!(hands.changes(|_| offered("B")).is_empty(), "told B: B is known");
        let back = hands.changes(|_| offered("A"));
        assert_eq!(back.len(), 1, "B -> A after she was told B is told again");
        assert_eq!(back[0].known, Some(id("B")));
        assert_ne!(back[0].key(), to_b[0].key(), "each transition is its own notice");
    }

    // what this catches: a removed verb run from her frozen schema, or told twice.
    #[test]
    fn a_removed_verb_is_told_once_and_a_return_is_told() {
        let mut hands = held("work/note", "A");
        let removed = hands.changes(|_| LiveContract::Removed);
        assert_eq!(removed.len(), 1);
        assert!(removed[0].text().contains("refused"), "she reads that a call is refused");
        hands.tell(&removed[0]);
        assert!(hands.changes(|_| LiveContract::Removed).is_empty(), "told removed: nothing more");
        assert_eq!(hands.changes(|_| offered("A")).len(), 1, "its return is a change she is told");
    }

    // what this catches: a restart that loses what she was told, or a corrupt record read as absent
    // (and so silently replaced by the live tools).
    #[test]
    fn custody_survives_its_checkpoint_and_a_corrupt_record_is_kept_whole() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Checkpoint {
            #[serde(default, serialize_with = "serialize", deserialize_with = "deserialize", skip_serializing_if = "HandsCustody::is_unrecorded")]
            hands: HandsCustody,
        }
        let mut hands = held("work/note", "A");
        let change = hands.changes(|_| offered("B")).remove(0);
        hands.tell(&change);
        let text = serde_json::to_string(&Checkpoint { hands: HandsCustody::Held(hands.clone()) }).expect("encodes");
        let back: Checkpoint = serde_json::from_str(&text).expect("decodes");
        assert_eq!(back.hands, HandsCustody::Held(hands), "generation, frozen bytes and what she was told survive");

        let absent: Checkpoint = serde_json::from_str("{}").expect("decodes");
        assert_eq!(absent.hands, HandsCustody::Unrecorded, "a checkpoint from before this records nothing");

        let corrupt: Checkpoint = serde_json::from_str(r#"{"hands":{"generation":"x"}}"#).expect("the checkpoint still decodes");
        let HandsCustody::Corrupt { raw, .. } = &corrupt.hands else {
            panic!("an undecodable record is corrupt, never absent"); // JUSTIFIED: test fixture
        };
        assert_eq!(raw, &serde_json::json!({"generation": "x"}), "it is kept whole");
        assert_eq!(serde_json::to_string(&corrupt).expect("encodes"), r#"{"hands":{"generation":"x"}}"#, "and written back as they were");
    }
}
