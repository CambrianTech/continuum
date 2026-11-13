//! The crate's own source, read by the tests that guard its shape.
//!
//! Six tests walked `src/` with six private copies of the same walk and the same
//! comment strip (the module-wiring audit, the lived-turn caller count, the loose-id
//! scan, the GPU base check, the context-constant scan, the swe env root). When test
//! modules moved out of oversized files into sibling `tests.rs` files (#5224), two of
//! the copies read a moved test module as production: a second caller of
//! `record_lived_turn`, a "registration" of `PersonaServiceModule`. One reader gives
//! every guard the same answer to "which file is this code in, and is it production".
#![cfg(test)]

use std::path::{Path, PathBuf};

/// One `.rs` file under `src/`.
pub(crate) struct SourceFile {
    pub(crate) path: PathBuf,
    /// Relative to `src/`, forward-slashed: the form an allow-list and a failure
    /// message both use.
    pub(crate) rel: String,
    pub(crate) text: String,
}

impl SourceFile {
    /// A file whose inner attributes include `#![cfg(test)]` is test code in full:
    /// a test module that moved out of its owner's file.
    pub(crate) fn is_test_file(&self) -> bool {
        for line in self.text.lines() {
            let code = code_only(line).trim();
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

    /// The whole file, comments dropped.
    pub(crate) fn code(&self) -> String {
        self.text
            .lines()
            .map(code_only)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The production code, comments dropped, nothing at all in a test file. Only the
    /// `#[cfg(test)]`-gated items are removed, by source-hygiene's projection: production
    /// BELOW a mid-file test mod is still production (Codex on #5228; cutting at the first
    /// `#[cfg(test)]` hid it from the lived-turn caller guard).
    pub(crate) fn production(&self) -> String {
        if self.is_test_file() {
            return String::new();
        }
        source_hygiene::lexer::production_half(&self.text)
            .lines()
            .map(code_only)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The file whose module this code belongs to: itself, or for a moved test file
    /// (`x/tests.rs`, `x/tests/mod.rs`, `x/tests/part_2.rs`) the owner `x.rs` or
    /// `x/mod.rs`. "Outside its own file" means outside this.
    pub(crate) fn owner(&self) -> PathBuf {
        if !self.is_test_file() {
            return self.path.clone();
        }
        // unwrap_or_default: a walked file always has a parent.
        let mut dir = self
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        // `x/tests/mod.rs` and `x/tests/part_N.rs` sit one level deeper than `x/tests.rs`.
        if self.path.file_stem().is_some_and(|s| s != "tests") && dir.ends_with("tests") {
            dir.pop();
        }
        let sibling = dir.with_extension("rs");
        if sibling.is_file() {
            sibling
        } else {
            dir.join("mod.rs")
        }
    }
}

/// One line with its `//` comment dropped, so prose can neither stand in for code
/// nor trip a guard. Source-hygiene's splitter, which knows string literals: one
/// comment rule for the ratchets and these guards.
pub(crate) fn code_only(line: &str) -> &str {
    source_hygiene::lexer::split_code_and_comment(line).0
}

/// The crates cut out of continuum-core whose modules core re-exports under their old
/// paths (`crate::runtime`, `crate::inference`, `crate::code`…). Their sources keep those
/// relative paths here, so every guard that walks the tree still sees them. One owner:
/// source-hygiene's list, which the ratchets audit the same way (card 03e2a1c8).
use source_hygiene::module_crates;

/// Every `.rs` file under this crate's `src/` and each cut-crate `src/` (their own
/// `lib.rs` files are crate roots, not modules core has, so they are left out).
pub(crate) fn crate_sources() -> Vec<SourceFile> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<SourceFile>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    let rel = path
                        .strip_prefix(root)
                        .unwrap_or(&path) // unwrap_or: every walked path is under root
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push(SourceFile { path, rel, text });
                }
            }
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    walk(&manifest.join("src"), &manifest.join("src"), &mut out);
    let cut_crates = module_crates().unwrap_or_else(|e| panic!("cut crates cannot be derived: {e}")); // test harness: a loud stop beats an empty scan
    for cut in &cut_crates {
        let root = manifest.join("..").join(cut).join("src");
        let before = out.len();
        walk(&root, &root, &mut out);
        let walked = out.split_off(before);
        out.extend(walked.into_iter().filter(|f| f.rel != "lib.rs"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(rel: &str, text: &str) -> SourceFile {
        SourceFile {
            path: PathBuf::from("/nonexistent/src").join(rel),
            rel: rel.into(),
            text: text.into(),
        }
    }

    // what this catches: a test module moved to a sibling file read as production (the
    // #5224 regression), and an inline test module or comment read as production.
    #[test]
    fn a_moved_test_file_has_no_production_and_belongs_to_its_owner() {
        let moved = file(
            "a/b/tests.rs",
            "//! Tests for b.\n#![cfg(test)]\nuse super::*;\nfn t() { call(); }\n",
        );
        assert!(moved.is_test_file());
        assert_eq!(moved.production(), "");
        assert_eq!(moved.owner(), PathBuf::from("/nonexistent/src/a/b/mod.rs"));
        let part = file("a/b/tests/part_2.rs", "#![cfg(test)]\nfn t() {}\n");
        assert_eq!(part.owner(), PathBuf::from("/nonexistent/src/a/b/mod.rs"));
        let inline = file("a/c.rs", "fn real() { call(); } // call() in prose\n#[cfg(test)]\nmod tests { fn t() { other(); } }\n");
        assert!(!inline.is_test_file());
        assert_eq!(inline.production(), "fn real() { call(); } ");
        assert_eq!(inline.owner(), inline.path);
        let prose = file(
            "a/d.rs",
            "//! Mentions #![cfg(test)] in prose.\nfn real() {}\n",
        );
        assert!(!prose.is_test_file());
    }

    // what this catches: a walk that silently finds nothing, which would make every
    // guard built on it vacuously green.
    // what this catches: Codex's P2 on #5228. A mid-file test mod (or a gated helper)
    // ended "production" at its first `#[cfg(test)]`, so a second lived-turn caller below
    // it would pass the exactly-one-caller guard unseen.
    #[test]
    fn production_below_a_mid_file_test_mod_is_still_production() {
        let f = file(
            "a/d.rs",
            "fn above() {}\n#[cfg(test)]\nmod tests { fn t() { gated(); } }\nfn below() { record(); }\n",
        );
        let prod = f.production();
        assert!(prod.contains("fn below() { record(); }"), "production below the test mod: {prod:?}");
        assert!(!prod.contains("gated()"), "the gated item is not production: {prod:?}");
    }

    #[test]
    fn the_walk_sees_the_crate() {
        let files = crate_sources();
        assert!(files.len() > 100, "found only {} files", files.len());
        assert!(files.iter().any(|f| f.rel == "test_env/source_tree.rs"));
    }
}
