//! A read that names no range answers with its FIRST PAGE when the whole file is more than
//! the reader can take in one act, and its last line says how much more there is (plan S1;
//! replay #5488: Benchy 52 -> 39 s/request, Aris 72 -> 62, no clip of her newest result).
//!
//! This is the TOOL answering a smaller question, never a clip: the result is exactly what
//! the tool says it returns (lines 1-N, numbered, whole lines), and its last line names the
//! rest and the parameters that read it. A mind's view never cuts a result
//! (docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md, "one codec for every layer"); a
//! page is a different, smaller answer, and the header (`end_line`, `lines_returned`) says
//! which.
//!
//! The budget is not this crate's to choose. The reading mind's act layer fills it from her
//! view's measured per-act headroom (`View::appended_per_act`), shared equally by the act's
//! reads with no range. No budget (an external driver, or a view with no act measured yet)
//! reads whole, as before. Ballpark: the budget covers the numbered lines and the note; the
//! result's one header line is outside it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ReadResult;

/// The most characters one read with no range may answer with: the reader's share of her
/// view's measured per-act headroom. Never zero (a zero share is no budget: the read is whole).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PageBudget {
    chars: usize,
}

impl PageBudget {
    /// The `code/read` parameter that carries it (`CodeReadParams::page_budget`). Filled by
    /// the act layer on the call it dispatches, never offered to her in the tool's schema.
    pub const FIELD: &'static str = "page_budget";

    /// `None` for an empty share: nothing measured to page by.
    pub fn new(chars: usize) -> Option<Self> {
        (chars > 0).then_some(Self { chars })
    }

    pub fn chars(self) -> usize {
        self.chars
    }

    /// Whether a `code/read` input names a range (`start_line` or `end_line`): such a read is
    /// answered as asked, never paged.
    pub fn names_a_range(input: &Value) -> bool {
        ["start_line", "end_line"].iter().any(|key| input.get(key).is_some_and(|value| !value.is_null()))
    }

    /// Put this budget on a `code/read` input.
    pub fn stamp(self, input: &mut Value) {
        if let Some(object) = input.as_object_mut() {
            object.insert(Self::FIELD.to_owned(), Value::from(self.chars));
        }
    }

    /// The answer to a read: whole when it names a range or carries no budget, otherwise its
    /// first page when the whole is more than the budget.
    pub fn answer(read: ReadResult, start_line: Option<u32>, end_line: Option<u32>, budget: Option<Self>) -> ReadResult {
        match (start_line, end_line, budget) {
            (None, None, Some(budget)) => budget.first_page(read),
            _ => read,
        }
    }

    /// `read` (a whole read from line 1) as its first page: the most whole lines from line 1
    /// that fit with the note, at least one, followed by exactly one line
    /// `[lines 1-N of M; read more with start_line/end_line]`. Whole when it fits.
    fn first_page(self, mut read: ReadResult) -> ReadResult {
        let Some(content) = read.content.as_deref() else { return read };
        if read.start_line != 1 || content.chars().count() <= self.chars {
            return read;
        }
        let total = read.total_lines;
        let note = |shown: u32| format!("[lines 1-{shown} of {total}; read more with start_line/end_line]");
        let mut room = self.chars.saturating_sub(note(total).chars().count() + 1);
        let (mut page, mut end) = (0u32, 0usize);
        for line in content.split('\n') {
            let cost = line.chars().count() + 1;
            if page > 0 && cost > room {
                break;
            }
            room = room.saturating_sub(cost);
            page += 1;
            end += line.len() + 1;
        }
        if page >= read.lines_returned {
            return read;
        }
        // `end` is one past the newline that ends line `page`, which a later line follows.
        let body = &content[..end - 1];
        read.content = Some(format!("{body}\n{}", note(page)));
        read.end_line = page;
        read.lines_returned = page;
        read
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_of(lines: u32) -> ReadResult {
        let content = (1..=lines).map(|n| super::super::numbered_line(n, &format!("let line_{n} = {n};"))).collect::<Vec<_>>().join("\n");
        ReadResult {
            success: true,
            size_bytes: content.len() as u64,
            content: Some(content),
            file_path: "src/lib.rs".into(),
            total_lines: lines,
            lines_returned: lines,
            start_line: 1,
            end_line: lines,
            error: None,
        }
    }

    fn content(read: &ReadResult) -> &str {
        read.content.as_deref().expect("test: a read carries content")
    }

    // what this catches: a read that fits its budget coming back paged or changed.
    #[test]
    fn a_read_that_fits_is_whole() {
        let whole = read_of(10);
        let budget = PageBudget::new(content(&whole).chars().count());
        let answered = PageBudget::answer(whole.clone(), None, None, budget);
        assert_eq!(content(&answered), content(&whole));
        assert_eq!((answered.end_line, answered.lines_returned), (10, 10));
    }

    // what this catches: a page cut mid-line, a dropped or misworded note, or a header that
    // still claims the whole file (her open views would record lines she was never shown).
    #[test]
    fn an_oversize_read_pages_at_whole_lines_with_the_note() {
        let whole = read_of(200);
        let line = super::super::numbered_line(1, "let line_1 = 1;").chars().count() + 1;
        let budget = PageBudget::new(line * 20);
        let paged = PageBudget::answer(whole.clone(), None, None, budget);
        let text = content(&paged);
        let mut lines: Vec<&str> = text.lines().collect();
        let note = lines.pop().expect("test: a page has a note");
        let shown = paged.lines_returned;
        assert!(shown > 0 && shown < 200, "{shown}");
        assert_eq!(note, format!("[lines 1-{shown} of 200; read more with start_line/end_line]"));
        assert_eq!(lines.len() as u32, shown);
        let whole_lines: Vec<&str> = content(&whole).lines().take(shown as usize).collect();
        assert_eq!(lines, whole_lines, "the page is the file's first lines, each whole");
        assert!(text.chars().count() <= line * 20, "the page and its note fit the budget");
        assert_eq!((paged.start_line, paged.end_line, paged.total_lines), (1, shown, 200));
    }

    // what this catches: an explicit range being paged, or a read with no budget (no headroom
    // measured yet, or an external driver) answering with anything but the whole file.
    #[test]
    fn a_range_or_no_budget_reads_as_asked() {
        let whole = read_of(200);
        let tiny = PageBudget::new(64);
        for (start, end) in [(Some(1), None), (None, Some(200)), (Some(1), Some(200))] {
            assert_eq!(content(&PageBudget::answer(whole.clone(), start, end, tiny)), content(&whole));
        }
        assert_eq!(content(&PageBudget::answer(whole.clone(), None, None, None)), content(&whole));
        assert_eq!(PageBudget::new(0), None);
    }

    // what this catches: the act layer's stamp and the tool's parameter drifting apart, so the
    // budget never reaches the read (every read whole again, silently).
    #[test]
    fn the_stamp_is_the_parameter_the_read_takes() {
        let mut input = serde_json::json!({"file_path": "src/lib.rs"});
        assert!(!PageBudget::names_a_range(&input));
        PageBudget::new(4_000).into_iter().for_each(|budget| budget.stamp(&mut input));
        let params: crate::modules::code_commands::CodeReadParams =
            serde_json::from_value(input).expect("test: a stamped read input is a code/read input");
        assert_eq!(params.page_budget, PageBudget::new(4_000));
        assert!(PageBudget::names_a_range(&serde_json::json!({"file_path": "a", "start_line": 3})));
    }
}
