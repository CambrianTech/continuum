//! **SQL lives in the ORM and nowhere else. Zero, in production and in tests.**
//!
//! Joel: "SQL is forbidden"; "only ORM, never raw sqlite3, even for repair". Data goes through the
//! ORM / data layer: no `rusqlite` or `sqlx`, no query strings, no DDL anywhere else, tests
//! included, because a test that opens its own connection is a second schema owner too.
//!
//! The one owner is `continuum-kernel`'s `orm` module (`core/continuum-kernel/src/orm/`). Outside
//! it this rule finds:
//! - a `rusqlite` or `sqlx` identifier in code (a `use`, a path, an `extern crate`);
//! - a string literal shaped like a statement: `SELECT .. FROM`, `INSERT [OR ..] INTO`,
//!   `REPLACE INTO`, `UPDATE <name> SET`, `DELETE FROM`, `CREATE [UNIQUE|VIRTUAL] TABLE|INDEX`,
//!   `DROP TABLE|INDEX`, `ALTER TABLE`, `PRAGMA <name>` (upper case, as statements are written;
//!   prose that says "select from" is not a statement);
//! - a dependency on `rusqlite` or `sqlx` in any manifest but the kernel's and the workspace's
//!   own `[workspace.dependencies]` table.
//!
//! Every Rust file under `core/`, `apps/`, `client/` and `tools/` is read whole (vendored trees and
//! build output aside). It read 0 on 2026-11-07 (canary e14d6d603, after one test fixture's file
//! content stopped being a `CREATE TABLE`), so it is a hard zero, not a ratchet.

use std::path::{Path, PathBuf};

use super::lexer::tokens::{tokenize, Kind};
use super::Violation;

/// The module that owns SQL, as a repository path prefix.
pub const OWNER: &str = "core/continuum-kernel/src/orm/";

/// The manifests that may name the SQL crates: the owner's crate and the workspace's table.
const MANIFESTS_ALLOWED: &[&str] = &["core/continuum-kernel/Cargo.toml", "Cargo.toml"];

/// The crates that open a database connection by hand.
const SQL_CRATES: &[&str] = &["rusqlite", "sqlx"];

/// The directories whose Rust is read.
const ROOTS: &[&str] = &["core", "apps", "client", "tools"];

/// Directory names never descended into: vendored upstreams and build output.
const SKIPPED_DIRS: &[&str] = &["vendor", "target", "node_modules"];

/// Whether `text` (a literal's text) is shaped like an SQL statement.
pub fn is_statement(text: &str) -> bool {
    let words: Vec<&str> = text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).collect();
    let at = |i: usize| words.get(i).copied().unwrap_or(""); // unwrap_or: past the end is no word
    let is_name = |w: &str| !w.is_empty() && w.chars().any(|c| c.is_ascii_alphabetic());
    (0..words.len()).any(|i| match words[i] {
        "SELECT" => words[i + 1..].contains(&"FROM"),
        "INSERT" => at(i + 1) == "INTO" || (at(i + 1) == "OR" && at(i + 3) == "INTO"),
        "REPLACE" => at(i + 1) == "INTO",
        "DELETE" => at(i + 1) == "FROM",
        "UPDATE" => is_name(at(i + 1)) && at(i + 2) == "SET",
        "CREATE" => {
            let kind = if matches!(at(i + 1), "UNIQUE" | "VIRTUAL" | "TEMP" | "TEMPORARY") { at(i + 2) } else { at(i + 1) };
            matches!(kind, "TABLE" | "INDEX")
        }
        "DROP" => matches!(at(i + 1), "TABLE" | "INDEX"),
        "ALTER" => at(i + 1) == "TABLE",
        "PRAGMA" => is_name(at(i + 1)),
        _ => false,
    })
}

/// The SQL sites in one Rust file at repository path `repo`, test code included.
pub fn rust_sites(repo: &str, src: &str) -> Vec<Violation> {
    if repo.starts_with(OWNER) {
        return Vec::new();
    }
    let lines: Vec<&str> = src.lines().collect();
    let line_of = |n: usize| lines.get(n - 1).map_or("", |l| l.trim()).to_owned(); // map_or: a token's line is inside its file
    tokenize(src)
        .code
        .iter()
        .filter_map(|t| {
            let what = match t.kind {
                Kind::Ident if SQL_CRATES.contains(&t.text) => "a hand-opened SQL crate",
                Kind::Literal if is_statement(t.text) => "an SQL statement",
                _ => return None,
            };
            Some(Violation { rule: "sql_outside_the_orm", file: repo.to_owned(), line: t.line, source: format!("{what}: {}", line_of(t.line)) })
        })
        .collect()
}

/// The SQL-crate dependencies in one manifest at repository path `repo`.
pub fn manifest_sites(repo: &str, toml: &str) -> Vec<Violation> {
    if MANIFESTS_ALLOWED.contains(&repo) {
        return Vec::new();
    }
    toml.lines()
        .enumerate()
        .filter(|(_, line)| {
            let key = line.trim_start().split(['=', '.', ' ']).next().unwrap_or(""); // unwrap_or: split yields at least one piece
            SQL_CRATES.contains(&key.trim_matches('"'))
        })
        .map(|(i, line)| Violation { rule: "sql_outside_the_orm", file: repo.to_owned(), line: i + 1, source: format!("an SQL crate dependency: {}", line.trim()) })
        .collect()
}

/// Every SQL site outside the owner in the checkout at `root`.
pub fn scan_checkout(root: &Path) -> Vec<Violation> {
    let mut out = Vec::new();
    for top in ROOTS {
        let mut stack: Vec<PathBuf> = vec![root.join(top)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if path.is_dir() {
                    if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                        stack.push(path);
                    }
                    continue;
                }
                let Ok(rel) = path.strip_prefix(root) else { continue };
                let repo = rel.to_string_lossy().replace('\\', "/");
                let reader: fn(&str, &str) -> Vec<Violation> = if name.ends_with(".rs") {
                    rust_sites
                } else if name == "Cargo.toml" {
                    manifest_sites
                } else {
                    continue;
                };
                match std::fs::read_to_string(&path) {
                    Ok(text) => out.extend(reader(&repo, &text)),
                    Err(e) => panic!("cannot read {}: {e}; an unread file is an unaudited one", path.display()),
                }
            }
        }
    }
    if let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) {
        out.extend(manifest_sites("Cargo.toml", &text));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: SQL anywhere outside the ORM, production or test: a hand-opened
    // connection, a query or DDL string, a manifest that pulls a driver in. It is zero; it stays zero.
    #[test]
    fn sql_lives_only_in_the_orm() {
        let root = crate::repo_root().expect("the audited tree has a workspace root");
        assert!(root.join(OWNER).is_dir(), "the ORM owner {OWNER} is gone: move OWNER with it, or the rule spares nothing real");
        crate::ratchet::zero("sql_outside_the_orm", &scan_checkout(&root), || {
            "SQL is forbidden outside the ORM (core/continuum-kernel/src/orm/), tests included: make \
             the data an ORM entity behind its own trait and read it through the store."
                .to_string()
        });
    }

    // what this catches: the statement predicate going blind (negative control: an added
    // statement is a site) or flagging prose. Fixtures are assembled from pieces so no literal in
    // this file is a statement itself.
    #[test]
    fn statements_are_sites_and_prose_is_not() {
        let s = |parts: &[&str]| parts.join(" ");
        assert!(is_statement(&s(&["SELECT id", "FROM peers WHERE x = ?1"])));
        assert!(is_statement(&s(&["INSERT", "OR REPLACE", "INTO t VALUES (?1)"])));
        assert!(is_statement(&s(&["UPDATE grants", "SET n = 1"])));
        assert!(is_statement(&s(&["DELETE", "FROM t"])));
        assert!(is_statement(&s(&["CREATE", "UNIQUE INDEX i ON t(a)"])));
        assert!(is_statement(&s(&["ALTER", "TABLE t ADD c"])));
        assert!(is_statement(&s(&["PRAGMA", "journal_mode = WAL"])));
        assert!(!is_statement("select the newest page from the board"), "lower-case prose");
        assert!(!is_statement("DELETE the card"), "a verb, not a statement");
        assert!(!is_statement("UPDATE: the lane SET is live"), "a heading");
        let repo = "core/continuum-mind/src/x.rs";
        let added = format!("fn f() {{ db.execute(\"{}\", []); }}", s(&["DELETE", "FROM t"]));
        assert_eq!(rust_sites(repo, &added).len(), 1, "an added statement fails the zero");
        assert_eq!(rust_sites(repo, &format!("use {}::Connection;", SQL_CRATES[0])).len(), 1);
        assert_eq!(rust_sites(&format!("{OWNER}sqlite.rs"), &added).len(), 0, "the owner");
        assert!(rust_sites(repo, &format!("// {}", s(&["DELETE", "FROM t"]))).is_empty(), "a comment is not code");
    }

    // what this catches: a crate other than the kernel taking a database driver as a dependency.
    #[test]
    fn only_the_kernel_and_the_workspace_table_name_a_driver() {
        let dep = format!("{} = {{ workspace = true }}\n", SQL_CRATES[0]);
        assert_eq!(manifest_sites("core/continuum-mind/Cargo.toml", &dep).len(), 1);
        assert_eq!(manifest_sites("core/continuum-kernel/Cargo.toml", &dep).len(), 0);
        assert_eq!(manifest_sites("core/continuum-mind/Cargo.toml", "rusqlite-free = \"1\"\n").len(), 0, "a different crate");
    }
}
