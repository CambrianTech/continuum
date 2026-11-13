//! Run the portable ratchets against ANY repository, judged against its own base.
//!
//! `cargo run -p source-hygiene --bin repo_ratchets -- <repo-root> [<base-ref>]`
//!
//! The `ratchets` binary audits continuum's own layout (its workspace policy, its module
//! crates). The rules that know nothing of that layout (`store_reads::portable_rules`) belong to
//! every repository that holds a store, so this driver scans `<repo-root>/{crates,core,apps}/*/src`
//! and `<repo-root>/src`, counts each rule, and with a base ref measures the merge base of
//! `<base-ref>` and HEAD the same way (a detached worktree, removed after) and fails if any count
//! rose. airc's CI runs it: one owner of the rules, each repository's own baseline. It prints
//! every offender, not a sample: the list is the work queue.
//!
//! The crate is std-only, so it builds standalone outside continuum's workspace (airc's CI copies
//! `core/source-hygiene` and appends an empty `[workspace]` table).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use source_hygiene::{collect_files, store_reads, SourceFile, Violation};

/// The directories whose children are crates, in every layout this driver has met.
const CRATE_GROUPS: &[&str] = &["crates", "core", "apps"];

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let list_reads = args.iter().any(|a| a == READS_FLAG);
    args.retain(|a| a != READS_FLAG);
    let Some(root) = args.first() else {
        eprintln!("usage: repo_ratchets <repo-root> [<base-ref>] [{READS_FLAG}]");
        return ExitCode::FAILURE;
    };
    if list_reads {
        print_reads(Path::new(root));
    }
    match run(Path::new(root), args.get(1).map(String::as_str)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("repo_ratchets: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(root: &Path, base_ref: Option<&str>) -> Result<bool, String> {
    let head = measure(root)?;
    let base = match base_ref {
        Some(base_ref) => Some(measure_base(root, base_ref)?),
        None => None,
    };
    let mut held = true;
    for (rule, violations) in &head.by_rule {
        let before = base.as_ref().map(|b| b.by_rule.get(rule).map_or(0, Vec::len)); // map_or: a rule the base never tripped counts zero there
        let movement = before.map_or_else(|| "unjudged (no base ref)".to_owned(), |b| format!("{b} -> {}", violations.len())); // map_or_else: without a base the count is reported, not judged
        println!("{rule}: {movement}");
        for (owner, count) in by_crate(violations) {
            let was = base.as_ref().map_or_else(String::new, |b| format!("{} -> ", by_crate(b.by_rule.get(rule).map_or(&[][..], Vec::as_slice)).get(owner).copied().unwrap_or(0))); // unwrap_or: a crate absent from the base had none there
            println!("  {owner:<28} {was}{count}");
        }
        for v in violations {
            println!("    {}:{}  {}", v.file, v.line, v.source);
        }
        if before.is_some_and(|b| violations.len() > b) {
            println!(
                "{rule} rose. Start the read newest with a limit from an index and page back on demand; filter in the store \
                 on typed indexed columns. A read bounded by construction carries `// bounded: <why>` on its statement."
            );
            held = false;
        }
    }
    println!("bounded_exemptions: {} (each is a reviewed claim)", head.exemptions.len());
    for v in &head.exemptions {
        println!("    {}:{}  {}", v.file, v.line, v.source);
    }
    Ok(held)
}

/// List every store read the rules found, bounded or not, with what bounds it: the audit of the
/// rules themselves (a read judged bounded by the wrong evidence is visible here).
const READS_FLAG: &str = "--reads";

fn print_reads(root: &Path) {
    for file in files(root) {
        for read in store_reads::reads::reads(&file.production) {
            let bound = read.bound.as_deref().unwrap_or("NONE"); // unwrap_or: an unbounded read prints as such
            println!("read {}:{} {:?} entity={} bound={bound} store_filters={:?} rust_filter={:?}", file.repo, read.line, read.shape, read.entity, read.filters, read.filtered_in_rust);
        }
    }
}

struct Measured {
    by_rule: BTreeMap<&'static str, Vec<Violation>>,
    exemptions: Vec<Violation>,
}

/// Every portable rule over every crate's `src` under `root`. A root with no Rust fails loudly:
/// a wrong path must not read as a clean repository.
fn measure(root: &Path) -> Result<Measured, String> {
    let files = files(root);
    if files.is_empty() {
        return Err(format!("no Rust sources under {}/{{{}}}/*/src or src: nothing would be audited", root.display(), CRATE_GROUPS.join(",")));
    }
    let mut by_rule: BTreeMap<&'static str, Vec<Violation>> = BTreeMap::new();
    for rule in store_reads::portable_rules() {
        let entry = by_rule.entry(rule.name()).or_default();
        for file in &files {
            entry.extend(rule.check(file));
        }
        entry.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
    }
    let exemptions = files.iter().flat_map(store_reads::exemptions).collect();
    Ok(Measured { by_rule, exemptions })
}

fn files(root: &Path) -> Vec<SourceFile> {
    let mut out = Vec::new();
    for group in CRATE_GROUPS {
        let Ok(entries) = std::fs::read_dir(root.join(group)) else {
            continue;
        };
        let mut crates: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.join("src").is_dir()).collect();
        crates.sort();
        for krate in crates {
            let Some(name) = krate.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            out.extend(collect_files(root, &format!("{group}/{name}/src")));
        }
    }
    if root.join("src").is_dir() {
        out.extend(collect_files(root, "src"));
    }
    out.retain(|f| !is_test_module(&f.repo));
    out
}

/// A test module moved to its own file and gated by its parent's `#[cfg(test)] mod tests;`
/// carries no inner `#![cfg(test)]` the lexer could read, so its path decides: `tests.rs`,
/// `*_tests.rs`, or anything under a `tests/` directory.
fn is_test_module(repo: &str) -> bool {
    repo.ends_with("/tests.rs") || repo.ends_with("_tests.rs") || repo.contains("/tests/")
}

/// The merge base of `base_ref` and HEAD in `root`, checked out detached and measured.
fn measure_base(root: &Path, base_ref: &str) -> Result<Measured, String> {
    let base = git(root, &["merge-base", base_ref, "HEAD"])?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()); // map_or: a clock before the epoch only loses the name's uniqueness suffix
    let scratch = std::env::temp_dir().join(format!("repo-ratchets-base-{}-{nanos}", std::process::id()));
    let scratch_arg = scratch.to_string_lossy().into_owned();
    git(root, &["worktree", "add", "--detach", &scratch_arg, &base])?;
    eprintln!("repo_ratchets: base {base} ({base_ref}) at {scratch_arg}");
    let measured = measure(&scratch);
    let removed = git(root, &["worktree", "remove", "--force", &scratch_arg]);
    let measured = measured?;
    removed?;
    Ok(measured)
}

/// The crate a repository path belongs to: `<group>/<crate>/...`, or its first directory.
fn crate_of(file: &str) -> &str {
    let mut parts = file.split('/');
    match (parts.next(), parts.next()) {
        (Some(group), Some(owner)) if CRATE_GROUPS.contains(&group) => owner,
        (Some(first), _) => first,
        (None, _) => file,
    }
}

fn by_crate(violations: &[Violation]) -> BTreeMap<&str, usize> {
    let mut out = BTreeMap::new();
    for v in violations {
        *out.entry(crate_of(&v.file)).or_default() += 1;
    }
    out
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).output().map_err(|e| format!("cannot run git {args:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
