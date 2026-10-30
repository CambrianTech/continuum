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
    eprintln!("ratchet                          base     head");
    for (name, before, after) in rows {
        let before = before.map_or_else(|| "-".to_owned(), |b| b.to_string()); // map_or_else: a ratchet the base run did not record prints as absent
        eprintln!("{name:<30} {before:>7} {after:>8}");
    }
}

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
