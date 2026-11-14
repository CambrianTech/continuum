//! Judge every ratchet against the tree this one is based on.
//!
//! `cargo run -p source-hygiene --bin ratchets -- <base-ref>` (CI passes the PR's base branch or
//! the push's previous commit; locally `origin/canary`):
//! 1. checks the merge base of `<base-ref>` and HEAD out as a detached worktree;
//! 2. runs this crate's tests with the base tree as the audited core, recording every count;
//! 3. runs them again over this tree, judging each count against the base's (`ratchet::hold`).
//!
//! Both runs use this tree's rules, so a PR that changes a rule is judged by the rule it ships.
//! See `src/ratchet.rs` for why no count is written into the source.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let base_ref = std::env::args().nth(1).unwrap_or_else(|| "origin/canary".to_owned()); // unwrap_or_else: no argument means the integration branch
    match run(&base_ref) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("ratchets: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(base_ref: &str) -> Result<bool, String> {
    let repo = PathBuf::from(git(&["rev-parse", "--show-toplevel"])?);
    let base = git(&["merge-base", base_ref, "HEAD"])?;
    let scratch = std::env::temp_dir().join(format!("source-hygiene-base-{}", std::process::id()));
    let record = scratch.with_extension("counts");
    let head_record = scratch.with_extension("head-counts");
    let touched = scratch.with_extension("touched");
    std::fs::write(&touched, touched_files(&base)?).map_err(|e| format!("cannot write the touched-file list {}: {e}", touched.display()))?;
    git(&["worktree", "add", "--detach", &scratch.to_string_lossy(), &base])?;
    eprintln!("ratchets: base {base} ({base_ref}) at {}", scratch.display());

    // The base run records; its other tests may fail on the base tree (an uninitialized
    // submodule for vendor_pin), which is not this PR's to judge, so only its record counts.
    let _ = cargo_test(&repo, &[("SOURCE_HYGIENE_CORE", scratch.join("core").as_path()), ("SOURCE_HYGIENE_RECORD", record.as_path())]);
    let judged = cargo_test(
        &repo,
        &[("SOURCE_HYGIENE_BASE", record.as_path()), ("SOURCE_HYGIENE_RECORD", head_record.as_path()), ("SOURCE_HYGIENE_TOUCHED", touched.as_path())],
    );
    print_movement(&record, &head_record);

    let removed = git(&["worktree", "remove", "--force", &scratch.to_string_lossy()]);
    let _ = std::fs::remove_file(&record);
    let _ = std::fs::remove_file(&head_record);
    let _ = std::fs::remove_file(&touched);
    removed?;
    judged
}

/// The receipt: every ratchet's base and head count, the moved ones first.
/// The Rust files this change touches, one repository path per line: the working tree against
/// `base` (so a local run judges uncommitted work too) and new files git does not track yet.
fn touched_files(base: &str) -> Result<String, String> {
    let changed = git(&["diff", "--name-only", "--diff-filter=ACMR", base])?;
    let untracked = git(&["ls-files", "--others", "--exclude-standard"])?;
    Ok(changed.lines().chain(untracked.lines()).filter(|path| path.ends_with(".rs")).collect::<Vec<_>>().join("\n"))
}

fn print_movement(base: &Path, head: &Path) {
    let read = |p: &Path| std::fs::read_to_string(p).map(|t| source_hygiene::ratchet::parse(&t)).unwrap_or_default(); // unwrap_or_default: a run that recorded nothing prints nothing; the judged run already failed loudly
    let (base, head) = (read(base), read(head));
    let mut rows: Vec<_> = head.iter().map(|(name, &after)| (name, base.get(name).copied(), after)).collect();
    rows.sort_by_key(|(name, before, after)| (before == &Some(*after), (*name).clone()));
    let rows: Vec<_> = rows.into_iter().filter(|(name, _, _)| !name.contains(source_hygiene::ratchet::BY_CRATE)).collect();
    eprintln!("ratchet                          base     head");
    for (name, before, after) in rows {
        let before = before.map_or_else(|| "-".to_owned(), |b| b.to_string()); // map_or_else: a ratchet the base run did not record prints as absent
        eprintln!("{name:<30} {before:>7} {after:>8}");
    }
    print_by_crate(source_hygiene::ratchet::crate_movement(&base, &head));
}

/// Where each ratchet's debt lives: its largest crates, and any crate a change swept to zero.
fn print_by_crate(rows: Vec<source_hygiene::ratchet::CrateMovement>) {
    let mut by_rule: std::collections::BTreeMap<String, Vec<source_hygiene::ratchet::CrateMovement>> = std::collections::BTreeMap::new();
    for row in rows {
        by_rule.entry(row.ratchet.clone()).or_default().push(row);
    }
    if by_rule.is_empty() {
        return;
    }
    print_every_crate(&by_rule);
    eprintln!("by crate (moved first, then largest)  base     head");
    for (rule, mut crates) in by_rule {
        crates.sort_by_key(|row| (row.before == Some(row.after), std::cmp::Reverse(row.after)));
        eprintln!("{rule}");
        for row in crates.into_iter().take(LARGEST_CRATES_SHOWN) {
            let before = row.before.map_or_else(|| "-".to_owned(), |b| b.to_string()); // map_or_else: a crate the base run did not record prints as absent
            eprintln!("  {:<30} {before:>7} {:>8}", row.owner, row.after);
        }
    }
}

/// Every crate of every ratchet as `ratchet@crate base head`, folded in the CI log: the full
/// per-crate baseline a floor-up sweep reads (the table above names only the largest). Printed
/// only under GitHub Actions, where the group folds; a local run keeps its short receipt.
fn print_every_crate(by_rule: &std::collections::BTreeMap<String, Vec<source_hygiene::ratchet::CrateMovement>>) {
    if std::env::var_os("GITHUB_ACTIONS").is_none() {
        return;
    }
    println!("::group::every crate of every ratchet (ratchet@crate base head)");
    for (rule, crates) in by_rule {
        for row in crates {
            let before = row.before.map_or_else(|| "-".to_owned(), |b| b.to_string()); // map_or_else: a crate the base run did not record prints as absent
            println!("{rule}{}{} {before} {}", source_hygiene::ratchet::BY_CRATE, row.owner, row.after);
        }
    }
    println!("::endgroup::");
}

/// How many crates the receipt names per ratchet: the owners a sweep starts with, not a list.
const LARGEST_CRATES_SHOWN: usize = 6;

fn cargo_test(repo: &Path, env: &[(&str, &Path)]) -> Result<bool, String> {
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())); // unwrap_or_else: outside `cargo run`, cargo is on PATH
    cmd.current_dir(repo).args(["test", "-q", "-p", "source-hygiene", "--lib"]);
    for (key, value) in env {
        cmd.env(key, value);
    }
    let status = cmd.status().map_err(|e| format!("cannot run cargo test: {e}"))?;
    Ok(status.success())
}

fn git(args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").args(args).output().map_err(|e| format!("cannot run git {args:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
