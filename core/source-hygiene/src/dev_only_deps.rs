//! A crate's production code may only name crates from its `[dependencies]`.
//!
//! A crate listed only under `[dev-dependencies]` is linked into the crate's test build
//! and nowhere else. PR CI builds each selected crate as `cargo test --lib --no-run`, which
//! is that test build, so production code naming a dev-only crate compiles green on the PR
//! and breaks every non-test build after merge: #5452 moved `hf` into continuum-academy,
//! core named it from production code, academy was only core's dev-dependency, and canary
//! stopped building until #5457 (card d4868960). A cold `cargo check` of each crate's
//! non-test lib would catch it at minutes per crate; reading the manifests and the
//! production halves catches the same class in milliseconds.
//!
//! BOUNDED, not a compiler (Codex on #5460): target-specific normal dependencies are unioned,
//! so a crate declared only under `[target.'cfg(unix)'.dependencies]` and named in code built
//! on Windows passes here; so do manifest or path forms this reader does not parse (renamed
//! tables, `r#` idents, macro-generated paths). The compiler check for PRs that change a
//! dependency table is card 58b6026f.

use std::collections::BTreeSet;
use std::path::Path;

use crate::{lexer::split_code_and_comment, collect_files, SourceFile, Violation};

/// The crates a manifest declares, by the name code uses for them (`-` becomes `_`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DeclaredCrates {
    /// `[dependencies]`, `[dependencies.x]` and every `[target.'…'.dependencies]`.
    pub normal: BTreeSet<String>,
    /// `[dev-dependencies]`, `[dev-dependencies.x]` and their target forms.
    pub dev: BTreeSet<String>,
}

impl DeclaredCrates {
    /// Read the dependency tables of a `Cargo.toml`. Build dependencies are neither: a build
    /// script is its own target, and what it may name is not this rule's question.
    pub fn parse(manifest: &str) -> Self {
        let mut declared = Self::default();
        let mut section: Option<bool> = None; // Some(true) = dev, Some(false) = normal
        for line in manifest.lines() {
            let line = line.split('#').next().unwrap_or("").trim(); // unwrap_or: split always yields one piece
            if let Some(header) = line.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
                let header = header.trim();
                // `[dependencies.foo]` / `[dev-dependencies.foo]`: the table names one crate.
                for (table, dev) in [("dependencies.", false), ("dev-dependencies.", true)] {
                    if let Some(name) = header.strip_prefix(table) {
                        declared.insert(name, dev);
                    }
                }
                let table = header.rsplit('.').next().unwrap_or(header); // unwrap_or: rsplit always yields one piece
                section = match table {
                    "dependencies" => Some(false),
                    "dev-dependencies" => Some(true),
                    _ => None,
                };
                continue;
            }
            let (Some(dev), Some((key, _))) = (section, line.split_once('=')) else {
                continue;
            };
            // `foo = "1"`, `foo = { … }` and `foo.workspace = true` all name `foo`.
            declared.insert(key.trim().split('.').next().unwrap_or(""), dev); // unwrap_or: split always yields one piece
        }
        declared
    }

    fn insert(&mut self, key: &str, dev: bool) {
        let name = key.trim().trim_matches('"').replace('-', "_");
        if name.is_empty() {
            return;
        }
        if dev { &mut self.dev } else { &mut self.normal }.insert(name);
    }

    /// The crates only the test build links.
    pub fn dev_only(&self) -> BTreeSet<&str> {
        self.dev.difference(&self.normal).map(String::as_str).collect()
    }
}

/// Whether a line of code names `krate` as a crate root: `krate::…`, `use krate;` or
/// `extern crate krate`. A module path that merely ends in the same word
/// (`crate::x::krate::`) is not the crate, so the name must not follow an identifier or `::`.
pub fn names_crate(code: &str, krate: &str) -> bool {
    let bytes = code.as_bytes();
    let mut from = 0;
    while let Some(at) = code[from..].find(krate) {
        let start = from + at;
        let end = start + krate.len();
        from = end;
        let ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        let before = start.checked_sub(1).map(|i| bytes[i]);
        // `::krate::` is an absolute path to the crate; `a::krate::` is a module of `a`.
        let absolute = before == Some(b':')
            && start >= 2
            && bytes[start - 2] == b':'
            && start.checked_sub(3).map(|i| bytes[i]).is_none_or(|b| !ident(b) && b != b'>');
        if before.is_some_and(|b| ident(b) || b == b':') && !absolute {
            continue;
        }
        let rest = &code[end..];
        if rest.starts_with("::") {
            return true;
        }
        let prefix = code[..start].trim_end();
        let after = rest.trim_start();
        let ends_item = after.starts_with(';') || after.starts_with("as ");
        if (prefix.ends_with("use") || prefix.ends_with("extern crate")) && (ends_item || rest.trim().is_empty()) {
            return true;
        }
    }
    false
}

/// Whether an attribute line gates its item to test builds: a `#[cfg(…)]` whose predicate
/// IMPLIES `test`, so no non-test build compiles the item. `test`, `all(test, …)` and
/// `any(test, feature = "test-fixtures")` do; `any(test, unix)` does not (a normal Unix build
/// compiles it), and neither does anything under `not(…)`. The fixtures feature counts as
/// test, as it does for the other scans: no release build enables it.
pub fn is_test_gate(code: &str) -> bool {
    test_gate_rest(code).is_some()
}

/// For a test-gate attribute line, whatever follows the attribute on the same line (the
/// gated item when it starts there, as in `#[cfg(test)] field: T,`); `None` otherwise.
fn test_gate_rest(code: &str) -> Option<&str> {
    let rest = code.trim().strip_prefix("#[cfg(")?;
    let mut depth = 1usize;
    let close = rest.char_indices().find_map(|(i, c)| {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        (depth == 0).then_some(i)
    })?;
    let after = rest[close + 1..].strip_prefix(']')?;
    implies_test(&rest[..close]).then_some(after.trim())
}

/// Whether a cfg predicate can only hold in a test build.
fn implies_test(predicate: &str) -> bool {
    let predicate = predicate.trim();
    if predicate == "test" || predicate.replace(' ', "") == "feature=\"test-fixtures\"" {
        return true;
    }
    let Some((op, inner)) = predicate.split_once('(') else {
        return false;
    };
    let Some(inner) = inner.strip_suffix(')') else {
        return false;
    };
    let args = split_top_level(inner);
    match op.trim() {
        "all" => args.iter().any(|a| implies_test(a)),
        "any" => !args.is_empty() && args.iter().all(|a| implies_test(a)),
        _ => false, // `not(…)` and anything unknown never imply test
    }
}

/// A cfg argument list split on its top-level commas.
fn split_top_level(list: &str) -> Vec<&str> {
    let (mut depth, mut start, mut out) = (0usize, 0usize, Vec::new());
    for (i, c) in list.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&list[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&list[start..]);
    out.into_iter().map(str::trim).filter(|a| !a.is_empty()).collect()
}

/// The module a test-gated `mod name;` declares, if this line is one.
fn declared_module(code: &str) -> Option<&str> {
    let mut rest = code.trim();
    if let Some(after) = rest.strip_prefix("pub") {
        rest = after.trim_start();
        if let Some(restricted) = rest.strip_prefix('(') {
            rest = restricted.split_once(')')?.1.trim_start();
        }
    }
    rest.strip_prefix("mod ")?.trim().strip_suffix(';').map(str::trim)
}

/// The directory a file's child modules live in, relative to `src/`: `x/` for `x.rs`,
/// `x/mod.rs`, `lib.rs` and `main.rs` at the top.
fn child_dir(rel: &str) -> String {
    let (dir, file) = rel.rsplit_once('/').map_or(("", rel), |(d, f)| (d, f));
    let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
    match file {
        "mod.rs" | "lib.rs" | "main.rs" => prefix,
        _ => format!("{prefix}{}/", file.trim_end_matches(".rs")),
    }
}

/// For one file, which lines are test-only code (by index), and which child modules it
/// declares behind a test gate (as paths relative to `src/`, without the extension).
fn test_regions(file: &SourceFile) -> (Vec<bool>, Vec<String>) {
    let lines: Vec<&str> = file.raw.lines().collect();
    let mut test_only = vec![file.is_moved_test(); lines.len()];
    let mut gated_children = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(same_line) = test_gate_rest(split_code_and_comment(lines[i]).0) else {
            i += 1;
            continue;
        };
        // The gated item starts after the attribute, on its own line or the next code line.
        let head = if same_line.is_empty() {
            let Some(head) = (i + 1..lines.len()).find(|&k| {
                let code = split_code_and_comment(lines[k]).0.trim();
                !code.is_empty() && !code.starts_with("#[")
            }) else {
                break;
            };
            head
        } else {
            i
        };
        let code = if head == i { same_line } else { split_code_and_comment(lines[head]).0.trim() };
        if let Some(name) = declared_module(code) {
            gated_children.push(format!("{}{name}", child_dir(&file.rel)));
        }
        // A gated field, struct-literal entry, statement or `mod x;` is its one line; a
        // gated block item runs to its close (read from the item, not the attribute).
        let end = if code.ends_with(',') || code.ends_with(';') {
            head
        } else {
            let mut item = lines[head..].to_vec();
            item[0] = code;
            head + crate::lexer::gated_item_end(&item, 0)
        };
        test_only[i..=end].iter_mut().for_each(|t| *t = true);
        i = end + 1;
    }
    (test_only, gated_children)
}

/// Production lines of one crate's sources that name one of its dev-only crates.
pub fn violations_in(crate_name: &str, declared: &DeclaredCrates, files: &[SourceFile]) -> Vec<Violation> {
    let dev_only = declared.dev_only();
    if dev_only.is_empty() {
        return Vec::new();
    }
    let regions: Vec<_> = files.iter().map(test_regions).collect();
    let gated_modules: Vec<&String> = regions.iter().flat_map(|(_, children)| children).collect();
    let in_gated_module = |rel: &str| {
        gated_modules.iter().any(|m| rel == format!("{m}.rs") || rel.starts_with(&format!("{m}/")))
    };
    let mut out = Vec::new();
    for (file, (test_only, _)) in files.iter().zip(&regions) {
        if in_gated_module(&file.rel) {
            continue;
        }
        for (index, line) in file.raw.lines().enumerate() {
            if test_only[index] {
                continue;
            }
            let code = split_code_and_comment(line).0;
            if let Some(krate) = dev_only.iter().find(|k| names_crate(code, k)) {
                out.push(Violation {
                    rule: "dev_only_deps",
                    file: format!("{crate_name}/src/{}", file.rel),
                    line: index + 1,
                    source: format!("{} (`{krate}` is only a dev-dependency)", line.trim()),
                });
            }
        }
    }
    out
}

/// Every workspace crate under `core/` and `apps/` with a manifest and a `src/`.
pub fn workspace_violations(repo: &Path) -> (usize, Vec<Violation>) {
    let mut scanned = 0;
    let mut out = Vec::new();
    for group in ["core", "apps"] {
        let Ok(entries) = std::fs::read_dir(repo.join(group)) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            let (Ok(manifest), true) = (std::fs::read_to_string(dir.join("Cargo.toml")), dir.join("src").is_dir()) else {
                continue;
            };
            scanned += 1;
            let name = entry.file_name().to_string_lossy().into_owned();
            out.extend(violations_in(&name, &DeclaredCrates::parse(&manifest), &collect_files(repo, &format!("{group}/{name}/src"))));
        }
    }
    (scanned, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: production code in any workspace crate naming a crate its manifest
    // only lists under [dev-dependencies] (#5452 -> #5457: canary's non-test build broke).
    // Fix it by moving the crate to [dependencies], or the code under #[cfg(test)].
    #[test]
    fn production_code_names_only_declared_dependencies() {
        let Some(repo) = crate::repo_root() else {
            panic!("no repository root above the audited crate: the scan would read nothing");
        };
        let (scanned, violations) = workspace_violations(&repo);
        assert!(scanned >= 10, "only {scanned} workspace crates found under {}: the scan is not reading the tree", repo.display());
        assert!(
            violations.is_empty(),
            "production code names a dev-only crate:\n{}",
            violations.iter().map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
        );
    }

    // what this catches: the manifest reader dropping a table form, so a dev-only crate
    // reads as declared (silence) or a normal one reads as dev-only (noise).
    #[test]
    fn the_manifest_reader_sorts_every_table_form() {
        let declared = DeclaredCrates::parse(
            "[package]\nname = \"x\"\n\
             [dependencies]\nserde = \"1\"\ncontinuum-base.workspace = true\n\
             [target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n\
             [dependencies.tokio]\nversion = \"1\"\n\
             [build-dependencies]\ncc = \"1\"\n\
             [dev-dependencies]\ntempfile = \"3\"\nserde = \"1\"  # also normal\n\
             continuum-academy = { path = \"../continuum-academy\" }\n\
             [dev-dependencies.proptest]\nversion = \"1\"\n",
        );
        assert_eq!(declared.dev_only(), BTreeSet::from(["continuum_academy", "proptest", "tempfile"]));
        assert!(declared.normal.contains("libc") && declared.normal.contains("tokio") && declared.normal.contains("continuum_base"));
        assert!(!declared.normal.contains("cc") && !declared.dev.contains("cc"), "build-dependencies are neither");
    }

    // what this catches: the crate-root match flagging a module path that ends in the same
    // word, or missing a `use` form.
    #[test]
    fn a_crate_root_is_told_from_a_module_path() {
        assert!(names_crate("pub use continuum_academy::hf;", "continuum_academy"));
        assert!(names_crate("let d = tempfile::tempdir()?;", "tempfile"));
        assert!(names_crate("use tempfile;", "tempfile"));
        assert!(names_crate("extern crate tempfile as tf;", "tempfile"));
        assert!(!names_crate("use crate::genome::tempfile::X;", "tempfile"));
        // regression (Codex on #5460): a leading `::` is an absolute path to the crate.
        assert!(names_crate("let d = ::tempfile::tempdir()?;", "tempfile"));
        assert!(names_crate("use ::tempfile::TempDir;", "tempfile"));
        assert!(!names_crate("let d = <T as X>::tempfile::y();", "tempfile"));
        assert!(!names_crate("let my_tempfile = 1;", "tempfile"));
        assert!(!names_crate("fn tempfile_path() {}", "tempfile"));
    }

    // what this catches: a gate that merely MENTIONS test read as test-only (Codex on #5460:
    // `any(test, unix)` compiles in every Unix build, so a dev-only crate under it breaks).
    #[test]
    fn only_a_predicate_that_implies_test_is_a_test_gate() {
        assert!(is_test_gate("#[cfg(test)]"));
        assert!(is_test_gate("    #[cfg(all(test, feature = \"stress-tests\"))]"));
        assert!(is_test_gate("#[cfg(any(test, feature = \"test-fixtures\"))]"));
        assert!(is_test_gate("#[cfg(all(unix, any(test, feature = \"test-fixtures\")))]"));
        assert!(!is_test_gate("#[cfg(any(test, unix))]"));
        assert!(!is_test_gate("#[cfg(not(test))]"));
        assert!(!is_test_gate("#[cfg(feature = \"testing\")]"));
        assert!(!is_test_gate("#[cfg(unix)]"));
        let declared = DeclaredCrates::parse("[dev-dependencies]\ntempfile = \"3\"\n");
        let file = SourceFile::for_test("a.rs", "#[cfg(any(test, unix))]\nfn f() { let _ = ::tempfile::tempdir(); }\n");
        assert_eq!(violations_in("c", &declared, &[file]).len(), 1, "a Unix build compiles f");
    }

    // what this catches: the #5452 shape itself, from manifest to line.
    #[test]
    fn a_dev_only_crate_in_production_code_is_reported_and_a_gated_one_is_not() {
        let declared = DeclaredCrates::parse("[dev-dependencies]\ncontinuum-academy = { path = \"x\" }\n");
        let files = [
            SourceFile::for_test("lib.rs", "pub use continuum_academy::hf;\n"),
            SourceFile::for_test("x/tests.rs", "#![cfg(test)]\nuse continuum_academy::hf;\n"),
            SourceFile::for_test("y.rs", "#[cfg(all(test, feature = \"stress-tests\"))]\nmod harness;\n#[cfg(test)]\nmod tests;\n"),
            SourceFile::for_test("y/harness.rs", "use continuum_academy::hf;\n"),
            SourceFile::for_test("y/tests.rs", "fn t() { continuum_academy::hf::x(); }\n"),
            SourceFile::for_test("w.rs", "struct W {\n    #[cfg(test)] dir: continuum_academy::Dir,\n}\n#[cfg(test)] fn g() {\n    continuum_academy::x();\n}\n"),
            SourceFile::for_test(
                "z.rs",
                "struct S {\n    a: u8,\n    #[cfg(test)]\n    dir: continuum_academy::Dir,\n}\n\
                 #[cfg(test)]\nfn helper() {\n    continuum_academy::hf::x();\n}\n\
                 fn real() { continuum_academy::hf::y(); }\n",
            ),
        ];
        let found = violations_in("continuum-core", &declared, &files);
        let at: Vec<_> = found.iter().map(|v| (v.file.as_str(), v.line)).collect();
        assert_eq!(at, [("continuum-core/src/lib.rs", 1), ("continuum-core/src/z.rs", 10)], "{found:?}");
    }
}
