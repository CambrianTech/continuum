//! One epoch of a mind's model view: a stable head, then versioned events in arrival order.
//!
//! Within an epoch the view only grows at its end. A source whose rendering changes does not
//! rewrite its earlier block; its new version is appended and supersedes the old one. So
//! every act's prompt is the previous act's prompt plus a tail, and the engine reuses the
//! whole previous prefix. Superseded versions cost tokens until the view starts a new epoch
//! (`view::View::turn_over`), which re-renders from the sources rather than editing this one.

/// Names the producer of a block (the results ring, the kanban, recall, a perception feed).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct SourceId(pub String);

/// What a block holds: plain text, or a caller's richer rendering (a role-tagged message).
/// The epoch compares blocks for equality and counts their characters; nothing else.
pub trait Block: Clone + PartialEq + std::hash::Hash {
    /// The block's text: what it costs and what its content hash is taken over.
    fn text(&self) -> &str;

    /// Characters this block costs the view, the unit [`Epoch::superseded_chars`] counts:
    /// characters, never bytes (bytes would weigh a non-ASCII block heavier than its twin and skew
    /// the one allocation).
    fn chars(&self) -> usize {
        self.text().chars().count()
    }
}

impl Block for String {
    fn text(&self) -> &str {
        self
    }
}

/// One rendered block: what a source said, and which of its versions this is.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Event<T = String> {
    pub source: SourceId,
    /// The occurrence this block renders (`crate::cause`); `None` for a source's fold block,
    /// which stands for a range of items.
    pub element: Option<crate::cause::ElementId>,
    /// The level the view rendered it at: what a render receipt reports she was shown.
    pub level: crate::level::Level,
    pub version: u32,
    pub text: T,
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

/// A point in an epoch's events: what [`Epoch::rewind`] returns it to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Mark {
    events: usize,
    superseded_chars: usize,
}

/// A mind's view for one epoch.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Epoch<T = String> {
    id: u64,
    head: Vec<T>,
    events: Vec<Event<T>>,
    superseded_chars: usize,
}

impl<T: Block> Epoch<T> {
    /// A fresh epoch whose stable head (identity, policy, tool schemas) never changes within it.
    pub fn new(id: u64, head: Vec<T>) -> Self {
        Self { id, head, events: Vec::new(), superseded_chars: 0 }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Record a source's current rendering. Appends only when it differs from that source's
    /// latest version; never edits an earlier block.
    pub fn observe(&mut self, source: SourceId, element: Option<crate::cause::ElementId>, level: crate::level::Level, text: T) -> Observed {
        let latest = self.events.iter().rposition(|e| e.source == source);
        if let Some(i) = latest {
            if self.events[i].text == text {
                return Observed::Unchanged;
            }
            self.superseded_chars += self.events[i].text.chars();
        }
        let version = latest.map_or(0, |i| self.events[i].version + 1);
        self.events.push(Event { source, element, level, version, text, retired: false });
        Observed::Appended { version }
    }

    /// A source ended (a view closed, a result was revoked from her view): append a short
    /// superseding notice instead of deleting its block, so the prefix never moves. Removal of
    /// the tokens themselves happens at the next turnover; a privacy revocation that cannot
    /// wait for it is the caller's reason to turn over now.
    pub fn retire(&mut self, source: SourceId, notice: T) -> Observed {
        let Some(i) = self.events.iter().rposition(|e| e.source == source) else {
            return Observed::Unchanged;
        };
        if self.events[i].retired {
            return Observed::Unchanged;
        }
        self.superseded_chars += self.events[i].text.chars();
        let (element, version) = (self.events[i].element, self.events[i].version + 1);
        self.events.push(Event { source, element, level: crate::level::Level::Pointer, version, text: notice, retired: true });
        Observed::Appended { version }
    }

    /// Where the view stands now, to [`Epoch::rewind`] to before anything is sent from it.
    pub(crate) fn mark(&self) -> Mark {
        Mark { events: self.events.len(), superseded_chars: self.superseded_chars }
    }

    /// Drop what was appended since `mark`. Only for an act whose prompt was never sent: the
    /// engine holds nothing past the mark, so nothing it cached moves.
    pub(crate) fn rewind(&mut self, mark: Mark) {
        self.events.truncate(mark.events);
        self.superseded_chars = mark.superseded_chars;
    }

    /// Whether `source` has a version in this epoch.
    pub(crate) fn holds(&self, source: &SourceId) -> bool {
        self.events.iter().any(|e| &e.source == source)
    }

    /// Every block after the head, in prompt order, superseded versions included: what a render
    /// receipt projects.
    pub fn events(&self) -> &[Event<T>] {
        &self.events
    }

    /// The view in order: the head, then every event as it arrived.
    pub fn render(&self) -> impl Iterator<Item = &T> {
        self.head.iter().chain(self.events.iter().map(|e| &e.text))
    }

    /// The characters the whole view costs, superseded versions included.
    pub fn chars(&self) -> usize {
        self.render().map(Block::chars).sum()
    }

    /// Characters held by versions a later version superseded: the price of never rewriting.
    pub fn superseded_chars(&self) -> usize {
        self.superseded_chars
    }
}

#[cfg(test)]
mod tests {
    /// A test block: whole, standing for no particular occurrence.
    fn seen(e: &mut Epoch<String>, source: SourceId, text: String) -> Observed {
        e.observe(source, None, crate::level::Level::Full, text)
    }

    use super::*;

    fn src(s: &str) -> SourceId {
        SourceId(s.to_string())
    }

    fn joined(e: &Epoch) -> String {
        e.render().map(String::as_str).collect::<Vec<_>>().join("\n")
    }

    // what this catches: any act that rewrites an earlier block, which made every restored KV
    // page cold on the M5 (10-19: the results ring mid-prompt re-prefilled 17-29k tokens per act).
    #[test]
    fn within_an_epoch_every_act_only_appends() {
        let mut e = Epoch::<String>::new(1, vec!["identity".into(), "tools".into()]);
        let acts: [&[(&str, &str)]; 4] = [
            &[("kanban", "board v1"), ("ring", "result 1")],
            &[("kanban", "board v1"), ("ring", "result 1\nresult 2")],
            &[("kanban", "board v2"), ("ring", "result 1\nresult 2")],
            &[("recall", "a memory"), ("kanban", "board v2")],
        ];
        let mut previous = joined(&e);
        for act in acts {
            for (s, t) in act {
                seen(&mut e, src(s), t.to_string());
            }
            let now = joined(&e);
            assert!(now.starts_with(&previous), "an act rewrote the prefix:\n{previous}\n---\n{now}");
            previous = now;
        }
    }

    // what this catches: re-sending an unchanged block as new tokens.
    #[test]
    fn an_unchanged_source_appends_nothing() {
        let mut e = Epoch::<String>::new(1, vec![]);
        assert_eq!(seen(&mut e, src("kanban"), "v1".into()), Observed::Appended { version: 0 });
        assert_eq!(seen(&mut e, src("kanban"), "v1".into()), Observed::Unchanged);
        assert_eq!(seen(&mut e, src("kanban"), "v2".into()), Observed::Appended { version: 1 });
        assert_eq!(e.superseded_chars(), 2);
    }

    // what this catches: a whole-block source (open views, working memory) re-appending all of
    // itself when one item changes. Keyed per item, an act appends only what changed: replayed
    // on the M5's captures, whole-block keys grew the view ~19k chars per act.
    #[test]
    fn per_item_sources_append_only_the_item_that_changed() {
        let mut e = Epoch::<String>::new(1, vec![]);
        for view in ["a.rs", "b.rs", "c.rs"] {
            seen(&mut e, src(&format!("open-views/{view}")), format!("{view} v1 {}", "x".repeat(1000)));
        }
        let before: usize = e.chars();
        seen(&mut e, src("open-views/a.rs"), "a.rs v1 ".to_string() + &"x".repeat(1000));
        seen(&mut e, src("open-views/b.rs"), "b.rs v2".into());
        seen(&mut e, src("open-views/c.rs"), "c.rs v1 ".to_string() + &"x".repeat(1000));
        let after: usize = e.chars();
        assert_eq!(after - before, "b.rs v2".len(), "only the changed view was appended");
    }

    // what this catches: a closed view still read as open (no notice appended), or a close
    // that rewrites the prefix to drop the view.
    #[test]
    fn retiring_a_source_appends_its_notice_once() {
        let mut e = Epoch::<String>::new(1, vec![]);
        seen(&mut e, src("open-views/a.rs"), "a.rs contents".into());
        let before = joined(&e);
        assert_eq!(e.retire(src("open-views/a.rs"), "[closed a.rs]".into()), Observed::Appended { version: 1 });
        assert!(joined(&e).starts_with(&before));
        assert_eq!(e.retire(src("open-views/never-opened"), "[closed]".into()), Observed::Unchanged);
        assert_eq!(e.retire(src("open-views/a.rs"), "[closed a.rs]".into()), Observed::Unchanged, "retired once");
    }
}
