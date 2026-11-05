//! Her thoughts, one record each.
//!
//! Every thought she produces is ONE record under its occurrence: what she thought and when.
//! Like her actions (`action_log`), the log is kept whole and checkpointed with the rest of her
//! working memory, so a turn of many acts and a restart both leave her every thought; how much of
//! each she reads is her view's one allocation (`context_streams::thoughts`: whole, its summary
//! level, or its pointer, collapsed only oldest first at an epoch start). Before this a thought
//! shared the capacity-bounded `entries` ring with receipts and facts, so three acts pushed it out
//! and the work turn paged her thoughts back off the room store with a page size, a keep-count and
//! a head cut of the older ones.

/// One thought.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ThoughtRecord {
    /// Its place in her memory's one entry sequence (`WmEntry::occurrence`), checkpointed with it:
    /// what orders it among her settlements and names it in her view across a restart.
    pub occurrence: u64,
    pub text: String,
    /// When she thought it (ms since the Unix epoch).
    pub at_ms: u64,
}

#[derive(Debug, Default)]
pub(super) struct ThoughtLog {
    /// Ascending by `occurrence`.
    records: Vec<ThoughtRecord>,
}

impl ThoughtLog {
    /// A log from records in any order. Nothing here bounds it: her RAM is the one memory owner's
    /// to account (`residency`), and what she sees of it is her view's.
    pub fn from_records(mut records: Vec<ThoughtRecord>) -> Self {
        records.sort_by_key(|record| record.occurrence);
        records.dedup_by_key(|record| record.occurrence);
        Self { records }
    }

    pub fn push(&mut self, record: ThoughtRecord) {
        self.records.push(record);
    }

    /// Every thought, oldest first.
    pub fn records(&self) -> impl Iterator<Item = &ThoughtRecord> {
        self.records.iter()
    }

    /// The occurrence after her newest thought: where a restored memory's entry sequence resumes.
    pub fn next_occurrence(&self) -> u64 {
        self.records.last().map_or(1, |record| record.occurrence + 1) // map_or: no thought yet, the sequence starts at its first
    }

    /// The RAM her thoughts hold, in bytes.
    pub fn resident_bytes(&self) -> u64 {
        self.records.iter().map(|record| record.text.len() as u64).sum()
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}
