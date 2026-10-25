//! A derived submission carries only what the card is about (card b3ecd1ac). work/submit
//! diffs her working tree, so a submodule pointer she moved, or that drifted under her,
//! rides along: Sahar's 3b33dd85 (2026-10-25) carried core/vendor/llama.cpp
//! 4a79fed98b -> 76dfd6482e beside a shutdown fix, an engine upgrade nobody asked for.
//!
//! Moves are read from git's structured diff (`--raw -z`: NUL-terminated paths, never quoted),
//! and a submodule is a gitlink, mode 160000 on either side: never a line of text that says
//! `Subproject commit`, which an ordinary file can hold (Codex on #5402).

use std::path::Path;

/// One submodule pointer the candidate moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SubmoduleMove {
    pub path: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// The gitlink mode: a submodule pointer.
const GITLINK: &str = "160000";

/// One record of `git diff --raw -z`.
#[derive(Debug, PartialEq, Eq)]
struct RawEntry {
    old_mode: String,
    new_mode: String,
    old_sha: String,
    new_sha: String,
    path: Vec<u8>,
}

/// PURE: the records of `git diff --raw -z --no-renames` output: `:old new old_sha new_sha
/// status` then the path, each NUL-terminated. A record that does not parse is an error.
fn parse_raw(bytes: &[u8]) -> Result<Vec<RawEntry>, String> {
    let mut fields = bytes.split(|b| *b == 0).filter(|f| !f.is_empty());
    let mut entries = Vec::new();
    while let Some(head) = fields.next() {
        let head = std::str::from_utf8(head).map_err(|e| format!("a raw diff header is not text: {e}"))?;
        let parts: Vec<&str> = head.trim_start_matches(':').split(' ').collect();
        let [old_mode, new_mode, old_sha, new_sha, _status] = parts.as_slice() else {
            return Err(format!("a raw diff header has {} fields, not 5: {head:?}", parts.len()));
        };
        let path = fields.next().ok_or_else(|| format!("a raw diff record has no path: {head:?}"))?;
        entries.push(RawEntry {
            old_mode: (*old_mode).into(),
            new_mode: (*new_mode).into(),
            old_sha: (*old_sha).into(),
            new_sha: (*new_sha).into(),
            path: path.to_vec(),
        });
    }
    Ok(entries)
}

fn named(sha: &str) -> Option<String> {
    (!sha.bytes().all(|b| b == b'0')).then(|| sha.to_string())
}

/// Every submodule pointer the checkout at `ws` moves against `base`, over the same paths the
/// submitted patch covers (`excludes` as pathspecs). The working tree's side of a submodule is
/// the commit its own HEAD names (git reports none for an unstaged gitlink); a submodule whose
/// commit is unchanged (only dirty) moves nothing.
pub(super) fn submodule_moves_in(ws: &Path, base: &str, excludes: &[&str]) -> Result<Vec<SubmoduleMove>, String> {
    let mut args = vec!["diff", "--raw", "-z", "--no-abbrev", "--no-renames", "--no-ext-diff", "--no-textconv", base, "--", "."];
    args.extend_from_slice(excludes);
    let out = std::process::Command::new("git").args(&args).current_dir(ws).output().map_err(|e| format!("could not read {}'s raw diff: {e}", ws.display()))?;
    if !out.status.success() {
        return Err(format!("could not read {}'s raw diff against {base}: {}", ws.display(), String::from_utf8_lossy(&out.stderr).trim()));
    }
    let mut moves = Vec::new();
    for entry in parse_raw(&out.stdout)? {
        if entry.old_mode != GITLINK && entry.new_mode != GITLINK {
            continue;
        }
        let path = String::from_utf8(entry.path).map_err(|e| format!("a submodule path is not UTF-8, so it cannot be named: {e}"))?;
        let to = match named(&entry.new_sha) {
            Some(sha) => Some(sha),
            None if entry.new_mode == GITLINK => submodule_head(&ws.join(&path))?,
            None => None,
        };
        let from = if entry.old_mode == GITLINK { named(&entry.old_sha) } else { None };
        if from != to {
            moves.push(SubmoduleMove { path, from, to });
        }
    }
    Ok(moves)
}

/// The commit a submodule's own checkout stands on; `None` when it is not checked out.
fn submodule_head(dir: &Path) -> Result<Option<String>, String> {
    if !dir.join(".git").exists() {
        return Ok(None);
    }
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("could not read {}'s HEAD: {e}", dir.display()))?;
    if !out.status.success() {
        return Err(format!("could not read {}'s HEAD: {}", dir.display(), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(Some(String::from_utf8_lossy(&out.stdout).trim().to_string()))
}

/// A path as one POSIX shell word: single-quoted, with any `'` closed, escaped and reopened
/// (POSIX sh syntax; PowerShell quotes differently, so the refusal says which shell it is for).
fn shell_word(path: &str) -> String {
    format!("'{}'", path.replace('\'', r"'\''"))
}

/// The refusal for moves she did not list in `submodules`, or None. Whether a bump is
/// intended is HER choice, typed on the submit; no card text is read (Fable on #5402).
/// The restore names the base pointer: `git submodule update` alone checks out what her
/// index records, which after a committed bump is the moved pointer itself.
pub(super) fn refuse_unlisted(moves: &[SubmoduleMove], listed: &[String], base_sha: &str) -> Option<String> {
    let unlisted: Vec<&SubmoduleMove> = moves
        .iter()
        .filter(|m| !listed.iter().any(|l| l.trim_end_matches('/') == m.path))
        .collect();
    if unlisted.is_empty() {
        return None;
    }
    // Whole commit ids: an id is an identity, never cut short.
    let commit = |c: &Option<String>| c.as_deref().map_or("none", |c| c).to_string();
    let named: Vec<String> = unlisted.iter().map(|m| format!("{} ({} -> {})", m.path, commit(&m.from), commit(&m.to))).collect();
    let paths: Vec<String> = unlisted.iter().map(|m| serde_json::Value::String(m.path.clone()).to_string()).collect(); // wire boundary: the paths as she would type them in JSON
    let restore: Vec<String> = unlisted
        .iter()
        .map(|m| format!("git checkout {base_sha} -- {0} && git submodule update -- {0}", shell_word(&m.path)))
        .collect();
    Some(format!(
        "submission refused: your checkout moves submodule {}, which you did not list. If the \
         move is yours, submit again with submodules: [{}]. If not, restore it from the repo \
         root (POSIX shell: {}), commit, and submit again.",
        named.join(", "),
        paths.join(", "),
        restore.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "protocol.file.allow=always", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git runs"); // JUSTIFIED: test fixture
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn head(dir: &Path) -> String {
        let out = std::process::Command::new("git").args(["rev-parse", "HEAD"]).current_dir(dir).output().expect("git runs"); // JUSTIFIED: test fixture
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    // what this catches (card b3ecd1ac; Codex on #5402): Sahar's shutdown fix carried an unasked
    // submodule pointer move. Over a real repository: a gitlink at a path git must quote (non-ASCII
    // and a space) is found with both ends; an ordinary file whose lines read
    // `Subproject commit ...` is not a submodule; an unlisted move refuses with a shell-safe
    // restore to the BASE pointer and the JSON-quoted path to list; a listed move rides.
    #[test]
    fn a_submodule_move_is_a_gitlink_found_by_mode_and_an_unlisted_one_is_refused() {
        let root = tempfile::tempdir().expect("root"); // JUSTIFIED: test fixture
        let (sub, repo) = (root.path().join("sub"), root.path().join("repo"));
        std::fs::create_dir_all(&sub).expect("dir"); // JUSTIFIED: test fixture
        git(&sub, &["init", "-q"]);
        git(&sub, &["commit", "-q", "--allow-empty", "-m", "one"]);
        let first = head(&sub);
        std::fs::create_dir_all(&repo).expect("dir"); // JUSTIFIED: test fixture
        git(&repo, &["init", "-q"]);
        std::fs::write(repo.join("notes.txt"), "Subproject commit 1111111111111111111111111111111111111111\n").expect("write"); // JUSTIFIED: test fixture
        let quoted = "vendor/\u{f6}dd name";
        git(&repo, &["submodule", "add", "-q", sub.to_str().expect("utf-8"), quoted]);
        git(&repo, &["commit", "-q", "-am", "base"]);
        let base = head(&repo);

        // An ordinary file that merely says "Subproject commit" changes: never a submodule move.
        std::fs::write(repo.join("notes.txt"), "Subproject commit 2222222222222222222222222222222222222222\n").expect("write"); // JUSTIFIED: test fixture
        assert_eq!(submodule_moves_in(&repo, &base, &[]), Ok(Vec::new()));

        // The submodule's checkout moves on: a move, both ends, at its exact (quoted-by-git) path.
        let inner = repo.join(quoted);
        git(&inner, &["commit", "-q", "--allow-empty", "-m", "two"]);
        let second = head(&inner);
        let moves = submodule_moves_in(&repo, &base, &[]).expect("moves"); // JUSTIFIED: test asserts the moves
        assert_eq!(moves, vec![SubmoduleMove { path: quoted.into(), from: Some(first.clone()), to: Some(second.clone()) }]);

        let refusal = refuse_unlisted(&moves, &[], &base).unwrap_or_default();
        assert!(refusal.contains(&format!("{quoted} ({first} -> {second})")), "{refusal}");
        assert!(refusal.contains(&format!("submodules: [\"{quoted}\"]")), "{refusal}");
        assert!(refusal.contains(&format!("git checkout {base} -- '{quoted}' && git submodule update -- '{quoted}'")), "{refusal}");
        assert_eq!(refuse_unlisted(&moves, &[quoted.into()], &base), None, "a listed move rides");
        assert_eq!(refuse_unlisted(&moves, &[format!("{quoted}/")], &base), None, "a trailing slash is the same path");
        assert!(refuse_unlisted(&moves, &["\u{f6}dd name".into()], &base).is_some(), "the leaf alone is not the path");

        // Back on the base pointer: nothing moves.
        git(&inner, &["checkout", "-q", &first]);
        assert_eq!(submodule_moves_in(&repo, &base, &[]), Ok(Vec::new()));
    }

    // what this catches: a restore whose path a shell would split or expand.
    #[test]
    fn a_restore_path_is_one_shell_word() {
        assert_eq!(shell_word("a b"), "'a b'");
        assert_eq!(shell_word("it's"), r"'it'\''s'");
        assert_eq!(shell_word("$(rm -rf x)"), "'$(rm -rf x)'");
    }
}
