//! `WorkingMemory`'s history bound: what her view folded leaves RAM for her archive, the archive
//! ticks with her checkpoint, and an act that left still reopens by its `#n`.
//!
//! The bound is her view's, measured: each start names the newest act it folded
//! (`View::fold_edge`), and every act up to it leaves the resident log at that start
//! ([`WorkingMemory::release_folded`]): O(acts drained), once per start. No count decides it.
//! A memory with no archive bound (a fork, a test that does not ask for one) keeps its log: there
//! is nowhere durable for an act to go.

use super::*;

impl WorkingMemory {
    /// Her checkpointed state's generation: it moves with every change a checkpoint would carry, so
    /// a checkpoint that finds it where the last one left it has nothing to write.
    pub(crate) fn checkpoint_generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Something her checkpoint carries changed.
    pub(super) fn touch(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Give her an archive: her own directory of the tool-output class. Called once where her
    /// resident mind is registered (a blocking worker at boot); until then her log is not bounded.
    pub fn bind_archive(&self, persona_id: Uuid) -> std::io::Result<()> {
        let store = crate::cognition::tool_executor::spill::store()?.owned_by(persona_id);
        let archive = archive::Archive::bind(persona_id, store)?;
        *self.archive.lock() = archive;
        Ok(())
    }

    /// Her view just started an epoch: every act up to its fold edge leaves RAM for her archive,
    /// and the view forgets their ids (its fold block's bytes stay). Nothing when her view folded
    /// no act or she has no archive.
    pub(crate) fn release_folded(&self, view: &mut ContextView) {
        let actions_source = crate::cognition::context_streams::actions::NAME;
        let Some(edge) = view.fold_edge(actions_source).and_then(|key| key.parse::<u64>().ok()) else {
            return;
        };
        let edge = self.release_edge(edge);
        if self.release_through(edge) > 0 {
            view.release(actions_source, |key| key.parse::<u64>().is_ok_and(|seq| seq <= edge));
        }
    }

    /// `edge`, never at or past the act in flight: it is newer than anything her view folded.
    fn release_edge(&self, edge: u64) -> u64 {
        let in_flight = self.last_action.lock().as_ref().map(|(seq, _)| *seq);
        in_flight.map_or(edge, |seq| edge.min(seq.saturating_sub(1))) // map_or: nothing in flight leaves the edge as measured
    }

    /// Move every resident act up to `edge` into her archive's outbox (written by the next
    /// checkpoint tick, off the act path); how many left. The act in flight never does: it is
    /// newer than anything her view folded.
    pub(crate) fn release_through(&self, edge: u64) -> usize {
        let mut archive = self.archive.lock();
        if !archive.is_bound() {
            return 0;
        }
        let edge = self.release_edge(edge);
        let drained = self.actions.lock().drain_through(edge);
        let count = drained.len();
        if count == 0 {
            return 0;
        }
        {
            let mut children = self.receipt_children.lock();
            for record in drained {
                let kids = children.remove(&record.receipt_key()).unwrap_or_default(); // unwrap_or_default: a receipt with no per-call outputs pointed at none
                archive.queue(record, kids);
            }
        }
        drop(archive);
        self.touch();
        self.publish_live_handles();
        continuum_base::probe!(
            class = "wm.history.released",
            instance = self.instance,
            edge = edge,
            released = count,
            resident = self.actions.lock().resident(),
            "acts her view folded left RAM for her archive"
        );
        count
    }

    /// One archive tick, run by her checkpoint before it writes (so an act is on disk before the
    /// checkpoint that no longer carries it replaces the one that did): the outbox as one segment,
    /// one retention check, one compaction step.
    pub(crate) fn tick_archive(&self) -> std::io::Result<archive::ArchiveTick> {
        archive::tick(&self.archive, self.actions_taken())
    }

    /// Whether any of her acts lives in her archive rather than in RAM.
    pub(crate) fn holds_archived_acts(&self) -> bool {
        self.archive.lock().holds_acts()
    }

    /// How many segments her archive holds (a receipt for tests and probes).
    #[cfg(test)]
    pub(crate) fn archive_segments(&self) -> usize {
        self.archive.lock().segments()
    }

    /// Her resident acts' count (a receipt for tests and probes).
    pub(crate) fn resident_acts(&self) -> usize {
        self.actions.lock().resident()
    }

    /// The saved output of act `seq` once it left RAM: from her outbox, else the one segment that
    /// holds it, read for this call and let go. `None` when her archive does not hold it.
    pub(super) fn archived_handle(&self, seq: u64) -> Option<Result<String, ActOutputRefusal>> {
        let located = {
            let archive = self.archive.lock();
            if let Some(act) = archive.pending(seq) {
                return Some(act.handle.ok_or(ActOutputRefusal::Unsaved { act: seq }));
            }
            archive.locate(seq)?
        };
        let (bound, segment) = located;
        let found = match archive::find(&bound, &segment, seq) {
            Ok(found) => found,
            Err(error) => {
                continuum_base::probe!(
                    class = "wm.history.reopen_unreadable",
                    act = seq,
                    error = %error,
                    "an archived act's segment could not be read"
                );
                return Some(Err(ActOutputRefusal::Unreadable { act: seq, error: error.to_string() }));
            }
        };
        self.archive.lock().note_reopen(self.actions_taken().saturating_sub(seq));
        Some(match found {
            archive::Found::Act(act) => act.handle.ok_or(ActOutputRefusal::Unsaved { act: seq }),
            archive::Found::Compacted(span) => Err(ActOutputRefusal::Compacted { act: seq, span }),
            archive::Found::Retired => Err(ActOutputRefusal::Retired { act: seq }),
        })
    }
}
