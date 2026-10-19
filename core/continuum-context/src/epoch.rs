//! One epoch of a mind's model view: a stable head, then versioned events in arrival order.
//!
//! Within an epoch the view only grows at its end. A source whose rendering changes does not
//! rewrite its earlier block; its new version is appended and supersedes the old one. So
//! every act's prompt is the previous act's prompt plus a tail, and the engine reuses the
//! whole previous prefix. Superseded versions cost tokens until the caller turns the epoch
//! over, which compacts the view to the latest version of each source.

/// Names the producer of a block (the results ring, the kanban, recall, a perception feed).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(pub String);

/// One rendered block: what a source said, and which of its versions this is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub source: SourceId,
    pub version: u32,
    pub text: String,
    /// The source ended with this notice; turnover drops it entirely.
    pub retired: bool,
}

/// What [`Epoch::observe`] did with a source's current rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Observed {
    /// Identical to the source's latest version: nothing appended, the view is unchanged.
    Unchanged,
    /// Appended as a new version at the end of the view.
    Appended { version: u32 },
}

/// A mind's view for one epoch.
#[derive(Clone, Debug, Default)]
pub struct Epoch {
    id: u64,
    head: Vec<String>,
    events: Vec<Event>,
    superseded_chars: usize,
}

impl Epoch {
    /// A fresh epoch whose stable head (identity, policy, tool schemas) never changes within it.
    pub fn new(id: u64, head: Vec<String>) -> Self {
        Self { id, head, events: Vec::new(), superseded_chars: 0 }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Record a source's current rendering. Appends only when it differs from that source's
    /// latest version; never edits an earlier block.
    pub fn observe(&mut self, source: SourceId, text: String) -> Observed {
        let latest = self.events.iter().rposition(|e| e.source == source);
        if let Some(i) = latest {
            if self.events[i].text == text {
                return Observed::Unchanged;
            }
            self.superseded_chars += self.events[i].text.len();
        }
        let version = latest.map_or(0, |i| self.events[i].version + 1);
        self.events.push(Event { source, version, text, retired: false });
        Observed::Appended { version }
    }

    /// A source ended (a view closed, a result was revoked from her view): append a short
    /// superseding notice instead of deleting its block, so the prefix never moves. Removal of
    /// the tokens themselves happens at the next turnover; a privacy revocation that cannot
    /// wait for it is the caller's reason to turn over now.
    pub fn retire(&mut self, source: SourceId, notice: String) -> Observed {
        let Some(i) = self.events.iter().rposition(|e| e.source == source) else {
            return Observed::Unchanged;
        };
        if self.events[i].retired {
            return Observed::Unchanged;
        }
        self.superseded_chars += self.events[i].text.len();
        let version = self.events[i].version + 1;
        self.events.push(Event { source, version, text: notice, retired: true });
        Observed::Appended { version }
    }

    /// The view in order: the head, then every event as it arrived.
    pub fn render(&self) -> impl Iterator<Item = &str> {
        self.head.iter().map(String::as_str).chain(self.events.iter().map(|e| e.text.as_str()))
    }

    /// Characters held by versions a later version superseded: the price of never rewriting.
    pub fn superseded_chars(&self) -> usize {
        self.superseded_chars
    }

    /// The next epoch: the same head, then only the latest version of each source, in the
    /// order those latest versions arrived. The caller decides WHEN (a budget, a revocation,
    /// a stale prerequisite); this only says WHAT the compacted view is.
    pub fn turn_over(&self, next_id: u64) -> Epoch {
        // One pass from the end: the first time a source is seen there is its latest version.
        let mut seen = std::collections::HashSet::new();
        let mut events: Vec<Event> =
            self.events.iter().rev().filter(|e| seen.insert(&e.source) && !e.retired).cloned().collect();
        events.reverse();
        Epoch { id: next_id, head: self.head.clone(), events, superseded_chars: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(s: &str) -> SourceId {
        SourceId(s.to_string())
    }

    fn joined(e: &Epoch) -> String {
        e.render().collect::<Vec<_>>().join("\n")
    }

    // what this catches: any act that rewrites an earlier block, which made every restored KV
    // page cold on the M5 (10-19: the results ring mid-prompt re-prefilled 17-29k tokens per act).
    #[test]
    fn within_an_epoch_every_act_only_appends() {
        let mut e = Epoch::new(1, vec!["identity".into(), "tools".into()]);
        let acts: [&[(&str, &str)]; 4] = [
            &[("kanban", "board v1"), ("ring", "result 1")],
            &[("kanban", "board v1"), ("ring", "result 1\nresult 2")],
            &[("kanban", "board v2"), ("ring", "result 1\nresult 2")],
            &[("recall", "a memory"), ("kanban", "board v2")],
        ];
        let mut previous = joined(&e);
        for act in acts {
            for (s, t) in act {
                e.observe(src(s), t.to_string());
            }
            let now = joined(&e);
            assert!(now.starts_with(&previous), "an act rewrote the prefix:\n{previous}\n---\n{now}");
            previous = now;
        }
    }

    // what this catches: re-sending an unchanged block as new tokens.
    #[test]
    fn an_unchanged_source_appends_nothing() {
        let mut e = Epoch::new(1, vec![]);
        assert_eq!(e.observe(src("kanban"), "v1".into()), Observed::Appended { version: 0 });
        assert_eq!(e.observe(src("kanban"), "v1".into()), Observed::Unchanged);
        assert_eq!(e.observe(src("kanban"), "v2".into()), Observed::Appended { version: 1 });
        assert_eq!(e.superseded_chars(), 2);
    }

    // what this catches: a whole-block source (open views, working memory) re-appending all of
    // itself when one item changes. Keyed per item, an act appends only what changed: replayed
    // on the M5's captures, whole-block keys grew the view ~19k chars per act.
    #[test]
    fn per_item_sources_append_only_the_item_that_changed() {
        let mut e = Epoch::new(1, vec![]);
        for view in ["a.rs", "b.rs", "c.rs"] {
            e.observe(src(&format!("open-views/{view}")), format!("{view} v1 {}", "x".repeat(1000)));
        }
        let before: usize = e.render().map(str::len).sum();
        e.observe(src("open-views/a.rs"), "a.rs v1 ".to_string() + &"x".repeat(1000));
        e.observe(src("open-views/b.rs"), "b.rs v2".into());
        e.observe(src("open-views/c.rs"), "c.rs v1 ".to_string() + &"x".repeat(1000));
        let after: usize = e.render().map(str::len).sum();
        assert_eq!(after - before, "b.rs v2".len(), "only the changed view was appended");
    }

    // what this catches: a closed view still read as open (no change appended), or a close
    // that rewrites the prefix to drop the view.
    #[test]
    fn retiring_a_source_appends_its_notice_and_turnover_drops_it() {
        let mut e = Epoch::new(1, vec![]);
        e.observe(src("open-views/a.rs"), "a.rs contents".into());
        let before = joined(&e);
        assert_eq!(e.retire(src("open-views/a.rs"), "[closed a.rs]".into()), Observed::Appended { version: 1 });
        assert!(joined(&e).starts_with(&before));
        assert_eq!(e.retire(src("open-views/never-opened"), "[closed]".into()), Observed::Unchanged);
        assert_eq!(e.retire(src("open-views/a.rs"), "[closed a.rs]".into()), Observed::Unchanged, "retired once");
        assert_eq!(e.turn_over(2).render().count(), 0, "a retired source leaves nothing behind");
    }

    // what this catches: a turnover that loses a source, keeps a stale version, or reorders.
    #[test]
    fn turnover_keeps_only_each_sources_latest_version_in_arrival_order() {
        let mut e = Epoch::new(1, vec!["head".into()]);
        e.observe(src("kanban"), "board v1".into());
        e.observe(src("ring"), "r1".into());
        e.observe(src("kanban"), "board v2".into());
        let next = e.turn_over(2);
        assert_eq!(next.id(), 2);
        assert_eq!(next.render().collect::<Vec<_>>(), ["head", "r1", "board v2"]);
        assert_eq!(next.superseded_chars(), 0);
    }
}
