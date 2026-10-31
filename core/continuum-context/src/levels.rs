//! Levels of detail for a layer's items, generated from the content's shape: a layer says what
//! its items ARE and what shape they have, and gets every level for free, the way a GPU builds
//! mipmaps from a texture instead of each texture shipping its own.
//!
//! Each level costs at most a quarter of the one above it (a mipmap level halves each axis),
//! so a summary is offered only when it is at most `full / LEVEL_RATIO`; otherwise the item is
//! its own best summary and the view chooses between whole and pointer. Ballpark by design: the
//! shapes are coarse rules that decide the same way a precise one would for the cases measured
//! (a 760-line read, a build log), and the bound is checked here, once, for every layer.
//!
//! Shapes:
//! - [`Shape::Code`]: a numbered read. Declarations (code starting at most one indent level in)
//!   stay at their line numbers and every elided run becomes one `… lines a-b` marker, so every
//!   line stays addressable. The line format belongs to its producer, which passes its parser.
//! - [`Shape::Output`]: a log or tool output: its head and its tail (errors live at both ends),
//!   with the count of lines between.
//! - [`Shape::List`]: entries one per line: the first entries and how many more.

/// How much smaller each level is than the one above it.
pub const LEVEL_RATIO: usize = 4;

/// The deepest indent, in spaces, a declaration's code may start at: an item, or a method one
/// level inside it, in Rust, TypeScript and Python alike.
const DECLARATION_INDENT: usize = 4;

/// A numbered line split into its number and its code, by the producer that owns the format.
pub type NumberedLine<'a> = &'a dyn Fn(&str) -> Option<(u32, &str)>;

/// What a layer's item is, which decides how its summary is built.
pub enum Shape<'a> {
    Code(NumberedLine<'a>),
    Output,
    List,
}

/// The item's summary level, or `None` when no summary of its shape costs at most a quarter of it.
pub fn summarize(full: &str, shape: Shape<'_>) -> Option<String> {
    // Characters, the unit the view counts every block in (`Block::chars`); bytes would weigh
    // non-ASCII text heavier than the view does (Cormac on #5422).
    let budget = full.chars().count() / LEVEL_RATIO;
    let summary = match shape {
        Shape::Code(split) => outline(full, split)?,
        Shape::Output => ends(full, budget)?,
        Shape::List => head(full, budget)?,
    };
    (summary.chars().count() <= budget).then_some(summary)
}

/// A code item's outline: its header lines, its declarations at their lines, the rest marked.
fn outline(full: &str, split: NumberedLine<'_>) -> Option<String> {
    let mut out = String::from("[outline: declarations at their line numbers; read a range for a body]\n");
    let mut numbered = false;
    let mut dropped: Option<(u32, u32)> = None;
    for line in full.lines() {
        let Some((number, code)) = split(line) else {
            if !numbered {
                out.push_str(line);
                out.push('\n');
            }
            continue;
        };
        numbered = true;
        let trimmed = code.trim();
        let declaration = !trimmed.is_empty()
            && indent(code) <= DECLARATION_INDENT
            && !trimmed.starts_with(['}', ')', ']', '*'])
            && !trimmed.starts_with("//")
            && !trimmed.starts_with("/*");
        if declaration {
            mark_dropped(&mut out, &mut dropped);
            out.push_str(line);
            out.push('\n');
        } else {
            dropped = Some(dropped.map_or((number, number), |(a, _)| (a, number))); // map_or: a run starts here
        }
    }
    mark_dropped(&mut out, &mut dropped);
    numbered.then_some(out)
}

/// A line's indent in columns, a tab counting as one indent level (four spaces), so tab-indented
/// code reads at the same depth as space-indented code (Cormac on #5422).
fn indent(code: &str) -> usize {
    code.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { DECLARATION_INDENT } else { 1 })
        .sum()
}

fn mark_dropped(out: &mut String, dropped: &mut Option<(u32, u32)>) {
    if let Some((a, b)) = dropped.take() {
        out.push_str(&format!("       … lines {a}-{b}\n"));
    }
}

/// An output's head and tail within `budget`, the lines between counted.
fn ends(full: &str, budget: usize) -> Option<String> {
    let lines: Vec<&str> = full.lines().collect();
    // The marker between the ends is part of the summary's cost: reserve it before the ends.
    let marker = |between: usize| format!("\n… {between} lines between (expand to read them)\n");
    let half = budget.saturating_sub(marker(lines.len()).chars().count()) / 2;
    let (mut head, mut head_chars) = (0, 0);
    while head < lines.len() && head_chars + lines[head].chars().count() < half {
        head_chars += lines[head].chars().count() + 1;
        head += 1;
    }
    let (mut tail, mut tail_chars) = (0, 0);
    while tail < lines.len() - head && tail_chars + lines[lines.len() - 1 - tail].chars().count() < half {
        tail_chars += lines[lines.len() - 1 - tail].chars().count() + 1;
        tail += 1;
    }
    let between = lines.len() - head - tail;
    if between == 0 || head + tail == 0 {
        return None;
    }
    let mut out = lines[..head].join("\n");
    out.push_str(&marker(between));
    out.push_str(&lines[lines.len() - tail..].join("\n"));
    Some(out)
}

/// A list's first entries within `budget`, and how many more.
fn head(full: &str, budget: usize) -> Option<String> {
    let lines: Vec<&str> = full.lines().collect();
    let (mut kept, mut chars) = (0, 0);
    while kept < lines.len() && chars + lines[kept].chars().count() < budget {
        chars += lines[kept].chars().count() + 1;
        kept += 1;
    }
    let more = lines.len() - kept;
    (kept > 0 && more > 0).then(|| format!("{}\n… and {more} more (expand to read them)", lines[..kept].join("\n")))
}

/// How much of an item the view renders: whole, its summary, its pointer, or inside its
/// source's one fold block with the rest of the oldest run. The view decides it; the epoch
/// records it on each block, so a render receipt says what she was actually shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Level {
    Full,
    Summary,
    Pointer,
    Folded,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(line: &str) -> Option<(u32, &str)> {
        let (number, code) = line.split_once(" | ")?;
        Some((number.trim().parse().ok()?, code))
    }

    fn numbered(source: &str) -> String {
        source.lines().enumerate().map(|(i, l)| format!("{:>6} | {l}\n", i + 1)).collect()
    }

    // what this catches: a large read with no level between whole and a pointer (Aiko, the M5,
    // 2026-10-25: one 51,827-char file read whole 9 times in 43 acts). Declarations keep their
    // lines and every elided run is marked, so nothing becomes unaddressable.
    #[test]
    fn code_keeps_declarations_at_their_lines_and_marks_the_rest() {
        let body = "        let x = 1;\n".repeat(60);
        let source = format!("pub struct A;\n\nimpl A {{\n    pub fn f(&self) {{\n{body}    }}\n}}\n");
        let read = format!("file_path: a.rs\n{}", numbered(&source));
        let Some(outline) = summarize(&read, Shape::Code(&split)) else { panic!("a long read has an outline") };
        assert!(outline.contains("     1 | pub struct A;") && outline.contains("     4 |     pub fn f(&self) {"));
        assert!(outline.contains("… lines 5-66"), "{outline}");
        assert!(outline.contains("file_path: a.rs") && outline.len() * LEVEL_RATIO <= read.len());
    }

    // what this catches: a build log collapsing to one line, hiding the error at its end.
    #[test]
    fn output_keeps_its_head_and_tail() {
        let log = format!("Compiling a\n{}error[E0308]: mismatched types\n", "   building step\n".repeat(200));
        let Some(summary) = summarize(&log, Shape::Output) else { panic!("a long log has ends") };
        assert!(summary.starts_with("Compiling a") && summary.ends_with("error[E0308]: mismatched types"));
        assert!(summary.contains("lines between"));
    }

    // what this catches: tab-indented code read as deeper than it is, so a method's signature was
    // dropped and the outline fell to the pointer (Cormac on #5422).
    #[test]
    fn tab_indented_declarations_are_kept() {
        let body = "\t\tlet x = 1;\n".repeat(60);
        let source = format!("impl A {{\n\tfn f(&self) {{\n{body}\t}}\n}}\n");
        let Some(outline) = summarize(&numbered(&source), Shape::Code(&split)) else { panic!("a long read has an outline") };
        assert!(outline.contains("     2 | \tfn f(&self) {"), "{outline}");
    }

    // what this catches: a level offered where it saves nothing (it would cost the view a version).
    #[test]
    fn a_short_item_is_its_own_summary() {
        assert_eq!(summarize("fn a() {}\n", Shape::Code(&split)), None);
        assert_eq!(summarize("one\ntwo\n", Shape::Output), None);
        assert_eq!(summarize("one\ntwo\n", Shape::List), None);
    }
}

