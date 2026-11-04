
//! Levels of detail for a layer's items, generated from the content's shape: a layer says what
//! its items ARE and what shape they have, and gets every level for free, the way a GPU builds
//! mipmaps from a texture instead of each texture shipping its own.
//!
//! Every size here is derived from the item itself, never a fixed ratio or indent:
//! - **A ladder from the item's own floor to its whole.** An item's least content-bearing
//!   rendering is its elision marker (`… N lines between`, `… and N more`): its floor. Its whole
//!   is its want. A budgeted level (`Output`, `List`) is the geometric mean of the two, so each
//!   level is the same RATIO between floor and whole, and summarizing a level again is the next
//!   rung, the ladder converging on the floor (`summarize` of a level answers `None` once no
//!   smaller rung carries content). The view's one allocation chooses which rung she sees.
//! - **A level is worth a version only when it is smaller than what it stands for**
//!   ([`is_a_level`]), the one rule every layer applies.
//! - **A code item's indent unit is its own**: the most common step between the leading
//!   whitespace of its lines (2 or 4 spaces, or a tab), never an assumed width.
//!
//! Shapes:
//! - [`Shape::Code`]: a numbered read. Declarations (code starting at most one indent level in)
//!   stay at their line numbers and every elided run becomes one `… lines a-b` marker, so every
//!   line stays addressable. The line format belongs to its producer, which passes its parser.
//! - [`Shape::Output`]: a log or tool output: its head and its tail (errors live at both ends),
//!   with the count of lines between.
//! - [`Shape::List`]: entries one per line: the first entries and how many more.

/// A numbered line split into its number and its code, by the producer that owns the format.
pub type NumberedLine<'a> = &'a dyn Fn(&str) -> Option<(u32, &str)>;

/// What a layer's item is, which decides how its summary is built.
pub enum Shape<'a> {
    Code(NumberedLine<'a>),
    Output,
    List,
}

/// Whether `level` is worth a version of what it stands for (`whole`): it is smaller. Characters,
/// the unit the view counts every block in (`Block::chars`); bytes would weigh non-ASCII text
/// heavier than the view does (Cormac on #5422).
pub fn is_a_level(level: &str, whole: &str) -> bool {
    level.chars().count() < whole.chars().count()
}

/// The rung between an item's floor (`floor_chars`, its elision marker) and its whole
/// (`whole_chars`): their geometric mean. Ballpark by design (f64), checked by the ladder test.
fn rung(floor_chars: usize, whole_chars: usize) -> usize {
    ((floor_chars as f64) * (whole_chars as f64)).sqrt() as usize
}

/// The item's summary level, or `None` when no smaller level of its shape carries content.
pub fn summarize(full: &str, shape: Shape<'_>) -> Option<String> {
    let whole = full.chars().count();
    let lines = full.lines().count();
    let summary = match shape {
        Shape::Code(split) => outline(full, split)?,
        Shape::Output => {
            let budget = rung(line_marker(lines).chars().count(), whole);
            let level = ends(full, budget)?;
            (level.chars().count() <= budget).then_some(level)?
        }
        Shape::List => {
            let budget = rung(more_marker(lines).chars().count(), whole);
            let level = head(full, budget)?;
            (level.chars().count() <= budget).then_some(level)?
        }
    };
    is_a_level(&summary, full).then_some(summary)
}

/// An output's elision marker: what stands for `between` lines.
fn line_marker(between: usize) -> String {
    format!("\n… {between} lines between (expand to read them)\n")
}

/// A list's elision marker: what stands for `more` entries.
fn more_marker(more: usize) -> String {
    format!("\n… and {more} more (expand to read them)")
}

/// A code item's outline: its header lines, its declarations at their lines, the rest marked.
fn outline(full: &str, split: NumberedLine<'_>) -> Option<String> {
    let unit = indent_unit(full.lines().filter_map(|line| split(line).map(|(_, code)| code)));
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
        // An item, or a method one level inside it, in Rust, TypeScript and Python alike.
        let declaration = !trimmed.is_empty()
            && depth(code, unit) <= 1
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

/// A line's leading spaces, and its leading tabs.
fn leading(code: &str) -> (usize, usize) {
    let tabs = code.chars().take_while(|c| *c == '\t').count();
    let spaces = code.chars().skip(tabs).take_while(|c| *c == ' ').count();
    (spaces, tabs)
}

/// The code's own indent unit in spaces: the most common positive step between the leading
/// spaces of consecutive non-blank lines (2 in a 2-space file, 4 in a 4-space file). `None` when
/// no line steps in by spaces (a tab-indented file, or flat code): depth is then read in tabs.
fn indent_unit<'a>(codes: impl Iterator<Item = &'a str>) -> Option<usize> {
    let mut steps: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    let mut previous: Option<usize> = None;
    for code in codes.filter(|code| !code.trim().is_empty()) {
        let (spaces, _) = leading(code);
        if let Some(step) = previous.and_then(|before| spaces.checked_sub(before)).filter(|step| *step > 0) {
            *steps.entry(step).or_insert(0) += 1;
        }
        previous = Some(spaces);
    }
    // The most common step; on a tie, the smaller (the finer unit).
    steps.into_iter().max_by(|(sa, ca), (sb, cb)| ca.cmp(cb).then(sb.cmp(sa))).map(|(step, _)| step)
}

/// A line's depth in the code's own indent levels: each leading tab is one level (Cormac on
/// #5422: tab-indented code read as deeper than it is dropped a method's signature), and its
/// leading spaces are levels of `unit`.
fn depth(code: &str, unit: Option<usize>) -> usize {
    let (spaces, tabs) = leading(code);
    tabs + unit.map_or(0, |unit| spaces / unit) // map_or: no space step in the file, so spaces carry no level
}

fn mark_dropped(out: &mut String, dropped: &mut Option<(u32, u32)>) {
    if let Some((a, b)) = dropped.take() {
        out.push_str(&format!("       … lines {a}-{b}\n"));
    }
}

/// An output's head and tail within `budget`: by whole lines, the lines between counted, else (an
/// output with no line short enough to keep, such as one compact JSON line) by characters.
fn ends(full: &str, budget: usize) -> Option<String> {
    line_ends(full, budget).or_else(|| char_ends(full, budget))
}

/// An output's head and tail characters within `budget`, the characters between counted: the
/// level of an output whose lines are no unit to keep.
fn char_ends(full: &str, budget: usize) -> Option<String> {
    let chars: Vec<char> = full.chars().collect();
    let marker = |between: usize| format!("\n… {between} chars between (expand to read them)\n");
    let half = budget.saturating_sub(marker(chars.len()).chars().count()) / 2;
    let between = chars.len().saturating_sub(2 * half);
    if half == 0 || between == 0 {
        return None;
    }
    let head: String = chars[..half].iter().collect();
    let tail: String = chars[chars.len() - half..].iter().collect();
    Some(format!("{head}{}{tail}", marker(between)))
}

/// An output's head and tail lines within `budget`, the lines between counted.
fn line_ends(full: &str, budget: usize) -> Option<String> {
    let lines: Vec<&str> = full.lines().collect();
    // The marker between the ends is part of the summary's cost: reserve it before the ends.
    let half = budget.saturating_sub(line_marker(lines.len()).chars().count()) / 2;
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
    out.push_str(&line_marker(between));
    out.push_str(&lines[lines.len() - tail..].join("\n"));
    Some(out)
}

/// A list's first entries within `budget` (its marker reserved), and how many more.
fn head(full: &str, budget: usize) -> Option<String> {
    let lines: Vec<&str> = full.lines().collect();
    let room = budget.saturating_sub(more_marker(lines.len()).chars().count());
    let (mut kept, mut chars) = (0, 0);
    while kept < lines.len() && chars + lines[kept].chars().count() < room {
        chars += lines[kept].chars().count() + 1;
        kept += 1;
    }
    let more = lines.len() - kept;
    (kept > 0 && more > 0).then(|| format!("{}{}", lines[..kept].join("\n"), more_marker(more)))
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
        assert!(outline.contains("file_path: a.rs") && is_a_level(&outline, &read));
    }

    // what this catches: a build log collapsing to one line, hiding the error at its end.
    #[test]
    fn output_keeps_its_head_and_tail() {
        let log = format!("Compiling a\n{}error[E0308]: mismatched types\n", "   building step\n".repeat(200));
        let Some(summary) = summarize(&log, Shape::Output) else { panic!("a long log has ends") };
        assert!(summary.starts_with("Compiling a") && summary.ends_with("error[E0308]: mismatched types"));
        assert!(summary.contains("lines between"));
    }

    // what this catches: an output with no line short enough to keep (one compact JSON line) having
    // no level at all, so her newest result could only stay whole over her window or fall to a bare
    // pointer (goal 7). Its ends by characters are its level, smaller than it.
    #[test]
    fn a_single_line_output_keeps_its_ends_by_characters() {
        let line = format!("{{\"start\":1,{}\"end\":2}}", "\"k\":\"v\",".repeat(2_000));
        let Some(summary) = summarize(&line, Shape::Output) else { panic!("a long line has ends") };
        assert!(summary.starts_with("{\"start\":1,") && summary.ends_with("\"end\":2}"), "{summary}");
        assert!(summary.contains("chars between") && is_a_level(&summary, &line));
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

    // what this catches: an assumed indent width. With a fixed 4, a 2-space file's statements two
    // levels in (4 spaces) read as declarations and the outline kept every body line; the file's
    // own unit (its most common step) keeps the method and drops its body, in 2-space, 4-space
    // and tab files alike.
    #[test]
    fn the_indent_unit_is_the_files_own() {
        for (unit, label) in [("  ", "2-space"), ("    ", "4-space"), ("\t", "tab")] {
            let body = format!("{unit}{unit}let x = 1;\n").repeat(60);
            let source = format!("impl A {{\n{unit}fn f(&self) {{\n{body}{unit}}}\n}}\n");
            let Some(outline) = summarize(&numbered(&source), Shape::Code(&split)) else { panic!("{label}: a long read has an outline") };
            assert!(outline.contains(&format!("     2 | {unit}fn f(&self) {{")), "{label}: the method is kept: {outline}");
            assert!(!outline.contains("let x = 1;") && outline.contains("… lines 3-64"), "{label}: its body is elided: {outline}");
        }
        assert_eq!(indent_unit(["a", "  b", "    c", "  d"].into_iter()), Some(2));
        assert_eq!(indent_unit(["a", "\tb", "\t\tc"].into_iter()), None, "tabs carry their own levels");
    }

    // what this catches: a fixed ratio between levels. Each rung is the geometric mean of the
    // item's floor (its elision marker) and the level above, so summarizing a level again walks a
    // ladder that strictly shrinks and ends at the floor: the last rung is the marker with at most
    // one kept piece at each end, and nothing smaller carries content.
    #[test]
    fn a_ladder_of_levels_reaches_the_floor() {
        let log = format!("BUILD START\n{}error[E0308]: mismatched types\n", "   compiling one crate of the workspace, step\n".repeat(9_000));
        let mut rungs = vec![log.clone()];
        while let Some(next) = summarize(rungs.last().map_or("", String::as_str), Shape::Output) { // map_or: the ladder starts at the whole
            let above = rungs.last().map_or(0, |r| r.chars().count()); // map_or: as above
            assert!(next.chars().count() < above, "each rung is smaller than the one above it");
            rungs.push(next);
            assert!(rungs.len() < 64, "the ladder terminates");
        }
        assert!(rungs.len() >= 3, "more than one rung between the whole and the floor: {}", rungs.len());
        let floor = rungs.last().map_or(0, |r| r.chars().count()); // map_or: rungs is never empty
        let marker = line_marker(log.lines().count()).chars().count().max(super::more_marker(0).chars().count());
        assert!(floor <= 4 * marker, "the last rung is about its marker: {floor} chars, marker {marker}");
        assert!(rungs[1].contains("BUILD START") && rungs[1].contains("error[E0308]"), "the first rung keeps both ends");
    }

    // what this catches: a level offered where it saves nothing (it would cost the view a version).
    #[test]
    fn a_short_item_is_its_own_summary() {
        assert_eq!(summarize("fn a() {}\n", Shape::Code(&split)), None);
        assert_eq!(summarize("one\ntwo\n", Shape::Output), None);
        assert_eq!(summarize("one\ntwo\n", Shape::List), None);
    }
}
