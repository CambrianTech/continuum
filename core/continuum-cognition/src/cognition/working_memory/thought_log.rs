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
pub struct ThoughtRecord {
    /// Its place in her memory's one entry sequence (`WmEntry::occurrence`), checkpointed with it:
    /// what orders it among her settlements and names it in her view across a restart.
    pub occurrence: u64,
    /// The whole thought while resident; `None` once a shed moved it to its handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Its summary level (`continuum_context::levels`), built once when it was recorded and kept
    /// after a shed: what her view gives it when it cannot keep it whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The saved whole (`spill`): where a shed moves the body and her view reloads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    /// The whole's length once its body left RAM: what her view prices it at without reading it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_chars: Option<usize>,
    /// When she thought it (ms since the Unix epoch): the one clock a shed orders by.
    pub at_ms: u64,
}

impl ThoughtRecord {
    /// A thought as she produced it: resident whole, its summary level built now.
    pub fn new(occurrence: u64, text: String, at_ms: u64) -> Self {
        let summary = continuum_context::levels::summarize(&text, continuum_context::levels::Shape::Output);
        Self { occurrence, text: Some(text), summary, handle: None, stored_chars: None, at_ms }
    }
}

impl Resident for ThoughtRecord {
    fn id(&self) -> RecordId {
        RecordId::Thought(self.occurrence)
    }

    fn body(&self) -> Option<&str> {
        self.text.as_deref()
    }

    fn handle(&self) -> Option<&str> {
        self.handle.as_deref()
    }

    fn received_ms(&self) -> Option<u64> {
        Some(self.at_ms)
    }

    fn resident_bytes(&self) -> u64 {
        [&self.text, &self.summary].into_iter().map(|text| text.as_ref().map_or(0, |text| text.len() as u64)).sum()
    }

    fn shed_body(&mut self) {
        self.stored_chars = self.text.as_ref().map(|text| text.chars().count());
        self.text = None;
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
        let newest = self.records.iter().rev().find(|record| record.text.is_some()).map(|record| record.occurrence);
        self.records
            .iter()
            .filter(|record| record.handle.is_none() && Some(record.occurrence) != newest)
            .filter_map(|record| record.text.clone().map(|text| (record.occurrence, text)))
            .collect()
    }

    /// Every handle a thought names: kept live while her log does.
    pub fn handles(&self) -> impl Iterator<Item = &str> {
        self.records.iter().filter_map(|record| record.handle.as_deref())
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}
