//! Her thoughts, one record each.
//!
//! Every thought she produces is ONE record under its occurrence, the shape of her action records
//! (`action_log`): its whole body while resident, its summary level built once when it is
//! recorded, the handle of its saved whole, and that whole's measured length once a shed moved
//! the body to the handle. The log is checkpointed with the rest of her working memory, so a turn
//! of many acts and a restart both leave her every thought; what of each body stays in RAM is the
//! one memory owner's to shed (`residency`, the same verb her results take), and how much of each
//! she reads is her view's one allocation (`context_streams::thoughts`). Before this a thought
//! shared the capacity-bounded `entries` ring with receipts and facts, so three acts pushed it out
//! and the work turn paged her thoughts back off the room store.

use super::residency::{RecordId, Resident};

/// One thought.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "ThoughtWire", into = "ThoughtWire")]
pub struct ThoughtRecord {
    /// Its place in her memory's one entry sequence (`WmEntry::occurrence`), checkpointed with it:
    /// what orders it among her settlements and names it in her view across a restart.
    pub occurrence: u64,
    /// Where the thought itself is: in RAM, or shed to its saved whole.
    pub body: ThoughtBody,
    /// Its summary level (`continuum_context::levels`), built once when it was recorded and kept
    /// after a shed: what her view gives it when it cannot keep it whole.
    pub summary: Option<String>,
    /// When she thought it (ms since the Unix epoch): the one clock a shed orders by.
    pub at_ms: u64,
}

/// Where a thought is. A thought with neither its text nor a saved whole cannot be represented:
/// before this a record of three `Option`s could hold one, and her view rendered it as an empty
/// message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThoughtBody {
    /// Her whole thought in RAM; `saved` names its saved whole once the memory owner wrote it.
    Resident { text: String, saved: Option<String> },
    /// Shed from RAM to its saved whole, `chars` long: what her view prices it at without reading it.
    Shed { handle: String, chars: std::num::NonZeroUsize },
}

impl ThoughtRecord {
    /// A thought as she produced it: resident whole, its summary level built now.
    pub fn new(occurrence: u64, text: String, at_ms: u64) -> Self {
        let summary = continuum_context::levels::summarize(&text, continuum_context::levels::Shape::Output);
        Self { occurrence, body: ThoughtBody::Resident { text, saved: None }, summary, at_ms }
    }

    /// The whole thought while it is in RAM.
    pub fn text(&self) -> Option<&str> {
        match &self.body {
            ThoughtBody::Resident { text, .. } => Some(text),
            ThoughtBody::Shed { .. } => None,
        }
    }

    /// Where its whole is saved: named once the memory owner wrote it, always once shed.
    pub fn saved_handle(&self) -> Option<&str> {
        match &self.body {
            ThoughtBody::Resident { saved, .. } => saved.as_deref(),
            ThoughtBody::Shed { handle, .. } => Some(handle),
        }
    }

    /// Its saved whole was written under `handle`. A shed thought already names its own.
    pub(super) fn saved_as(&mut self, handle: String) {
        if let ThoughtBody::Resident { saved, .. } = &mut self.body {
            *saved = Some(handle);
        }
    }
}

/// A thought as her checkpoint writes it: the shape every build reads and writes, so a checkpoint
/// decodes across a deploy and a revert alike.
#[derive(serde::Serialize, serde::Deserialize)]
struct ThoughtWire {
    occurrence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stored_chars: Option<usize>,
    at_ms: u64,
}

/// A checkpointed thought that names neither its text nor a saved whole with its length: nothing
/// of it can be shown or reloaded.
#[derive(Debug)]
pub struct UnreadableThought {
    pub occurrence: u64,
}

impl std::fmt::Display for UnreadableThought {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "thought {} names neither its text nor a saved whole with its length", self.occurrence)
    }
}

impl TryFrom<ThoughtWire> for ThoughtRecord {
    type Error = UnreadableThought;

    fn try_from(wire: ThoughtWire) -> Result<Self, Self::Error> {
        let ThoughtWire { occurrence, text, summary, handle, stored_chars, at_ms } = wire;
        let body = match (text, handle, stored_chars.and_then(std::num::NonZeroUsize::new)) {
            (Some(text), saved, _) => ThoughtBody::Resident { text, saved },
            (None, Some(handle), Some(chars)) => ThoughtBody::Shed { handle, chars },
            (None, _, _) => return Err(UnreadableThought { occurrence }),
        };
        Ok(Self { occurrence, body, summary, at_ms })
    }
}

impl From<ThoughtRecord> for ThoughtWire {
    fn from(record: ThoughtRecord) -> Self {
        let ThoughtRecord { occurrence, body, summary, at_ms } = record;
        let (text, handle, stored_chars) = match body {
            ThoughtBody::Resident { text, saved } => (Some(text), saved, None),
            ThoughtBody::Shed { handle, chars } => (None, Some(handle), Some(chars.get())),
        };
        Self { occurrence, text, summary, handle, stored_chars, at_ms }
    }
}

/// Her checkpointed thoughts: each that decodes, an unreadable one dropped with a probe naming it
/// (nothing of it could be shown), never failing the checkpoint it rides in.
pub(super) fn readable<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<ThoughtRecord>, D::Error> {
    let wires = <Vec<ThoughtWire> as serde::Deserialize>::deserialize(deserializer)?;
    Ok(wires
        .into_iter()
        .filter_map(|wire| match ThoughtRecord::try_from(wire) {
            Ok(record) => Some(record),
            Err(unreadable) => {
                continuum_base::probe!(
                    class = "persona.checkpoint.thought_unreadable",
                    occurrence = unreadable.occurrence,
                    "a checkpointed thought names neither its text nor a saved whole; it is dropped"
                );
                None
            }
        })
        .collect())
}

impl Resident for ThoughtRecord {
    fn id(&self) -> RecordId {
        RecordId::Thought(self.occurrence)
    }

    fn body(&self) -> Option<&str> {
        self.text()
    }

    fn handle(&self) -> Option<&str> {
        self.saved_handle()
    }

    fn received_ms(&self) -> Option<u64> {
        Some(self.at_ms)
    }

    fn resident_bytes(&self) -> u64 {
        [self.text(), self.summary.as_deref()].into_iter().flatten().map(|text| text.len() as u64).sum()
    }

    /// Only a saved, non-empty thought sheds: its whole must be reloadable at the length it had.
    fn shed_body(&mut self) {
        let ThoughtBody::Resident { text, saved: Some(handle) } = &self.body else {
            return;
        };
        if let Some(chars) = std::num::NonZeroUsize::new(text.chars().count()) {
            self.body = ThoughtBody::Shed { handle: handle.clone(), chars };
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ThoughtLog {
    /// Ascending by `occurrence`.
    records: Vec<ThoughtRecord>,
}

impl ThoughtLog {
    /// A log from records in any order. Nothing here bounds it: her RAM is the one memory owner's
    /// to shed (`residency`), and what she sees of it is her view's.
    pub fn from_records(mut records: Vec<ThoughtRecord>) -> Self {
        records.sort_by_key(|record| record.occurrence);
        records.dedup_by_key(|record| record.occurrence);
        Self { records }
    }

    pub fn push(&mut self, record: ThoughtRecord) {
        self.records.push(record);
    }

    /// Every thought, oldest first.
    pub fn records(&self) -> impl DoubleEndedIterator<Item = &ThoughtRecord> + Clone {
        self.records.iter()
    }

    pub fn get_mut(&mut self, occurrence: u64) -> Option<&mut ThoughtRecord> {
        let index = self.records.binary_search_by_key(&occurrence, |record| record.occurrence).ok()?;
        self.records.get_mut(index)
    }

    /// The RAM her thoughts hold, in bytes: bodies and summaries.
    pub fn resident_bytes(&self) -> u64 {
        self.records.iter().map(Resident::resident_bytes).sum()
    }

    /// Her resident thoughts with no saved whole yet, other than her newest: what the memory owner
    /// saves before it can shed one (the newest is never shed, so it is never saved for that).
    pub fn unsaved(&self) -> Vec<(u64, String)> {
        let newest = self.records.iter().rev().find(|record| record.text().is_some()).map(|record| record.occurrence);
        self.records
            .iter()
            .filter(|record| Some(record.occurrence) != newest)
            .filter_map(|record| match &record.body {
                ThoughtBody::Resident { text, saved: None } => Some((record.occurrence, text.clone())),
                ThoughtBody::Resident { saved: Some(_), .. } | ThoughtBody::Shed { .. } => None,
            })
            .collect()
    }

    /// Every handle a thought names: kept live while her log does.
    pub fn handles(&self) -> impl Iterator<Item = &str> {
        self.records.iter().filter_map(ThoughtRecord::saved_handle)
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}
