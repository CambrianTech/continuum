//! Open views: the paging cursors of a mind working through data larger than its window.
//!
//! A file of a few hundred lines, or a saved tool result, is read in pages. What she has
//! read of it used to live only in the conversation history that carried each page, and
//! the history fit drops the oldest messages first. Measured on the M5, 2026-10-17: Benchy
//! held a card that needed a 498-line, 22 KB file (`core/source-hygiene/src/lib.rs`); her
//! prompt asked for ~66k tokens against a 34,560-token window, the fit dropped 5 of 9
//! history messages including her earlier reads, and every turn she re-read the file from
//! line 1 and never reached the write. A bigger window only moves that wall; a mind with a
//! 5,000-token window hits it on the first page (Joel: "paging cursors", "Positron needs to
//! deliver this common interface to large data").
//!
//! So working memory keeps, per view, what it is, how big it is, which line ranges she has
//! read (merged), and the handle of the saved whole result when there is one. It is state
//! of working memory, checkpointed with it, and rendered every turn as one line per view in
//! the required notices, which the history fit never drops. It is filled from the typed
//! acts at the receipt seam ([`OpenViews::note_acts`]); nothing here re-reads prose a
//! producer did not define as its own format.

use crate::ai::types::ResultCustody;
use crate::cognition::act_observe::{Observation, ToolVerb};

/// What a view is OF.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum ViewSubject {
    /// A file in the checkout she is rooted at, by the path `code/read` reported.
    File { path: String },
    /// A tool result saved whole because it was larger than her share for one result.
    Output { handle: String },
}

/// One open view: a large thing she is working through, and how far.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OpenView {
    pub subject: ViewSubject,
    /// The command that opened it (`code/read`, `code/shell`, ...).
    pub label: String,
    /// Lines in the whole thing, when its producer said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<u32>,
    /// The 1-based inclusive line ranges she has been shown, sorted and merged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seen: Vec<(u32, u32)>,
    /// Where the whole result of the read that last opened it is saved, when it was larger
    /// than what she was shown (`ResultCustody::Pointer`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    /// The receipt `#seq` that last touched this view; the oldest leaves first.
    pub touched_seq: u64,
    /// The acting root she stood at (a card's checkout), so a path means the file in THAT
    /// checkout; `None` = her own workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Intent stated before the read, not a conclusion drawn from its result. Offered only
    /// in the whole representation; the shared context allocator decides its visibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Storage retention in records, not a prompt budget. Cursor and intent costs are measured
/// separately by the shared context allocator. The ninth view evicts the least recently
/// touched record; this bound must never be used to estimate its rendered cost.
// derived-or-floor: a count of retained records in her own state, never prompt tokens
const OPEN_VIEWS_KEPT: usize = 8;

/// The open views of one mind, least recently touched first.
#[derive(Debug, Clone, Default)]
pub struct OpenViews {
    views: Vec<OpenView>,
}

impl OpenViews {
    pub fn from_saved(mut views: Vec<OpenView>) -> Self {
        views.sort_by_key(|v| v.touched_seq);
        let mut this = Self { views };
        this.evict_over_kept();
        this
    }

    pub fn saved(&self) -> Vec<OpenView> {
        self.views.clone()
    }


    /// Record what one receipt's acts opened or paged. Returns whether any view changed.
    pub fn note_acts(&mut self, acts: &[Observation], seq: u64, scope: Option<&str>) -> bool {
        let mut changed = false;
        for act in acts {
            changed |= self.note_act(act, seq, scope);
        }
        changed
    }

    fn note_act(&mut self, act: &Observation, seq: u64, scope: Option<&str>) -> bool {
        let result = &act.output.result;
        if act.output.verb == ToolVerb::Read {
            return self.note_file_read(act, seq, scope);
        }
        let name = crate::ai::tool_dialect::resolve_wire_name(&act.call.name);
        if name == "tool/output" {
            return self.note_output_page(act, seq);
        }
        // Any other result saved whole because it was too big to show: open it as an output,
        // successful or not (a failing build's log is exactly what gets paged).
        if let ResultCustody::Pointer { handle } = &result.custody {
            let subject = ViewSubject::Output { handle: handle.clone() };
            let view = self.view_mut(subject, &name, seq, scope);
            view.touched_seq = seq;
            return true;
        }
        false
    }

    /// A `code/read`: the range she was shown, from the read's own header when she was shown
    /// all of it, and from the gutter numbers of the head she was shown when it was stored.
    fn note_file_read(&mut self, act: &Observation, seq: u64, scope: Option<&str>) -> bool {
        if !act.completed_successfully() {
            return false;
        }
        let result = &act.output.result;
        let header = crate::cognition::tool_executor::spill::result_scalars(&result.content);
        let field_u32 = |key: &str| -> Option<u32> {
            header.as_ref()?.get(key)?.as_u64().and_then(|n| u32::try_from(n).ok())
        };
        let Some(path) = header
            .as_ref()
            .and_then(|h| h.get("file_path")?.as_str().map(str::to_owned))
            .or_else(|| act.call.input.get("file_path")?.as_str().map(str::to_owned))
        else {
            return false;
        };
        let shown = match &result.custody {
            ResultCustody::Pointer { .. } => {
                let mut numbers =
                    result.content.lines().filter_map(crate::code::file_engine::gutter_line_number);
                numbers.next().map(|first| (first, numbers.last().unwrap_or(first))) // unwrap_or: a one-line head ends where it starts
            }
            _ => match (field_u32("start_line"), field_u32("end_line"), field_u32("lines_returned")) {
                (Some(start), Some(end), Some(returned)) if returned > 0 && end >= start => Some((start, end)),
                _ => None,
            },
        };
        let view = self.view_mut(ViewSubject::File { path }, "code/read", seq, scope);
        view.touched_seq = seq;
        if let Some(total) = field_u32("total_lines") {
            view.total_lines = Some(total);
        }
        if let Some(range) = shown {
            merge_range(&mut view.seen, range);
        }
        view.handle = result.custody.is_pointer().then(|| result.custody.handle().map(str::to_owned)).flatten();
        true
    }

    /// A `tool/output` line range over a saved output: the requested range, clamped to the
    /// output's size, when the page came back whole. A page of a stored FILE read only
    /// touches that file's view: its line numbers are the saved result's, which open with a
    /// header line, not the file's.
    fn note_output_page(&mut self, act: &Observation, seq: u64) -> bool {
        let header = crate::cognition::tool_executor::spill::result_scalars(&act.output.result.content);
        // The output she named, or, for a call by act number, the one the result echoes.
        let named = act.call.input.get("outputId").and_then(|v| v.as_str());
        let echoed = header.as_ref().and_then(|h| h.get("outputId")?.as_str());
        let Some(handle) = named.or(echoed) else {
            return false;
        };
        let Some(index) = self.views.iter().position(|v| {
            v.handle.as_deref() == Some(handle)
                || v.subject == ViewSubject::Output { handle: handle.to_owned() }
        }) else {
            return false;
        };
        let mut view = self.views.remove(index);
        view.touched_seq = seq;
        if act.completed_successfully() && matches!(view.subject, ViewSubject::Output { .. }) {
            let total = header.as_ref().and_then(|h| h.get("totalLines")?.as_u64()).and_then(|n| u32::try_from(n).ok());
            let truncated = header.as_ref().and_then(|h| h.get("truncated")?.as_bool()).unwrap_or(true); // unwrap_or: a page that does not say it came back whole is not counted as read
            let line = |key: &str| act.call.input.get(key)?.as_u64().and_then(|n| u32::try_from(n).ok());
            if total.is_some() {
                view.total_lines = total;
            }
            if let (Some(start), false) = (line("startLine"), truncated) {
                let end = line("endLine").unwrap_or(u32::MAX).min(total.unwrap_or(u32::MAX)); // unwrap_or: no endLine reads to the end of the output
                if end >= start {
                    merge_range(&mut view.seen, (start, end));
                }
            }
        }
        self.views.push(view);
        true
    }

    /// The view for `(scope, subject)`, moved to the most-recent end, or a new one.
    fn view_mut(&mut self, subject: ViewSubject, label: &str, seq: u64, scope: Option<&str>) -> &mut OpenView {
        let found = self
            .views
            .iter()
            .position(|v| v.subject == subject && v.scope.as_deref() == scope);
        let view = match found {
            Some(index) => self.views.remove(index),
            None => OpenView {
                subject,
                label: label.to_owned(),
                total_lines: None,
                seen: Vec::new(),
                handle: None,
                touched_seq: seq,
                scope: scope.map(str::to_owned),
                note: None,
            },
        };
        self.views.push(view);
        self.evict_over_kept();
        let last = self.views.len() - 1;
        &mut self.views[last]
    }

    fn evict_over_kept(&mut self) {
        if self.views.len() > OPEN_VIEWS_KEPT {
            let excess = self.views.len() - OPEN_VIEWS_KEPT;
            self.views.drain(..excess);
        }
    }

    /// Retain the intent preceding this receipt without applying a source-local budget.
    /// Visibility of this detail belongs to the shared context allocation.
    pub fn annotate(&mut self, seq: u64, label: &str) -> bool {
        let note = label.split_whitespace().collect::<Vec<_>>().join(" ");
        if note.is_empty() {
            return false;
        }
        let mut changed = false;
        for view in self.views.iter_mut().filter(|v| v.touched_seq == seq) {
            if view.note.as_deref() != Some(note.as_str()) {
                view.note = Some(note.clone());
                changed = true;
            }
        }
        changed
    }

    /// The banner her open views are listed under.
    pub const BANNER: &'static str = "[open views] Files and outputs I have open, and which lines of each I have read \
         (kept across turns and restarts; not a message from anyone). code/read with \
         start_line/end_line pages a file; tool/output with outputId and startLine/endLine \
         pages an output:";

    /// Each visible view's stable key, whole representation and resolvable cursor. Planning
    /// prose is detail, never part of the mandatory cursor floor (FOCUS-CONTRACT rules 1–3).
    pub fn lines(&self, scope: Option<&str>) -> Vec<ViewLine> {
        let mut visible: Vec<&OpenView> = self.views.iter().filter(|v| visible_from(v, scope)).collect();
        visible.sort_by(|a, b| a.subject.cmp(&b.subject));
        visible.into_iter().map(|view| {
            let pointer = render_cursor(view);
            let full = match &view.note {
                Some(note) => format!("{pointer}; intent before this read: \"{note}\""),
                None => pointer.clone(),
            };
            ViewLine { key: view_key(view), full, pointer, touched_seq: view.touched_seq }
        }).collect()
    }

    /// The keys of views her hands cannot see from `scope` (they stand in another checkout):
    /// her context view retires them, and offers them again when her hands return.
    pub fn out_of_scope(&self, scope: Option<&str>) -> Vec<String> {
        self.views.iter().filter(|v| !visible_from(v, scope)).map(view_key).collect()
    }

    #[cfg(test)]
    pub fn render(&self, scope: Option<&str>) -> Option<String> {
        let lines = self.lines(scope);
        if lines.is_empty() {
            return None;
        }
        let mut out = String::from(Self::BANNER);
        for line in lines {
            out.push_str("\n- ");
            out.push_str(&line.full);
        }
        Some(out)
    }
}

/// One open view as her view renders it: its key, its whole and pointer lines, and the act that
/// last read it (its cause).
pub struct ViewLine {
    pub key: String,
    pub full: String,
    pub pointer: String,
    pub touched_seq: u64,
}

fn visible_from(view: &OpenView, scope: Option<&str>) -> bool {
    view.scope.is_none() || view.scope.as_deref() == scope
}

fn view_key(view: &OpenView) -> String {
    // An unscoped view is visible from every checkout.
    let scope = match view.scope.as_deref() {
        Some(scope) => scope,
        None => "*",
    };
    match &view.subject {
        ViewSubject::File { path } => format!("{scope}/file/{path}"),
        ViewSubject::Output { handle } => format!("{scope}/output/{handle}"),
    }
}

fn render_cursor(view: &OpenView) -> String {
    let mut line = match &view.subject {
        ViewSubject::File { path } => path.clone(),
        ViewSubject::Output { handle } => format!("output `{handle}` (from {})", view.label),
    };
    if let Some(total) = view.total_lines {
        line.push_str(&format!(", {total} lines"));
    }
    line.push_str(": ");
    let ranges = |rs: &[(u32, u32)]| rs.iter().map(|(a, b)| format!("{a}-{b}")).collect::<Vec<_>>().join(", ");
    if view.seen.is_empty() {
        line.push_str("not yet read past what was first shown");
    } else {
        let unread = view.total_lines.map(|total| unread_ranges(&view.seen, total)).unwrap_or_default();
        line.push_str("read ");
        line.push_str(&ranges(&view.seen));
        match view.total_lines {
            Some(_) if unread.is_empty() => line.push_str(" (all of it)"),
            Some(_) => {
                line.push_str("; not yet read ");
                line.push_str(&ranges(&unread));
            }
            None => {}
        }
    }
    if let (ViewSubject::File { .. }, Some(handle)) = (&view.subject, &view.handle) {
        line.push_str(&format!(" (that whole read is saved as output `{handle}`)"));
    }
    line
}

/// The saved results these views page, which must stay resolvable while a view is open.
pub fn view_handles(views: &[OpenView]) -> impl Iterator<Item = &str> {
    views.iter().filter_map(|v| match &v.subject {
        ViewSubject::Output { handle } => Some(handle.as_str()),
        ViewSubject::File { .. } => v.handle.as_deref(),
    })
}

/// Merge one inclusive range into sorted, merged ranges (touching ranges join).
fn merge_range(seen: &mut Vec<(u32, u32)>, (a, b): (u32, u32)) {
    seen.push((a.min(b), a.max(b)));
    seen.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(seen.len());
    for (start, end) in seen.drain(..) {
        match merged.last_mut() {
            Some(last) if start <= last.1.saturating_add(1) => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    *seen = merged;
}

/// The ranges of `1..=total` not covered by the merged `seen`.
fn unread_ranges(seen: &[(u32, u32)], total: u32) -> Vec<(u32, u32)> {
    let mut gaps = Vec::new();
    let mut next = 1u32;
    for &(start, end) in seen {
        if start > next && next <= total {
            gaps.push((next, (start - 1).min(total)));
        }
        next = next.max(end.saturating_add(1));
    }
    if next <= total {
        gaps.push((next, total));
    }
    gaps
}
