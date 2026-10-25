//! A numbered read's outline: the coarse level of a file she read, between the whole read and
//! a one-line pointer. When her window cannot keep a large read whole, its outline keeps what
//! the file IS (its items and methods, each at its line number) and where everything else
//! sits, so she reads the range she needs instead of the whole file again. A pointer alone
//! told her only THAT she had read it: Aiko on the M5 read one 760-line, 51,827-char file
//! whole 9 times in 43 acts (2026-10-25).
//!
//! Ballpark by design: a line is kept when its code starts at most one indent level in (four
//! spaces: an item, or a method inside an impl/class), which is the shape of a declaration in
//! Rust, TypeScript and Python alike, and is not a closing delimiter or a comment. A run of
//! dropped lines becomes one `… lines a-b` marker, so every line of the file stays addressable.
//! The outline is offered only when it costs at most half of the read: otherwise the read is
//! its own best summary.

use super::edit_checks::gutter_line_number;

/// The deepest indent, in spaces, a kept line's code may start at: an item, or a method one
/// level inside it.
const OUTLINE_INDENT: usize = 4;

/// The outline of a numbered `code/read` result, or `None` when it has no numbered lines or
/// the outline would not cost at most half of it.
pub fn outline(rendered: &str) -> Option<String> {
    let mut out = String::from("[outline: declarations with their line numbers; read a range (start_line/end_line) for a body]\n");
    let mut numbered = 0usize;
    let mut dropped: Option<(u32, u32)> = None;
    for line in rendered.lines() {
        let Some(number) = gutter_line_number(line) else {
            // The read's own header and any trailing notes: what the read was.
            if numbered == 0 {
                out.push_str(line);
                out.push('\n');
            }
            continue;
        };
        numbered += 1;
        let code = line.split_once(" | ").map_or("", |(_, code)| code); // map_or: an empty source line renders with no code
        let indent = code.len() - code.trim_start().len();
        let trimmed = code.trim();
        let declaration = !trimmed.is_empty()
            && indent <= OUTLINE_INDENT
            && !trimmed.starts_with(['}', ')', ']', '*'])
            && !trimmed.starts_with("//")
            && !trimmed.starts_with("/*");
        if declaration {
            flush(&mut out, &mut dropped);
            out.push_str(line);
            out.push('\n');
        } else {
            dropped = Some(dropped.map_or((number, number), |(a, _)| (a, number)));
            // map_or: a run starts here
        }
    }
    flush(&mut out, &mut dropped);
    (numbered > 0 && out.len() * 2 <= rendered.len()).then_some(out)
}

/// Close a run of dropped lines with its marker.
fn flush(out: &mut String, dropped: &mut Option<(u32, u32)>) {
    if let Some((a, b)) = dropped.take() {
        out.push_str(&format!("       … lines {a}-{b}\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(source: &str) -> String {
        source
            .lines()
            .enumerate()
            .map(|(i, l)| format!("{:>6} | {l}\n", i + 1))
            .collect()
    }

    // what this catches: a large read with no level between whole and a pointer. Its outline
    // keeps each declaration at its line and marks every elided run, so nothing is unaddressable.
    #[test]
    fn an_outline_keeps_declarations_at_their_lines_and_marks_the_rest() {
        let body = "        let x = 1;\n".repeat(40);
        let source =
            format!("pub struct A;\n\nimpl A {{\n    pub fn f(&self) {{\n{body}    }}\n}}\n");
        let read = format!("file_path: a.rs\n{}", numbered(&source));
        let Some(outline) = outline(&read) else {
            panic!("a long read has an outline")
        };
        assert!(outline.contains("     1 | pub struct A;"));
        assert!(outline.contains("     4 |     pub fn f(&self) {"));
        assert!(outline.contains("… lines 5-46"), "{outline}");
        assert!(outline.starts_with("[outline:") && outline.contains("file_path: a.rs"));
    }

    // what this catches: an outline offered where it saves nothing, or for output with no lines.
    #[test]
    fn a_short_read_or_unnumbered_output_has_no_outline() {
        assert_eq!(outline(&numbered("fn a() {}\nfn b() {}\n")), None);
        assert_eq!(outline("build failed: error[E0308]"), None);
    }
}
