//! What of her records' bodies stays in RAM: the one memory owner decides.
//!
//! A body (an act's result, a thought) stays resident until the memory pressure monitor asks the
//! mind-results reporter to shed (`persona_workspace::mind_results_reporter`), never by a size cap
//! at receipt or restore: RAM decides, not a number (Joel, 2026-10-29). A shed moves the oldest
//! saved bodies to their handles; each keeps its measured length, so her view still prices its
//! whole level and loads it when the allocation grants it (FOCUS-CONTRACT rule 7,
//! `continuum_context::StoredWhole`). The newest of each kind always stays, and so does any body
//! whose saved output is not there to reload: it is the last copy. One verb for every kind
//! ([`Resident`]): her action log and her thought log shed the same way.

use super::*;

/// Which record a body belongs to: an act by its `#n`, a thought by its occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum RecordId {
    Act(u64),
    Thought(u64),
}

/// A record whose body a shed may move to its saved handle: an act's result, a thought.
pub(crate) trait Resident {
    fn id(&self) -> RecordId;
    /// Its body while resident.
    fn body(&self) -> Option<&str>;
    fn handle(&self) -> Option<&str>;
    /// When it was received; `None` (an older build's record) sheds first.
    fn received_ms(&self) -> Option<u64>;
    /// Every byte it holds in RAM: its body and what no shed releases (a line, a summary).
    fn resident_bytes(&self) -> u64;
    /// Move its body to its handle, keeping the whole's measured length for her view's price.
    fn shed_body(&mut self);

    /// The bytes a shed releases: its resident body.
    fn body_bytes(&self) -> u64 {
        self.body().map_or(0, |body| body.len() as u64) // map_or: a shed body releases nothing more
    }
}

/// One resident body a shed may move to its handle, if its saved output is there to reload
/// (the caller checks that outside her lock).
#[derive(Debug, Clone)]
pub(crate) struct ShedCandidate {
    /// When it was received; `None` (an older build's record) sheds first.
    pub received_ms: Option<u64>,
    pub id: RecordId,
    pub handle: String,
    pub bytes: u64,
}

/// The bodies of `records` (oldest first) a shed may move: resident, saved, and not the newest
/// resident one, which is what she just read.
fn shed_candidates<'a, R: Resident + 'a>(records: impl DoubleEndedIterator<Item = &'a R> + Clone + 'a) -> impl Iterator<Item = ShedCandidate> + 'a {
    let newest = records.clone().rev().find(|record| record.body().is_some()).map(Resident::id);
    records.filter(move |record| record.body().is_some() && Some(record.id()) != newest).filter_map(|record| {
        Some(ShedCandidate { received_ms: record.received_ms(), id: record.id(), handle: record.handle()?.to_owned(), bytes: record.body_bytes() })
    })
}

/// One mind's resident bodies, read under her locks with no filesystem access.
#[derive(Debug, Clone)]
pub(crate) struct ResidentResults {
    /// Bytes of every record: resident bodies, plus the lines and summaries no shed releases.
    pub resident_bytes: u64,
    /// Saved bodies other than each kind's newest: what a shed may move once each candidate's
    /// output is vouched for.
    pub candidates: Vec<ShedCandidate>,
}

impl WorkingMemory {
    /// The RAM her action and thought records hold, in bytes: bodies, lines and summaries.
    /// Cheap: one pass, no allocation.
    pub(crate) fn resident_result_bytes(&self) -> u64 {
        self.actions.lock().resident_bytes() + self.thoughts.lock().resident_bytes()
    }

    /// Her resident bodies and the saved ones a shed could move. Only memory is read under the
    /// locks: whether each handle's output is there is the caller's check, made after they drop.
    pub(crate) fn resident_results(&self) -> ResidentResults {
        let actions = self.actions.lock();
        let thoughts = self.thoughts.lock();
        let candidates = shed_candidates(actions.records()).chain(shed_candidates(thoughts.records())).collect();
        ResidentResults { resident_bytes: actions.resident_bytes() + thoughts.resident_bytes(), candidates }
    }

    /// A copy of one resident body, if its record still names `handle`: what the memory owner
    /// proves its saved copy against, outside the lock (the lock is held only for the copy).
    pub(crate) fn resident_body(&self, id: RecordId, handle: &str) -> Option<String> {
        let copy = |record: &dyn Resident| (record.handle() == Some(handle)).then(|| record.body().map(str::to_owned)).flatten();
        match id {
            RecordId::Act(seq) => self.actions.lock().get(seq).and_then(|record| copy(record)),
            RecordId::Thought(occurrence) => self.thoughts.lock().records().find(|record| record.occurrence == occurrence).and_then(|record| copy(record)),
        }
    }

    /// Save every resident thought that has no saved whole yet (her newest excepted), so a shed
    /// can move it: written outside her lock through one [`LivePin`] held until the handles are
    /// published. Only the memory owner calls it, at shed time, never the act that recorded it.
    ///
    /// [`LivePin`]: crate::cognition::tool_executor::spill::LivePin
    pub(crate) fn save_unsaved_thoughts(&self, persona_id: Uuid) {
        let unsaved = self.thoughts.lock().unsaved();
        if unsaved.is_empty() {
            return;
        }
        let pin = crate::cognition::tool_executor::spill::LivePin::new();
        let saved: Vec<(u64, String)> =
            unsaved.into_iter().filter_map(|(occurrence, text)| pin.retain(persona_id, &text).handle().map(|handle| (occurrence, handle.to_owned()))).collect();
        {
            let mut thoughts = self.thoughts.lock();
            for (occurrence, handle) in saved {
                if let Some(record) = thoughts.get_mut(occurrence) {
                    record.handle = Some(handle);
                }
            }
        }
        self.publish_live_handles();
        drop(pin);
    }

    /// THE eviction verb: move the oldest saved bodies, acts and thoughts alike, to their handles
    /// until at most `budget` bytes stay resident. Only the memory owner's shed calls it in
    /// production, with `saved(id, handle)` answering from the copies it proved outside the lock,
    /// so a record that changed since (a different handle, or gone) is not moved. No filesystem
    /// access happens under the lock. Each kind's newest stays, and past the budget after that the
    /// overrun is reported, never forced.
    pub(crate) fn evict_resident_results_over(&self, budget: u64, saved: &dyn Fn(RecordId, &str) -> bool) {
        let total = {
            let mut actions = self.actions.lock();
            let mut thoughts = self.thoughts.lock();
            let mut total = actions.resident_bytes() + thoughts.resident_bytes();
            if total > budget {
                let mut chosen: Vec<ShedCandidate> = shed_candidates(actions.records())
                    .chain(shed_candidates(thoughts.records()))
                    .filter(|candidate| saved(candidate.id, &candidate.handle))
                    .collect();
                chosen.sort_by_key(|candidate| (candidate.received_ms, candidate.id));
                let mut over = total - budget;
                for candidate in chosen {
                    if over == 0 {
                        break;
                    }
                    let record: Option<&mut dyn Resident> = match candidate.id {
                        RecordId::Act(seq) => actions.get_mut(seq).map(|record| record as &mut dyn Resident),
                        RecordId::Thought(occurrence) => thoughts.get_mut(occurrence).map(|record| record as &mut dyn Resident),
                    };
                    if let Some(record) = record {
                        let bytes = record.body_bytes();
                        record.shed_body();
                        over = over.saturating_sub(bytes);
                        total -= bytes;
                    }
                }
            }
            total
        };
        self.publish_live_handles();
        if total > budget {
            continuum_base::probe!(
                class = "wm.results.last_copies_over_allocation",
                total = %total,
                budget = %budget,
                "bodies with no saved output to reload stay resident over the shed's allocation"
            );
        }
    }
}
