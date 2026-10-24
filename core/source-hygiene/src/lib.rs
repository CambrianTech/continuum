//! Crate-wide source hygiene: **rules that make a bad shape unrepresentable, not
//! merely discouraged.**
//!
//! # Why this module exists
//!
//! CLAUDE.md already carries hygiene laws in prose — "every unwrap requires an
//! inline justification", "implement the probe, never the monitor", "no new
//! hardcoded context constant". Prose stops nobody. Every one of those laws that
//! actually holds today holds because someone wrote a TEST that scans the source,
//! and every one that gets re-broken is one that didn't.
//!
//! There were already two such scanners in tree, each hand-rolled inside the module
//! it happened to guard: the de-hardcode guard in `cognition/context_budget.rs` and
//! the module-wiring audit in `runtime/registry.rs`. Two copies of "walk src/, strip
//! comments, match a predicate, report offenders" is the same duplication this
//! codebase forbids everywhere else — so the third rule got the SEAM instead of a
//! third copy.
//!
//! - [`SourceFile`] — one file, already split into its production and test halves.
//! - [`SourceRule`] — what a rule supplies, and all it supplies: a predicate over
//!   one file. The walking, the comment handling, the reporting are the scanner's.
//! - [`scan`] — walks `src/` once and applies every rule.
//!
//! The two existing scanners should migrate onto this seam (they predate it); doing
//! that is a follow-up, not a reason to hand-roll a fourth.

pub mod boundary_serialization;
pub mod continuum_home;
pub mod derived_bounds;
pub mod grab_bag;
pub mod head_cuts;
pub mod file_size;
pub mod identity_discipline;
pub mod layer_discipline;
pub mod module_cycles;
pub mod production_reachability;
pub mod ratchet;
pub mod tenant_neutrality;
pub mod test_mod_singularity;
pub mod subprocess_spawn;
pub mod unwrap_justification;
pub mod wall_clock;

use std::path::{Path, PathBuf};

/// One source file, pre-split so a rule never has to re-derive the split.
pub struct SourceFile {
    /// Path relative to `src/`, forward-slashed — the form a rule's allow-list and
    /// a failure message should both use.
    pub rel: String,
    /// The PRODUCTION half: everything before the file's first column-zero `#[cfg(test)]`.
    ///
    /// A test-only helper gated mid-file is scanned as production: this errs toward a
    /// little noise, never toward the silence an early cut produced (see [`production_half`]).
    pub production: String,
    /// The WHOLE file, test mods included.
    ///
    /// Carried alongside `production` rather than re-read on demand because a rule
    /// that compares one text shape against another must be able to get both from the
    /// same read. (The first cut of [`production_reachability`] re-read the file and
    /// fell back to `production` when the read failed — a fallback that silently
    /// swapped the shape and reintroduced the exact miscount it was written to fix.)
    pub raw: String,
}

impl SourceFile {
    /// The module a file's code belongs to, as the path that names it: `x` for `x.rs`,
    /// `x/mod.rs`, and for a test module moved out of it (`x/tests.rs`,
    /// `x/tests/mod.rs`, `x/tests/part_2.rs`, each marked `#![cfg(test)]`). A rule that
    /// asks "outside its own file" asks "outside its owning module": a test that moved
    /// to a sibling file to keep its owner readable is still that owner's own test.
    pub fn owning_module(&self) -> &str {
        let rel = self.rel.as_str();
        if self.is_moved_test() {
            let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
            return dir.strip_suffix("/tests").unwrap_or(dir); // unwrap_or: `x/tests.rs` is already at `x`
        }
        rel.strip_suffix("/mod.rs")
            .or_else(|| rel.strip_suffix(".rs"))
            .unwrap_or(rel) // unwrap_or: every scanned file ends in .rs
    }

    /// A whole file of test code: a test module moved out of its owner, which declares
    /// `#![cfg(test)]` before any item.
    pub fn is_moved_test(&self) -> bool {
        for line in self.raw.lines() {
            let code = split_code_and_comment(line).0.trim();
            if code.is_empty() {
                continue;
            }
            if !code.starts_with("#![") {
                return false;
            }
            if code.starts_with("#![cfg(test)]") {
                return true;
            }
        }
        false
    }

    /// Production lines paired with their 1-indexed line number in the whole file,
    /// so a violation can point at a real editor line.
    pub fn production_lines(&self) -> impl Iterator<Item = (usize, &str)> {
        self.production.lines().enumerate().map(|(i, l)| (i + 1, l))
    }

    /// Build a `SourceFile` from an inline snippet — fixture for a rule's own
    /// unit tests, so predicate regressions are pinned without touching disk.
    #[cfg(test)]
    pub fn for_test(rel: &str, production: &str) -> Self {
        Self {
            rel: rel.to_string(),
            production: production.to_string(),
            raw: production.to_string(),
        }
    }
}

/// One violation, in the shape a failure message can print directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub file: String,
    pub line: usize,
    /// The offending source line, trimmed — so the message shows the code, not just
    /// a coordinate.
    pub source: String,
}

/// What a hygiene rule supplies. Implement this; do not walk the tree yourself.
pub trait SourceRule {
    fn name(&self) -> &'static str;
    fn check(&self, file: &SourceFile) -> Vec<Violation>;
}

/// `src/` of this crate, resolved from the manifest so the scan is independent of
/// the process's working directory (a test's cwd is not the crate root under every
/// runner).
pub fn crate_src_root() -> PathBuf {
    // THE SCANNED CRATE, NOT THIS ONE. These rules used to live inside
    // `continuum-core`, where `CARGO_MANIFEST_DIR` and "the tree under audit" were
    // the same directory. They are not the same any more: this crate's manifest dir
    // is `core/source-hygiene`, and a scan rooted there would walk 2,173 lines of
    // scanner and report a clean bill of health on a codebase it never opened —
    // every ratchet silently passing at 0 findings. The audited tree is named
    // explicitly so that failure is impossible rather than merely unlikely.
    audited_crate_root().join("src")
}

/// `core/continuum-core` — the crate these rules audit, resolved from this crate's
/// own manifest so the answer never depends on the process's working directory.
fn audited_crate_root() -> PathBuf {
    // A ratchet is judged by measuring the base tree with the head's rules
    // (`ratchet.rs`): `SOURCE_HYGIENE_CORE` names that tree's `core` directory.
    if let Some(core) = std::env::var_os("SOURCE_HYGIENE_CORE") {
        return PathBuf::from(core).join(AUDITED_CRATE);
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // core/source-hygiene -> core
        .map(|core| core.join(AUDITED_CRATE))
        // unwrap_or_else: a manifest dir with no parent cannot happen under cargo,
        // and inventing a relative fallback would scan the wrong tree silently.
        .unwrap_or_else(|| PathBuf::from(AUDITED_CRATE))
}

/// The crate under audit. One name, one place — a second spelling of this is how a
/// rule ends up scanning a directory that does not exist and reporting zero.
const AUDITED_CRATE: &str = "continuum-core";

/// Crates split out of `continuum-core` (card a31492cb). A split must never move code out
/// of the gates' sight, so their files are audited too, with the crate name as a path
/// prefix (`continuum-base/time/mod.rs`): totals stay continuous across a cut and every
/// per-path rule stays unambiguous.
const SPLIT_CRATES: &[&str] = &["continuum-base"];

/// Crates cut out of `continuum-core` that keep core's module names: `continuum-kernel`'s
/// `runtime/`, `routing/`, `orm/`… are re-exported by core as `crate::runtime` and so on, so
/// their files are audited UNPREFIXED (`runtime/registry.rs`), exactly as before the cut.
/// Every allow-list, ring and count stays continuous; prefixing them would drop 78k lines out
/// of the ring table and the path allow-lists, and the counts would fall for no change.
/// The ONE list: continuum-core's tree-walking guards (`test_env::source_tree`) read it too,
/// so a new cut is one line here (card 03e2a1c8: a second copy silently dropped perception).
pub const MODULE_CRATES: &[&str] = &["continuum-kernel", "continuum-mind", "continuum-perception", "continuum-genome", "continuum-memory", "continuum-live", "continuum-hands", "continuum-engine"];

/// Every file the per-file and crate-wide rules audit: `continuum-core/src` with paths
/// relative to it, then each split crate's `src` with its name prefixed. A listed crate
/// whose `src` is missing fails loudly rather than auditing nothing.
pub fn audited_files() -> Vec<SourceFile> {
    let mut files = collect_files(&crate_src_root());
    let Some(core_dir) = audited_crate_root().parent().map(Path::to_path_buf) else {
        panic!("{AUDITED_CRATE} has no parent directory: the split crates cannot be found");
    };
    for name in SPLIT_CRATES {
        let src = core_dir.join(name).join("src");
        assert!(src.is_dir(), "split crate {name} has no src at {}: the gates would audit nothing", src.display());
        files.extend(collect_files(&src).into_iter().map(|f| SourceFile { rel: format!("{name}/{}", f.rel), ..f }));
    }
    for name in MODULE_CRATES {
        // A tree from before the cut has no such crate: its modules are still in
        // continuum-core/src and were collected above, so nothing goes unaudited.
        let src = core_dir.join(name).join("src");
        if src.is_dir() {
            files.extend(collect_files(&src).into_iter().filter(|f| f.rel != "lib.rs"));
        }
    }
    files
}

/// The repository root, found by ANCHOR rather than by counting directories up.
///
/// Every caller here used to say `../..` or `ancestors().nth(2)`, which was correct
/// only because the scanner happened to sit two levels down. That assumption survived
/// this crate's move by luck — `core/source-hygiene` is also two levels down — and
/// Cormac flagged it in review as the one path the empty-scan guard cannot see: a
/// rule whose root is wrong but EXISTS scans a real directory and finds nothing.
///
/// Walking up to the directory that actually holds the workspace manifest removes the
/// assumption instead of documenting it, so moving this crate anywhere cannot break it.
/// `None` when no ancestor carries the anchor, which callers turn into a loud failure
/// rather than a silent empty scan.
pub fn repo_root() -> Option<PathBuf> {
    let start = audited_crate_root();
    start
        .ancestors()
        .find(|dir| dir.join(WORKSPACE_ANCHOR).is_file() && dir.join("core").is_dir())
        .map(Path::to_path_buf)
}

/// What marks the repository root. The workspace manifest is the honest anchor: it is
/// the file that DEFINES the tree these rules audit.
const WORKSPACE_ANCHOR: &str = "Cargo.toml";

/// Every `src/` directory in the workspace — this crate's, its sibling crates', and
/// the apps'.
///
/// **Why a rule ever needs more than `crate_src_root`:** some hygiene questions are
/// not answerable from one crate. "Is this `pub` type referenced anywhere?" is the
/// motivating case — `continuum-core` is a LIBRARY, so a type with no in-crate caller
/// may be perfectly well wired from `apps/cli` or `continuum-mcp`. A guard that
/// reported those as dead would be wrong on its first run and deleted by its second.
///
/// So the asymmetry is deliberate and load-bearing: **violations are only ever raised
/// against `crate_src_root()`, while REFERENCES are searched across every root here.**
/// Widening the reference corpus can only ever REMOVE findings, never add one.
///
/// Resolved from the manifest (`core/continuum-core` → repo root two levels up) so the
/// scan does not depend on the process's working directory. Roots that do not exist
/// are skipped, so this stays correct if the layout changes under it.
pub fn workspace_src_roots() -> Vec<PathBuf> {
    // The audited crate's src, not this crate's — same correction as
    // `crate_src_root`: violations are only ever raised against that tree.
    let mut roots = vec![crate_src_root()];
    // Anchored, not counted: see `repo_root`. A missing anchor yields the audited
    // root alone rather than a wrong tree full of nothing.
    let Some(repo) = repo_root() else {
        return roots;
    };
    for group in ["core", "apps"] {
        let Ok(entries) = std::fs::read_dir(repo.join(group)) else {
            continue;
        };
        for entry in entries.flatten() {
            let src = entry.path().join("src");
            // The crate's own root is already first; skip the duplicate rather than
            // paying to read every file in it twice.
            if src.is_dir() && src != roots[0] {
                roots.push(src);
            }
        }
    }
    roots
}

/// The PRODUCTION half of a source file (see [`SourceFile::production`]): every line
/// not gated by a `#[cfg(test)]` that is CODE, not prose. Matching the raw string
/// would truncate at the first doc comment that merely MENTIONS test-gating, and module
/// headers in this crate do exactly that, including this one. Found by the guard's own
/// positive control: the naive form excluded nearly all of `source_hygiene/mod.rs` from
/// its own scan, silently. Match by nature, never by name.
///
/// A file that IS a test module, moved out of its parent to shrink it, says so with the
/// inner `#![cfg(test)]` on its first code line: the whole file is test code. Without
/// this, every `mod tests;` split read as all-production and charged its unwraps and
/// spawns to the ratchets (card 0af5d5ec, 2026-10-18).
///
/// A COLUMN-ZERO `#[cfg(test)]` gates one file-scope item: that item is test code and
/// nothing else is. An indented `#[cfg(test)]` (a test-only helper inside an `impl`) is
/// scanned as production, erring toward noise: cut there, `cognition/llm_deliberation_faculty.rs`
/// hid ~9,000 production lines from every ratchet (found 2026-10-20).
///
/// Only the gated ITEM is test code, never the rest of the file. The first cut stopped
/// at the first column-zero `#[cfg(test)]`, which is the end of production only when the
/// test mod is the file's last item. A split that declares `#[cfg(test)] mod tests;` with
/// the other modules near the top (llamacpp_adapter.rs, line 93 of 934), or a gated test
/// const above the code (generate_response.rs, line 75 of 644), hid everything below it:
/// a split read as 10 unwraps removed when none were (found on #5225, 2026-10-20).
/// Lines of a gated item are blanked, not dropped, so line numbers stay true.
/// The attributes that gate an item OUT of a production build: unit tests, and the fixtures a
/// crate exports to its dependents' tests (`test-fixtures` is never enabled in a release
/// build). Code under either is test code to these scans; the kernel cut moved its fixtures
/// from the first spelling to the second so core's tests can reach them across the crate.
const TEST_GATES: &[&str] = &["#[cfg(test)]", "#[cfg(any(test, feature = \"test-fixtures\"))]"];

pub fn production_half(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut gated = false;
    let mut i = 0;
    while i < lines.len() {
        let code = split_code_and_comment(lines[i]).0;
        if code.trim_start().starts_with("#![cfg(test)]") {
            gated = true;
            break; // the whole file from here is a test module moved out of its owner
        }
        let Some(gate) = TEST_GATES.iter().find(|gate| code.starts_with(**gate)) else {
            out.push(lines[i]);
            i += 1;
            continue;
        };
        // The gated item can begin on the attribute's own line. Keep the slice's
        // line count intact so blanking still preserves source line numbers.
        let mut item_lines = lines[i..].to_vec();
        item_lines[0] = &lines[i][gate.len()..];
        let end = i + gated_item_end(&item_lines, 0);
        out.extend(std::iter::repeat_n("", end + 1 - i));
        gated = true;
        i = end + 1;
    }
    if !gated {
        return src.to_string();
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n")
}

/// Byte offset after a block comment's closing delimiter, given its body (after `/*`).
/// Rust block comments nest; punctuation inside every nesting level is trivia.
fn block_comment_end(bytes: &[u8]) -> Option<usize> {
    let (mut depth, mut at) = (1usize, 0usize);
    while at < bytes.len() {
        if bytes[at..].starts_with(b"/*") {
            depth += 1;
            at += 2;
        } else if bytes[at..].starts_with(b"*/") {
            depth -= 1;
            at += 2;
            if depth == 0 { return Some(at); }
        } else {
            at += 1;
        }
    }
    None
}

/// Skip Rust whitespace and comments. Comments are trivia even
/// between `const` and `fn`; nested block comments are consumed as one comment.
fn skip_item_trivia(mut src: &str) -> &str {
    loop {
        src = src.trim_start();
        if let Some(comment) = src.strip_prefix("//") {
            match comment.find('\n') {
                Some(end) => src = &comment[end + 1..],
                None => return "",
            }
        } else if let Some(comment) = src.strip_prefix("/*") {
            let Some(end) = block_comment_end(comment.as_bytes()) else { return "" };
            src = &comment[end..];
        } else {
            break;
        }
    }
    src
}

fn significant_word(src: &str) -> (Option<&str>, &str) {
    let src = skip_item_trivia(src);
    let end = src.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_').count();
    if end == 0 { (None, src) } else { (Some(&src[..end]), &src[end..]) }
}

/// Index of the last line of the item starting at or after `from` (attributes, blank
/// lines and comments before it belong to it). The item ends on the line where its
/// braces close, or on its `;` at depth zero when it opens none (`mod tests;`, a
/// multi-line `static ... = ...;`). Read as one character stream, not line by line: a
/// test module's multi-line string fixtures (JSON, source snippets, raw strings) carry
/// braces that a per-line count read as the module's end, leaking the rest of the tests
/// into production. An item that never closes runs to the end of the file.
fn gated_item_end(lines: &[&str], from: usize) -> usize {
    let last = lines.len().saturating_sub(1);
    let Some(head) = (from..lines.len()).find(|&k| {
        let code = split_code_and_comment(lines[k]).0.trim();
        !code.is_empty() && !code.starts_with("#[")
    }) else {
        return last;
    };
    let text = lines[head..].join("\n");
    // Initializer blocks can be followed by operators, method calls, or more blocks.
    // A static/const ends at its outer semicolon, never at an initializer's `}`.
    let (mut keyword, mut rest) = significant_word(&text);
    if keyword == Some("pub") {
        rest = skip_item_trivia(rest);
        if let Some(restricted) = rest.strip_prefix('(') {
            rest = restricted;
            loop {
                rest = skip_item_trivia(rest);
                match rest.chars().next() {
                    Some(')') => {
                        rest = &rest[1..];
                        break;
                    }
                    Some(ch) => rest = &rest[ch.len_utf8()..],
                    None => return last,
                }
            }
        }
        (keyword, rest) = significant_word(rest);
    }
    let semicolon_terminated = match keyword {
        Some("static") => true,
        Some("const") => !matches!(significant_word(rest).0, Some("fn" | "unsafe" | "extern")),
        _ => false,
    };
    let b = text.as_bytes();
    let ident = |at: usize| b[at].is_ascii_alphanumeric() || b[at] == b'_';
    let (mut depth, mut line, mut i, mut opened) = (0usize, head, 0usize, false);
    let (mut parentheses, mut brackets) = (0usize, 0usize);
    while i < b.len() {
        match b[i] {
            b'\n' => line += 1,
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i + 1 < b.len() && b[i + 1] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let start = i + 2;
                let Some(length) = block_comment_end(&b[start..]) else { return last };
                line += b[start..start + length].iter().filter(|&&c| c == b'\n').count();
                i = start + length - 1;
            }
            // A raw string, r"…" / r#"…"# / br#"…"#: no escapes, ends at `"` + the same hashes.
            b'r' if (i == 0 || !ident(i - 1)) || (b[i - 1] == b'b' && (i < 2 || !ident(i - 2))) => {
                let hashes = b[i + 1..].iter().take_while(|&&c| c == b'#').count();
                if b.get(i + 1 + hashes) == Some(&b'"') {
                    i += 2 + hashes;
                    while i < b.len() && !(b[i] == b'"' && b[i + 1..].iter().take(hashes).all(|&c| c == b'#') && b.len() - i > hashes) {
                        line += usize::from(b[i] == b'\n');
                        i += 1;
                    }
                    i += hashes;
                }
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        // An escape, including the `\` + newline continuation: its newline still counts.
                        line += usize::from(b.get(i + 1) == Some(&b'\n'));
                        i += 2;
                        continue;
                    }
                    line += usize::from(b[i] == b'\n');
                    i += 1;
                }
            }
            // A char literal ('{', '\'', '\u{7d}') is skipped whole; a lifetime ('a, 'static)
            // has no closing quote right after its first character and is left alone.
            b'\'' => {
                if b.get(i + 1) == Some(&b'\\') {
                    i += 2;
                    while i < b.len() && b[i] != b'\'' {
                        i += 1;
                    }
                } else if b.get(i + 2) == Some(&b'\'') {
                    i += 2;
                }
            }
            b'{' => {
                depth += 1;
                opened = true;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && parentheses == 0 && brackets == 0
                    && opened && !semicolon_terminated
                {
                    return line;
                }
            }
            b'(' => parentheses += 1,
            b')' => parentheses = parentheses.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b';' if depth == 0 && parentheses == 0 && brackets == 0 => return line,
            _ => {}
        }
        i += 1;
    }
    last
}

/// Read every `.rs` file under `root`, pre-split into production and test halves.
///
/// Unreadable files are skipped rather than failing the scan: a hygiene guard must
/// not turn a transient IO error into a red build that says nothing about hygiene.
pub fn collect_files(root: &Path) -> Vec<SourceFile> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let production = production_half(&src);
            out.push(SourceFile {
                rel,
                production,
                raw: src,
            });
        }
    }
    out
}



/// Walk this crate's `src/` once and apply every per-file rule to every `.rs` file.
pub fn scan(rules: &[&dyn SourceRule]) -> Vec<Violation> {
    let files = audited_files();
    let mut out = Vec::new();
    for file in &files {
        for rule in rules {
            out.extend(rule.check(file));
        }
    }
    sort_violations(&mut out);
    out
}

/// A rule that needs to see the WHOLE crate at once, not one file at a time.
///
/// [`SourceRule`] covers the common case — a predicate over a single file — and every
/// rule that CAN be expressed that way should be, because per-file rules are cheap and
/// trivially testable. But some hygiene questions are irreducibly cross-file: "does
/// anything reference this?" cannot be answered from the file that declares it. That
/// is this trait, and the reason it is a separate one rather than a widened
/// `SourceRule`: a per-file rule that received the whole crate would invite every
/// future rule to reach for global state it does not need.
pub trait CrateRule {
    fn name(&self) -> &'static str;
    /// `files` are this crate's, the only source of violations. `corpus` is every
    /// production line in the workspace — the reference haystack, never a source of
    /// violations. See [`workspace_src_roots`] for why the two differ.
    fn check_crate(&self, files: &[SourceFile], corpus: &str) -> Vec<Violation>;
}

/// Apply crate-wide rules. Reads this crate once for violations and the workspace once
/// for references.
pub fn scan_crate(rules: &[&dyn CrateRule]) -> Vec<Violation> {
    let files = audited_files();
    // The reference corpus is RAW file text — production AND test halves.
    //
    // This asymmetry against `files` (production-only, the violation source) is
    // deliberate. A type referenced only from another file's `#[cfg(test)]` mod is
    // WIRED — to a test harness, on purpose; `GridSimulator` and `RecordingGridCapture`
    // are exactly that. Counting only production halves reported them as dead, and a
    // guard that cries wolf on legitimate harnesses is one that gets deleted. Asking
    // "referenced NOWHERE, not even by a test" is the sharper question and still
    // catches the motivating defect, whose only mentions are inside its own file.
    let mut corpus = String::new();
    for root in workspace_src_roots() {
        for text in raw_sources(&root) {
            corpus.push_str(&text);
            corpus.push('\n');
        }
    }
    let mut out = Vec::new();
    for rule in rules {
        out.extend(rule.check_crate(&files, &corpus));
    }
    sort_violations(&mut out);
    out
}

/// Every `.rs` file under `root`, unsplit — the whole text including test mods.
fn raw_sources(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    out.push(text);
                }
            }
        }
    }
    out
}

fn sort_violations(v: &mut [Violation]) {
    v.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
}

/// Split a source line into (code, same-line comment), respecting string literals so
/// a `//` inside `"http://…"` is not read as a comment.
///
/// Shared because every rule that asks "is this line justified / is this a real
/// occurrence" needs exactly this, and getting it subtly different per rule is how
/// two scanners disagree about the same file.
pub fn split_code_and_comment(line: &str) -> (&str, Option<&str>) {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut escaped = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if !in_str {
            if let Some(len) = char_literal_len(bytes, i) {
                i += len;
                continue;
            }
        }
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            return (&line[..i], Some(&line[i + 2..]));
        }
        i += 1;
    }
    (line, None)
}

/// The byte length of a char (or byte) literal starting at `i`, if one does: `'x'` (any
/// single byte, `'"'` and `'{'` included) or an escaped `'\x'` (`'\''`, `'\"'`). A lifetime
/// (`'a`) never has a closing `'` two bytes on, so it is never mistaken for one (Cormac,
/// review of #4678: `'"'` opened a string and hid every brace after it on the line).
fn char_literal_len(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'\'') {
        return None;
    }
    if bytes.get(i + 2) == Some(&b'\'') && bytes.get(i + 1) != Some(&b'\\') {
        return Some(3);
    }
    if bytes.get(i + 1) == Some(&b'\\') && bytes.get(i + 3) == Some(&b'\'') {
        return Some(4);
    }
    None
}

/// The net brace depth change of a line of CODE (comments already split off), counting
/// only braces outside string literals and outside the `'{'` / `'}'` char literals, by
/// the same string tracking as [`split_code_and_comment`]. A brace in `"}"` is text, not
/// a block (Astra, review of #4678). Line-based like its sibling, so a string literal
/// spanning lines or a raw string with an unbalanced brace is outside its reach.
pub fn brace_delta(code: &str) -> i64 {
    let bytes = code.as_bytes();
    let mut in_str = false;
    let mut escaped = false;
    let mut delta = 0i64;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if let Some(len) = char_literal_len(bytes, i) {
            i += len;
            continue;
        } else if c == b'{' {
            delta += 1;
        } else if c == b'}' {
            delta -= 1;
        }
        i += 1;
    }
    delta
}

#[cfg(test)]
mod tests {
    // what this catches (Astra, review of #4678): a brace inside a string or char
    // literal counted as a block, so `{ "}" }` read as closing the enclosing impl.
    #[test]
    fn braces_in_string_and_char_literals_are_not_blocks() {
        assert_eq!(super::brace_delta(r#"fn label() -> &str { "}" }"#), 0);
        assert_eq!(super::brace_delta(r#"let s = "{{";"#), 0);
        assert_eq!(super::brace_delta(r#"let s = "a \" }";"#), 0, "an escaped quote does not end the string");
        assert_eq!(super::brace_delta("if c == '{' { open += 1;"), 1);
        assert_eq!(super::brace_delta("impl Runner {"), 1);
        assert_eq!(super::brace_delta("}"), -1);
        // Cormac, review of #4678: a quote char or byte literal is not a string.
        assert_eq!(super::brace_delta("if c == '\"' {"), 1);
        assert_eq!(super::brace_delta("} else if c == b'\"' {"), 0);
        assert_eq!(super::brace_delta(r"} else if c == b'\'' {"), 0, "an escaped quote char");
        assert_eq!(super::brace_delta("fn f<'a>(x: &'a str) {"), 1, "lifetimes are not char literals");
        // The same rule keeps the comment splitter right after a quote char literal.
        assert_eq!(super::split_code_and_comment("if c == '\"' { // note").1, Some(" note"));
    }

    // what this catches: THE EXTRACTION'S OWN FAILURE MODE, and it is the quiet one.
    // These rules used to live inside the crate they audit, so `CARGO_MANIFEST_DIR`
    // and "the tree under audit" were the same directory and could not disagree.
    // They are different directories now. A `crate_src_root()` that resolves to this
    // crate — or to anything that does not exist — makes every rule walk an empty or
    // wrong tree, find nothing, and report GREEN. Forever. No rule fails, no count
    // rises, and the ratchets look healthier than they have ever been while guarding
    // nothing at all. Cormac named this exact risk when reviewing the move.
    //
    // So the root is asserted to be real, to be the audited crate, and to contain
    // enough Rust that an empty scan cannot be mistaken for a clean one.
    // what this catches: the depth assumption that survived this crate's move by LUCK.
    // Every root here used to be `../..` or `ancestors().nth(2)`, correct only because
    // the scanner sat two levels down — and `core/source-hygiene` happens to sit two
    // levels down too, so the move could not expose it (Cormac, review of #4346). It is
    // also the one path `the_audited_root_...` cannot protect: a root that is WRONG but
    // EXISTS scans a real directory and finds nothing. Anchored resolution has to be
    // asserted directly or the next crate move reintroduces the bug silently.
    #[test]
    fn the_repo_root_is_found_by_anchor_and_is_above_this_crate() {
        let root = super::repo_root().expect(
            "no ancestor carries the workspace Cargo.toml — every cross-crate rule \
             would fall back to the audited root alone",
        );
        assert!(
            root.join("Cargo.toml").is_file() && root.join("core").is_dir(),
            "{} does not look like the repo root",
            root.display()
        );
        // The audited crate must live UNDER it — the relationship the old `../..`
        // asserted by arithmetic, now asserted by fact.
        assert!(
            super::crate_src_root().starts_with(&root),
            "audited root {} is not under repo root {}",
            super::crate_src_root().display(),
            root.display()
        );
        // And it must be the workspace manifest, not some nested crate's Cargo.toml
        // that happens to sit beside a `core/` directory.
        let manifest = std::fs::read_to_string(root.join("Cargo.toml"))
            // unwrap_or_default: an unreadable manifest fails the assert below rather
            // than passing on an absent string.
            .unwrap_or_default();
        assert!(
            manifest.contains("[workspace]"),
            "{} is not the WORKSPACE manifest",
            root.display()
        );
    }

    #[test]
    fn the_audited_root_is_the_other_crate_and_actually_has_source_in_it() {
        let root = super::crate_src_root();
        assert!(
            root.is_dir(),
            "the audited src root does not exist ({}) — every rule would scan nothing \
             and pass green",
            root.display()
        );
        assert!(
            root.ends_with("continuum-core/src"),
            "the audited root must be the AUDITED crate's src, not this scanner's ({})",
            root.display()
        );
        let rust_files = std::fs::read_dir(&root)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
                    .count()
            })
            // unwrap_or: an unreadable root is zero files, which the assert below
            // rejects — never a silently satisfied precondition.
            .unwrap_or(0);
        assert!(
            rust_files > 0,
            "the audited root {} contains no .rs files at its top level — a scan \
             rooted here reports zero findings because there is nothing to find",
            root.display()
        );
    }

    use super::*;

    /// What this catches: a `//` inside a string literal being read as the start of
    /// a comment — which would let `let u = fetch("http://x").unwrap();` count as
    /// "justified" because the URL's slashes look like a comment. Real shape: URLs
    /// and file paths appear on plenty of unwrap lines in this crate.
    #[test]
    fn a_double_slash_inside_a_string_is_not_a_comment() {
        let (code, comment) = split_code_and_comment(r#"let u = get("http://x").unwrap();"#);
        assert_eq!(comment, None, "the URL's // must not read as a comment");
        assert!(code.contains("unwrap"));

        let (code, comment) =
            split_code_and_comment(r#"let u = get("http://x").unwrap(); // JUSTIFIED: seeded"#);
        assert_eq!(comment, Some(" JUSTIFIED: seeded"));
        assert!(code.contains("unwrap"));

        // An escaped quote must not end the string early.
        let (_, comment) = split_code_and_comment(r#"let s = "a\"// b";"#);
        assert_eq!(comment, None);
    }

    // what this catches (card 0af5d5ec): a test module moved to its own file, marked
    // `#![cfg(test)]`, read as production, so its unwraps and spawns counted against the
    // ratchets and every "split the monolith by moving its tests" looked like growth.
    #[test]
    fn a_file_marked_inner_cfg_test_has_no_production_half() {
        let moved = "//! Tests for disk_eviction.\n#![cfg(test)]\nuse super::*;\nfn t() { x.unwrap(); }\n";
        assert_eq!(production_half(moved), "//! Tests for disk_eviction.");
        let inline = "fn real() {}\n#[cfg(test)]\nmod tests { fn t() { x.unwrap(); } }\n";
        assert_eq!(production_half(inline), "fn real() {}");
        let prose = "//! Mentions #![cfg(test)] in prose only.\nfn real() {}\n";
        assert_eq!(production_half(prose), prose, "a mention in a comment is not the marker");
    }

    // what this catches: the first cut's blind spot. A `mod tests;` declared with the other
    // modules near the top, or a gated test const above the code, hid every production line
    // below it (llamacpp_adapter.rs on #5225: 10 unwraps read as removed when none were).
    #[test]
    fn only_the_gated_item_is_test_code_never_the_rest_of_the_file() {
        let declared = "mod a;\n#[cfg(test)]\nmod tests;\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(declared), "mod a;\n\n\nfn real() { x.unwrap(); }");
        let konst = "#[cfg(test)]\n#[allow(dead_code)]\nstatic LOCK: Lazy<M> =\n    Lazy::new(|| M::new());\nfn real() { y.unwrap(); }\n";
        assert!(production_half(konst).ends_with("fn real() { y.unwrap(); }"));
        assert!(!production_half(konst).contains("LOCK"));
        let mid = "#[cfg(test)]\nmod tests {\n    fn t() { let s = \"}\"; let c = '}'; }\n}\nfn after() { z.unwrap(); }\n";
        let p = production_half(mid);
        assert!(p.contains("fn after()") && !p.contains("fn t()"), "{p:?}");
        assert_eq!(p.lines().count(), 5, "line numbers stay true");
        let helper = "fn real() {}\n#[cfg(test)]\nfn helper() {\n    q.unwrap();\n}\nfn more() {}\n";
        assert_eq!(production_half(helper), "fn real() {}\n\n\n\n\nfn more() {}");
        // Regression for #5227: a same-line test item must not consume its successor.
        let inline = "#[cfg(test)] mod tests {}\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(inline), "\nfn real() { x.unwrap(); }");
        let inline_decl = "#[cfg(test)] mod tests;\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(inline_decl), "\nfn real() { x.unwrap(); }");
        // A comment's semicolon cannot make the closed test module swallow production.
        let commented = "#[cfg(test)]\nmod tests {\n} // diagnostic; not syntax\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(commented), "\n\n\nfn real() { x.unwrap(); }");
        // Real constructor closing syntax still belongs to the gated static.
        let constructor = "#[cfg(test)]\nstatic X: LazyLock<()> = LazyLock::new(|| {\n});\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(constructor), "\n\n\nfn real() { x.unwrap(); }");
        // Regression for the first proposed fix: initializer blocks do not end a static.
        for initializer in [
            "{1} + { let y = 2;\n const EXTRA_LIMIT: usize = 9;\n y + EXTRA_LIMIT\n}",
            "{1usize}.wrapping_add({ let y = 2;\n const EXTRA_LIMIT: usize = 9;\n y + EXTRA_LIMIT\n})",
            "[ { let y = 2;\n const EXTRA_LIMIT: usize = 9;\n y + EXTRA_LIMIT\n} ][0]",
        ] {
            for declaration in [
                "static X", "const X", "pub(in crate::owner) const X",
                "pub\nconst X", "pub /* comment */ (crate) const X",
                "pub // visibility\n(in crate::owner) static X",
            ] {
                let raw = format!("#[cfg(test)]\n{declaration}: usize = {initializer};\nfn real() {{ x.unwrap(); }}\n");
                let projected = production_half(&raw);
                assert!(!projected.contains("EXTRA_LIMIT"), "{projected}");
                assert_eq!(projected.lines().last(), Some("fn real() { x.unwrap(); }"));
                assert_eq!(projected.lines().count(), raw.lines().count(), "line positions");
            }
        }
        // Const functions still end with their body, even when a comment has punctuation.
        let konst_fn = "#[cfg(test)]\nconst fn helper() -> usize { 1 } // ;\nfn real() { x.unwrap(); }\n";
        assert_eq!(production_half(konst_fn), "\n\nfn real() { x.unwrap(); }");
        // Regression for proposed revision 4: nested comment delimiters are never code.
        let nested = "#[cfg(test)]\nfn helper() { /* outer /* inner */ ( [ { ;\n tail ) ] } */ }\nfn production() { x.unwrap(); }\n";
        assert_eq!(production_half(nested), "\n\n\nfn production() { x.unwrap(); }");
        let open_delimiter = "#[cfg(test)]\nfn helper() { /* outer /* inner */ ( */ }\nfn production() { x.unwrap(); }\n";
        assert_eq!(production_half(open_delimiter), "\n\nfn production() { x.unwrap(); }");

        // Regression for proposed revision 2: comments are whitespace between keywords.
        for trivia in ["/* explanation */", "// explanation\n", "/* outer /* nested */ tail */"] {
            let raw = format!("#[cfg(test)]\nconst {trivia} fn helper() -> usize {{ 1 }}\nfn production() {{ x.unwrap(); }}\n");
            let projected = production_half(&raw);
            assert_eq!(projected.lines().last(), Some("fn production() { x.unwrap(); }"));
            assert!(!projected.contains("helper"));
            assert_eq!(projected.lines().count(), raw.lines().count());
        }



    }

    /// What this catches: the production/test split silently swallowing the whole
    /// file (or none of it), which would make every rule vacuously green or wildly
    /// noisy. The split is an approximation and this pins WHICH approximation.
    #[test]
    fn production_half_stops_at_the_first_cfg_test() {
        let root = crate_src_root();
        assert!(root.join("lib.rs").is_file(), "src root resolved wrong");

        struct Collect;
        impl SourceRule for Collect {
            fn name(&self) -> &'static str {
                "collect"
            }
            fn check(&self, file: &SourceFile) -> Vec<Violation> {
                // This module's own tests live below a `#[cfg(test)]`, so the
                // production half must NOT contain this very marker string.
                if file.rel == "source_hygiene/mod.rs" && file.production.contains("mod tests") {
                    return vec![Violation {
                        rule: "collect",
                        file: file.rel.clone(),
                        line: 0,
                        source: "test mod leaked into the production half".into(),
                    }];
                }
                Vec::new()
            }
        }
        assert!(scan(&[&Collect]).is_empty());
    }
}
pub mod vendor_pin;
