//! Which crates are tested, audited and counted, derived from the workspace.
//!
//! One source: the root `Cargo.toml`. Every `core/` member is tested, audited by the per-file
//! ratchets and counted by the warnings ratchet, except the exceptions named under
//! `[workspace.metadata.ci]`. A crate cut out of `continuum-core` is added to `members` and
//! nothing else (card 85138cb9; review caught a missed list entry twice on 2026-10-26).
//! `tools/scripts/ci-change-scope.cjs` derives the CI side from the same file by the same rule;
//! [`crate::crate_wiring`] guards that no hand-kept list creeps back.

use std::collections::BTreeSet;
use std::path::Path;

/// The exceptions under `[workspace.metadata.ci]`, by package name.
#[derive(Debug, Default, Clone)]
pub struct Policy {
    /// Permanent exceptions (vendored, tool, no tests, own job).
    pub by_nature: BTreeSet<String>,
    /// Tests exist but no job selects them yet: debt that only shrinks.
    pub pending: BTreeSet<String>,
    /// `by_nature` and `pending` together: what the crate matrix leaves out.
    pub untested: BTreeSet<String>,
    pub own_lib_test_step: BTreeSet<String>,
    pub split_prefixed: BTreeSet<String>,
    pub unaudited: BTreeSet<String>,
    pub warnings_uncounted: BTreeSet<String>,
}

/// A workspace member under `core/`: its package name and directory name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub dir: String,
}

fn strip_comments(text: &str) -> String {
    text.lines().map(|line| line.split('#').next().unwrap_or("")).collect::<Vec<_>>().join("\n") // unwrap_or: split always yields one piece
}

/// The quoted strings of the first `key = [ ... ]` found in `body`.
fn array(body: &str, key: &str) -> Option<Vec<String>> {
    let mut from = 0;
    while let Some(at) = body[from..].find(key) {
        let start = from + at;
        let before_ok = body[..start].chars().next_back().map_or(true, |c| !(c.is_alphanumeric() || c == '_' || c == '-'));
        let rest = body[start + key.len()..].trim_start();
        if before_ok {
            if let Some(list) = rest.strip_prefix('=').map(str::trim_start).and_then(|r| r.strip_prefix('[')) {
                let list = &list[..list.find(']')?];
                return Some(list.split('"').skip(1).step_by(2).map(str::to_string).collect());
            }
        }
        from = start + key.len();
    }
    None
}

/// The text of one `[section]` of a manifest, comments stripped.
fn section(manifest: &str, name: &str) -> String {
    let mut inside = false;
    let mut out = String::new();
    for line in strip_comments(manifest).lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t.trim_matches(|c| c == '[' || c == ']').trim() == name;
        } else if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The policy from the root manifest text; an absent key is an error, so a rename cannot
/// turn every exception off silently.
pub fn parse_policy(root_manifest: &str) -> Result<Policy, String> {
    let body = section(root_manifest, "workspace.metadata.ci");
    let get = |key: &str| {
        array(&body, key)
            .map(|names| names.into_iter().collect::<BTreeSet<_>>())
            .ok_or_else(|| format!("no `{key}` under [workspace.metadata.ci] in the root Cargo.toml"))
    };
    let (by_nature, pending) = (get("by_nature")?, get("pending")?);
    Ok(Policy {
        untested: by_nature.union(&pending).cloned().collect(),
        by_nature,
        pending,
        own_lib_test_step: get("own_lib_test_step")?,
        split_prefixed: get("split_prefixed")?,
        unaudited: get("unaudited")?,
        warnings_uncounted: get("warnings_uncounted")?,
    })
}

/// Whether a member has a lib target (`src/lib.rs` or a `[lib]` section), so a lib-test job has
/// something to test.
pub fn has_lib(repo: &Path, dir: &str) -> bool {
    let crate_dir = repo.join("core").join(dir);
    crate_dir.join("src/lib.rs").is_file()
        || std::fs::read_to_string(crate_dir.join("Cargo.toml")).is_ok_and(|m| m.lines().any(|l| l.trim() == "[lib]"))
}

/// A crate manifest's `[package] name`.
fn package_name(manifest: &str) -> Option<String> {
    section(manifest, "package")
        .lines()
        .find_map(|line| line.trim().strip_prefix("name"))
        .and_then(|rest| rest.split('"').nth(1))
        .map(str::to_string)
}

/// Every workspace member under `core/`, read from the root manifest and each crate's own.
pub fn read_members(repo: &Path) -> Result<Vec<Member>, String> {
    let read = |path: &Path| std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    let root = strip_comments(&read(&repo.join("Cargo.toml"))?);
    let members = array(&root, "members").ok_or("no `members` in the root Cargo.toml")?;
    let mut out = Vec::new();
    for dir in members.iter().filter_map(|m| m.strip_prefix("core/")) {
        let manifest = read(&repo.join("core").join(dir).join("Cargo.toml"))?;
        let name = package_name(&manifest).ok_or_else(|| format!("core/{dir}/Cargo.toml has no package name"))?;
        out.push(Member { name, dir: dir.to_string() });
    }
    if out.is_empty() {
        return Err("no core/ members read: the parse no longer matches the manifest".into());
    }
    Ok(out)
}

fn kept<'a>(members: &'a [Member], skip: &[&BTreeSet<String>]) -> Vec<&'a Member> {
    members.iter().filter(|m| skip.iter().all(|set| !set.contains(&m.name))).collect()
}

/// Crates the CI matrix tests.
pub fn tested<'a>(members: &'a [Member], policy: &Policy) -> Vec<&'a Member> {
    kept(members, &[&policy.untested])
}

/// Tested crates the per-file ratchets audit.
pub fn audited<'a>(members: &'a [Member], policy: &Policy) -> Vec<&'a Member> {
    kept(members, &[&policy.untested, &policy.unaudited])
}

/// Audited crates whose files are prefixed with their name (`continuum-base/time/mod.rs`).
pub fn split<'a>(members: &'a [Member], policy: &Policy) -> Vec<&'a Member> {
    audited(members, policy).into_iter().filter(|m| policy.split_prefixed.contains(&m.name)).collect()
}

/// Audited crates cut out of `continuum-core` that keep core's module names (files unprefixed).
pub fn modules<'a>(members: &'a [Member], policy: &Policy) -> Vec<&'a Member> {
    audited(members, policy)
        .into_iter()
        .filter(|m| m.name != crate::AUDITED_CRATE && !policy.split_prefixed.contains(&m.name))
        .collect()
}

/// Crates the warnings ratchet counts.
pub fn warned<'a>(members: &'a [Member], policy: &Policy) -> Vec<&'a Member> {
    kept(members, &[&policy.untested, &policy.warnings_uncounted])
}

/// The workspace and its policy, read once for the whole test run.
pub(crate) fn workspace() -> &'static (Vec<Member>, Policy) {
    static CACHE: std::sync::OnceLock<(Vec<Member>, Policy)> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let Some(repo) = crate::repo_root() else {
            panic!("no repository root above the audited crate: the crate policy cannot be read");
        };
        let read = || -> Result<_, String> {
            let policy = parse_policy(&std::fs::read_to_string(repo.join("Cargo.toml")).map_err(|e| e.to_string())?)?;
            Ok((read_members(&repo)?, policy))
        };
        read().unwrap_or_else(|e| panic!("crate policy: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "[workspace]\nmembers = [\n  \"core/continuum-core\", # the audited crate\n  \"core/continuum-base\",\n  \"core/continuum-host\",\n  \"core/continuum-new\",\n  \"core/llama\",\n  \"apps/cli\",\n]\n\
[workspace.metadata.ci]\n# comment [\"not-a-name\"]\nby_nature = [\"llama\"]\npending = []\nown_lib_test_step = [\"continuum-core\"]\nsplit_prefixed = [\"continuum-base\"]\nunaudited = [\"continuum-host\"]\nwarnings_uncounted = [\"continuum-base\", \"continuum-host\"]\n";

    fn fixture() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("crate-policy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(dir.join("Cargo.toml"), ROOT).unwrap_or_else(|e| panic!("{e}"));
        for (d, name) in [("continuum-core", "continuum-core"), ("continuum-base", "continuum-base"), ("continuum-host", "continuum-host"), ("continuum-new", "continuum-new"), ("llama", "llama")] {
            let crate_dir = dir.join("core").join(d);
            std::fs::create_dir_all(&crate_dir).unwrap_or_else(|e| panic!("{e}"));
            std::fs::write(crate_dir.join("Cargo.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n")).unwrap_or_else(|e| panic!("{e}"));
        }
        dir
    }

    fn names(members: Vec<&Member>) -> Vec<&str> {
        members.into_iter().map(|m| m.name.as_str()).collect()
    }

    // what this catches: a crate dropped into `members` that is not tested, audited and counted
    // with no list edited (#5456, #5450), and each exception still applying to its own facet.
    #[test]
    fn a_new_member_is_tested_audited_and_counted_with_no_list_edited() {
        let repo = fixture();
        let members = read_members(&repo).unwrap_or_else(|e| panic!("{e}"));
        let policy = parse_policy(ROOT).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(names(tested(&members, &policy)), ["continuum-core", "continuum-base", "continuum-host", "continuum-new"]);
        assert_eq!(names(audited(&members, &policy)), ["continuum-core", "continuum-base", "continuum-new"]);
        assert_eq!(names(split(&members, &policy)), ["continuum-base"]);
        assert_eq!(names(modules(&members, &policy)), ["continuum-new"]);
        assert_eq!(names(warned(&members, &policy)), ["continuum-core", "continuum-new"]);
        let _ = std::fs::remove_dir_all(&repo);
    }

    // what this catches: a renamed policy key reading as "no exceptions" and silently testing,
    // auditing and counting everything.
    #[test]
    fn a_missing_policy_key_is_an_error() {
        assert!(parse_policy("[workspace]\nmembers = []\n").is_err());
        assert!(parse_policy(&ROOT.replace("unaudited", "unaudit")).is_err());
    }
}
