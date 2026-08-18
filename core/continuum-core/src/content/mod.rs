//! Content that is too big to hand over stays where it is, and travels as a HANDLE.
//!
//! See [docs/architecture/CONTENT-TRAVELS-BY-HANDLE.md]. The rule: a consumer never
//! receives a cut-down copy of something, it receives a small honest header plus a
//! reference, and calls the reference for more.
//!
//! # Why a trait and not a function
//!
//! The alternative — a reducer that inspects content and shrinks it — cannot work, and
//! failed live on 2026-08-18: a citizen mid-SWE-bench was handed her own directory listing
//! as `bytes":22592},{"kind":"file"…`, cut mid-token, with nothing saying anything had been
//! removed. Cutting bytes breaks JSON; cutting entries breaks a diff; cutting lines shifts
//! the line numbers the next edit depends on. The distinction between those is not in the
//! bytes, it is in what the content MEANS, and only its producer knows that.
//!
//! So the producer implements [`ContentSource`] and nobody else decides anything. A caller
//! holds a reference and calls its method; it never asks what kind of content it has.
//! Polymorphism in place of inspection — the same `cv::Algorithm` shape the rest of this
//! codebase uses for search, vision and audio.
//!
//! # What this buys, none of it special-cased
//!
//! - **Nothing is ever malformed.** A source hands out its own content in its own units.
//! - **Content size decouples from context size.** A 40k-line file is fully available to a
//!   citizen on a 4k window; she pages. The window stops bounding what is TRUE.
//! - **Indivisible content refuses**, in its own words, naming the narrowing — because its
//!   own `fetch` knows it is indivisible. No central policy table.
//! - **The grid is free.** [`HandleRef`] already routes a call back to the machine that
//!   minted it, so a handle to a peer's content is the same interface as a local one.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::runtime::cell_shapes::HandleRef;

/// The owner module every content handle routes back through — the `owner` field of the
/// minted [`HandleRef`], and the command prefix that dereferences it.
pub const CONTENT_OWNER: &str = "content";

/// What a piece of content IS, small enough to always fit in a prompt.
///
/// This is what a consumer gets instead of the content. It must be sufficient to decide
/// whether to fetch more and how — so it names the extent in the source's OWN units
/// (lines, entries, bytes) rather than a byte count nobody can act on.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../../protocol/typescript/content/ContentHeader.ts")]
pub struct ContentHeader {
    /// What this is, in the producer's words: `"file"`, `"directory listing"`,
    /// `"test run output"`. Free text on purpose — a closed enum here would be a central
    /// list every new source has to be added to.
    pub kind: String,
    /// One line a reader can act on: `"18 files under astropy/io/fits"`.
    pub summary: String,
    /// How much there is, in the source's own units.
    pub extent: Extent,
    /// The exact call that fetches more. Stated by the SOURCE so a consumer never has to
    /// guess the parameter name — the difference between a usable refusal and a burnt turn.
    pub fetch_with: String,
}

/// How much content there is, counted the way its own source counts it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../../protocol/typescript/content/Extent.ts")]
pub enum Extent {
    /// Line-addressed (a file, a log). 1-based, inclusive — the convention every editor
    /// and every `code/edit` call already uses.
    Lines { total: usize },
    /// Entry-addressed (a listing, a board, search hits).
    Entries { total: usize },
    /// Not divisible at any granularity — a patch, an image, a computed answer. A `fetch`
    /// on this either returns the whole thing or refuses.
    Whole { bytes: usize },
}

/// A request for part of a source's content, in that source's units.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../../protocol/typescript/content/Span.ts")]
pub struct Span {
    /// First unit wanted, 1-based.
    pub from: usize,
    /// How many units. The source clamps to what it has and REPORTS the clamp — it never
    /// silently returns less than asked without saying so.
    pub count: usize,
}

/// Part of a source's content, plus where the reader is in it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../../protocol/typescript/content/Slice.ts")]
pub struct Slice {
    /// The content itself, whole in its own units — never cut mid-unit.
    pub body: String,
    /// What this slice covers, after clamping.
    pub covered: Span,
    /// The next span, when there is more. `None` means the reader has reached the end —
    /// which is how she knows she has seen everything, a thing a truncated copy can never
    /// tell her.
    pub next: Option<Span>,
}

/// Something that produced content and can still be asked about it.
///
/// Implemented BY the producer. Callers hold `Arc<dyn ContentSource>` and never learn
/// which implementation they have.
pub trait ContentSource: Send + Sync {
    /// Small, complete, always fits.
    fn header(&self) -> ContentHeader;

    /// Dereference. The source decides what a span MEANS for its own content, and refuses
    /// in its own words when its content cannot be divided.
    fn fetch(&self, span: Span) -> Result<Slice, String>;
}

/// Live content sources, keyed by the UUID inside their [`HandleRef`].
///
/// Process-global because a handle minted during one command must be dereferenceable by a
/// later, unrelated command — that is the whole point of a handle. Lifetime is the
/// producer's per the `HandleRef` contract; a dropped entry yields a typed "handle not
/// found" rather than a panic.
static REGISTRY: OnceLock<Mutex<HashMap<Uuid, Arc<dyn ContentSource>>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<Uuid, Arc<dyn ContentSource>>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Park a source and mint the handle that reaches it. The header is returned with the
/// handle because a consumer needs both in the same breath: what this is, and how to get
/// more of it.
pub fn publish(source: Arc<dyn ContentSource>) -> (HandleRef, ContentHeader) {
    let header = source.header();
    let id = Uuid::new_v4();
    registry().lock().expect("content registry").insert(id, source);
    (
        HandleRef::with_id(CONTENT_OWNER, id, "content::ContentSource"),
        header,
    )
}

/// Dereference a handle. `Err` when the producer has released it — the honest answer, and
/// the one the `HandleRef` contract specifies.
pub fn fetch(id: Uuid, span: Span) -> Result<Slice, String> {
    let source = {
        let reg = registry().lock().expect("content registry");
        reg.get(&id).cloned()
    };
    match source {
        Some(s) => s.fetch(span),
        None => Err(format!(
            "handle not found: {id} — its producer has released it. Re-run the call that \
             produced it to get a fresh handle."
        )),
    }
}

/// Release a source. Producers call this when their state goes away.
pub fn release(id: Uuid) -> bool {
    registry()
        .lock()
        .expect("content registry")
        .remove(&id)
        .is_some()
}

// ---------------------------------------------------------------------------
// Outlier A — line-addressed text.
// ---------------------------------------------------------------------------

/// A body of text addressed by LINE, with the line numbers preserved exactly.
///
/// The first of the two proving implementations (CLAUDE.md's outlier rule): coordinates
/// are load-bearing here, which is precisely what a generic cutter destroys. A slice from
/// line 400 reports that it starts at 400, so a `code/edit` built from it targets the
/// right place.
pub struct TextContent {
    kind: String,
    summary: String,
    lines: Vec<String>,
}

impl TextContent {
    pub fn new(kind: impl Into<String>, summary: impl Into<String>, body: &str) -> Self {
        Self {
            kind: kind.into(),
            summary: summary.into(),
            lines: body.lines().map(str::to_string).collect(),
        }
    }
}

impl ContentSource for TextContent {
    fn header(&self) -> ContentHeader {
        ContentHeader {
            kind: self.kind.clone(),
            summary: self.summary.clone(),
            extent: Extent::Lines {
                total: self.lines.len(),
            },
            fetch_with: "content/fetch(handle, from=<line>, count=<n>)".to_string(),
        }
    }

    fn fetch(&self, span: Span) -> Result<Slice, String> {
        let total = self.lines.len();
        if span.from == 0 || span.from > total {
            return Err(format!(
                "line {} is outside this content (1..{total}) — ask within range",
                span.from
            ));
        }
        let start = span.from - 1;
        let end = (start + span.count).min(total);
        let covered = Span {
            from: span.from,
            count: end - start,
        };
        Ok(Slice {
            body: self.lines[start..end].join("\n"),
            covered,
            next: (end < total).then_some(Span {
                from: end + 1,
                count: span.count,
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// Outlier B — entry-addressed listing. Maximally different from A: no coordinates,
// entries are independent, and each is already a complete thing.
// ---------------------------------------------------------------------------

/// A list of independently meaningful entries — a directory listing, search hits, a board.
///
/// The interface must fit this WITHOUT forcing, or it is the wrong interface. It does:
/// `Extent::Entries` counts entries, a span means entries, and a slice is whole entries.
/// The generic char-cutter this replaces produced `bytes":22592},{"kind":"file"` here.
pub struct ListingContent {
    kind: String,
    summary: String,
    entries: Vec<String>,
}

impl ListingContent {
    pub fn new(
        kind: impl Into<String>,
        summary: impl Into<String>,
        entries: Vec<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            summary: summary.into(),
            entries,
        }
    }
}

impl ContentSource for ListingContent {
    fn header(&self) -> ContentHeader {
        ContentHeader {
            kind: self.kind.clone(),
            summary: self.summary.clone(),
            extent: Extent::Entries {
                total: self.entries.len(),
            },
            fetch_with: "content/fetch(handle, from=<entry>, count=<n>)".to_string(),
        }
    }

    fn fetch(&self, span: Span) -> Result<Slice, String> {
        let total = self.entries.len();
        if span.from == 0 || span.from > total {
            return Err(format!(
                "entry {} is outside this listing (1..{total}) — ask within range",
                span.from
            ));
        }
        let start = span.from - 1;
        let end = (start + span.count).min(total);
        Ok(Slice {
            body: self.entries[start..end].join("\n"),
            covered: Span {
                from: span.from,
                count: end - start,
            },
            next: (end < total).then_some(Span {
                from: end + 1,
                count: span.count,
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// Outlier C — indivisible. Not a third shape so much as the REFUSAL the other two
// prove is expressible: a patch that would be corrupted by any partial read.
// ---------------------------------------------------------------------------

/// Content whose parts are not independently valid — a patch, a diff, an image.
///
/// `fetch` returns the whole thing or refuses, and the refusal is written by the producer
/// because the producer is the only party that knows WHY. This is the case that motivated
/// the whole design: half a code fix applies cleanly and does the wrong thing, so a
/// partial read is worse than no read.
pub struct WholeContent {
    kind: String,
    summary: String,
    body: String,
    narrow_with: String,
}

impl WholeContent {
    pub fn new(
        kind: impl Into<String>,
        summary: impl Into<String>,
        body: impl Into<String>,
        narrow_with: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            summary: summary.into(),
            body: body.into(),
            narrow_with: narrow_with.into(),
        }
    }
}

impl ContentSource for WholeContent {
    fn header(&self) -> ContentHeader {
        ContentHeader {
            kind: self.kind.clone(),
            summary: self.summary.clone(),
            extent: Extent::Whole {
                bytes: self.body.len(),
            },
            fetch_with: self.narrow_with.clone(),
        }
    }

    fn fetch(&self, span: Span) -> Result<Slice, String> {
        // from=1, count>=1 means "give me the whole thing" — the only division this
        // content admits.
        if span.from != 1 {
            return Err(format!(
                "this {} cannot be read in parts — a partial copy would look valid and be \
                 wrong. {}",
                self.kind, self.narrow_with
            ));
        }
        Ok(Slice {
            body: self.body.clone(),
            covered: Span { from: 1, count: 1 },
            next: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE bug this module exists for. An oversized listing must reach a
    // consumer as WHOLE entries with a true total, never as a byte fragment. The live
    // failure was `bytes":22592},{"kind":"file"` — an entry cut mid-key.
    #[test]
    fn a_listing_yields_whole_entries_and_a_true_total() {
        let entries: Vec<String> = (0..50).map(|i| format!("file_{i}.py  22592 bytes")).collect();
        let src = ListingContent::new("directory listing", "50 files under io/fits", entries);

        assert_eq!(src.header().extent, Extent::Entries { total: 50 });

        let slice = src.fetch(Span { from: 1, count: 6 }).expect("fetch");
        assert_eq!(slice.covered.count, 6);
        for line in slice.body.lines() {
            assert!(
                line.starts_with("file_") && line.ends_with("bytes"),
                "every entry must be WHOLE — a half-entry is the bug: {line:?}"
            );
        }
        assert_eq!(
            slice.next,
            Some(Span { from: 7, count: 6 }),
            "the reader must be told there is more AND exactly how to ask for it"
        );
    }

    // what this catches: line numbers drifting. This is what makes a generic cutter
    // dangerous rather than merely lossy — a slice that renumbers its lines produces an
    // edit that targets the wrong place with right-looking coordinates.
    #[test]
    fn a_text_slice_preserves_its_absolute_line_numbers() {
        let body: String = (1..=1000).map(|i| format!("line {i}\n")).collect();
        let src = TextContent::new("file", "sympify.py, 1000 lines", &body);

        let slice = src.fetch(Span { from: 270, count: 3 }).expect("fetch");
        assert_eq!(slice.covered.from, 270, "the slice reports where it STARTS");
        assert_eq!(
            slice.body, "line 270\nline 271\nline 272",
            "content at 270 is the content at 270, not renumbered from 1"
        );
    }

    // what this catches: the end of content being indistinguishable from a truncation.
    // `next: None` is how a reader knows she has seen everything — the one thing a cut-down
    // copy can never tell her, and the reason a citizen reasons about a fragment as if it
    // were whole.
    #[test]
    fn reaching_the_end_is_reported_as_the_end() {
        let src = ListingContent::new("listing", "3 files", vec!["a".into(), "b".into(), "c".into()]);
        let slice = src.fetch(Span { from: 1, count: 10 }).expect("fetch");
        assert_eq!(slice.covered.count, 3, "clamped to what exists");
        assert!(slice.next.is_none(), "and says there is no more");
    }

    // what this catches: an indivisible payload being silently divided. The producer — not
    // the substrate — refuses, and the refusal names the remedy so the turn is not burnt.
    #[test]
    fn indivisible_content_refuses_a_partial_read_in_its_own_words() {
        let src = WholeContent::new(
            "patch",
            "fix for sympy-18057, 42 lines",
            "--- a/x\n+++ b/x\n",
            "Apply it whole, or request a smaller change.",
        );
        let err = src.fetch(Span { from: 2, count: 1 }).expect_err("must refuse");
        assert!(err.contains("cannot be read in parts"), "{err}");
        assert!(err.contains("Apply it whole"), "names the remedy: {err}");
        // ...and asking for the whole thing works.
        assert!(src.fetch(Span { from: 1, count: 1 }).is_ok());
    }

    // what this catches: the registry round trip — publish, deref through the HandleRef's
    // UUID, release. This is the seam that makes content survive PAST the command that
    // produced it, which is the entire difference between a handle and a return value.
    #[test]
    fn a_published_source_is_reachable_by_its_handle_and_gone_after_release() {
        let (handle, header) = publish(Arc::new(ListingContent::new(
            "listing",
            "2 files",
            vec!["one".into(), "two".into()],
        )));
        assert_eq!(handle.owner, CONTENT_OWNER, "routes back to the content module");
        assert_eq!(header.extent, Extent::Entries { total: 2 });

        let id: uuid::Uuid = handle.id.into();
        let slice = fetch(id, Span { from: 1, count: 1 }).expect("reachable by handle");
        assert_eq!(slice.body, "one");

        assert!(release(id));
        let err = fetch(id, Span { from: 1, count: 1 }).expect_err("gone after release");
        assert!(
            err.contains("handle not found") && err.contains("Re-run"),
            "a released handle explains itself and names the recovery: {err}"
        );
    }
}
