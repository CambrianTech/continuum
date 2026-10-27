//! A deploy checkout's submodules: what counts as an operator's work, and aligning the
//! checkouts that are present to the superproject's gitlinks (card a1e1af51).
//!
//! A checkout moves gitlinks, not submodule trees. The deploy consumer aligned them with
//! `git submodule update`, which touches INITIALIZED submodules only. On enrico the
//! engine's checkout was present (its `.git` file pointing into `.git/modules`) but never
//! registered in `.git/config`, so every deploy and rollback across an engine pin left
//! `core/vendor/llama.cpp` at the old commit (2026-10-27: rolled back to 7d8bff696 and still
//! at 741b0175b, then the reverse). The tracker counted that pointer as dirt and the
//! consumer ignored it, so the node read `RefuseDirty` and took no deploy until a hand
//! realigned it. Both now read the tree through [`has_operator_work`], and the consumer
//! aligns through [`align`].

use std::path::Path;

/// The `git status` arguments whose output [`has_operator_work`] reads. Tracked changes only:
/// an untracked file cannot change what a detached checkout of the tip builds.
pub const STATUS_ARGS: [&str; 3] = ["status", "--porcelain=v2", "--untracked-files=no"];

/// PURE: whether `git status --porcelain=v2` output shows work an operator could lose. A
/// changed file is work, and so are changes inside a submodule. A submodule whose checked-out
/// commit merely differs from the gitlink is not: aligning it is the deploy's own job.
pub fn has_operator_work(status_v2: &str) -> bool {
    status_v2.lines().any(|line| {
        let mut fields = line.split(' ');
        match (fields.next(), fields.next(), fields.next()) {
            // The deploy's own drift, and nothing else: an ordinary entry whose only change is
            // an UNSTAGED submodule commit ("<XY>=.M", "<sub>=SC.."). Fails closed: a staged
            // gitlink, a rename or copy (2), an unmerged entry (u), a change inside the
            // submodule or any file is someone's work (Codex on #5525).
            (Some("1"), Some(".M"), Some("SC..")) => false,
            (Some("1" | "2" | "u"), _, _) => true,
            _ => false,
        }
    })
}

/// PURE: the paths `git submodule status` lists as not initialized (a leading `-`).
pub fn uninitialized(submodule_status: &str) -> Vec<&str> {
    submodule_status
        .lines()
        .filter_map(|line| line.strip_prefix('-'))
        .filter_map(|rest| rest.split_whitespace().nth(1))
        .collect()
}

/// Align every submodule whose checkout is present to the superproject's gitlinks, running
/// git through the caller's own runner (`git(args)` in the deploy tree: the consumer's one
/// spawn site, so this crate adds none). A present checkout that was never registered is
/// initialized first (`git submodule init` writes config only), so this never starts a
/// clone; an absent one stays absent. The caller refuses a tree with [`has_operator_work`]
/// before moving it, so nothing aligned here holds an operator's change.
pub fn align(repo: &Path, git: &dyn Fn(&[&str]) -> Result<String, String>) -> Result<(), String> {
    let status = git(&["submodule", "status"])?;
    for path in uninitialized(&status) {
        if repo.join(path).join(".git").exists() {
            git(&["submodule", "init", "--", path])?;
        }
    }
    git(&["submodule", "update", "--quiet", "--recursive"]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a stale pointer counted as an operator's work (the tracker wedged at
    // RefuseDirty), or a change inside the engine's checkout not counted (a deploy losing it).
    #[test]
    fn a_moved_pointer_is_not_work_and_a_change_inside_is() {
        let drift = "1 .M SC.. 160000 160000 160000 7d8b 741b core/vendor/llama.cpp\n";
        let inside = "1 .M S.M. 160000 160000 160000 741b 741b core/vendor/llama.cpp\n";
        let file = "1 .M N... 100644 100644 100644 aaaa bbbb core/Cargo.toml\n";
        assert!(!has_operator_work(drift));
        assert!(has_operator_work(inside));
        assert!(has_operator_work(file));
        assert!(has_operator_work("u UU N... 100644 100644 100644 100644 a b c core/x.rs\n"));
        let staged = "1 M. SC.. 160000 160000 160000 7d8b 741b core/vendor/llama.cpp\n";
        assert!(has_operator_work(staged), "a staged gitlink is an operator's work");
        let renamed = "2 R. S... 160000 160000 160000 7d8b 7d8b R100 core/vendor/llama.cpp\tvendor/llama.cpp\n";
        assert!(has_operator_work(renamed), "a renamed submodule is an operator's work");
        assert!(!has_operator_work(""));
    }

    /// The deploy's runner, as the consumer passes `git_in`: git in `dir`, stdout or the error.
    fn runner(dir: &Path) -> impl Fn(&[&str]) -> Result<String, String> + '_ {
        move |args| {
            let out = std::process::Command::new("git")
                .current_dir(dir)
                .args(["-c", "protocol.file.allow=always"])
                .args(args)
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).into_owned());
            }
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        }
    }

    fn run(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.email=fixture@continuum", "-c", "user.name=fixture", "-c", "protocol.file.allow=always"])
            .args(args)
            .output()
            .expect("git runs in the fixture");
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    // what this catches (card a1e1af51, enrico 2026-10-27): a deploy tree whose engine checkout
    // is present but unregistered stays at the old pin on every move, forward or back, and
    // reads as dirty; and an absent submodule being cloned by a deploy.
    #[test]
    fn a_present_unregistered_checkout_follows_the_pin_both_ways() {
        let _git_running = crate::GIT_PROCESSES.lock().unwrap_or_else(|poisoned| poisoned.into_inner()); // unwrap_or_else: a neighbour's failure must not fail this test too
        let root = tempfile::tempdir().expect("tempdir");
        let engine = root.path().join("engine");
        std::fs::create_dir(&engine).expect("engine dir");
        run(&engine, &["init", "-q"]);
        std::fs::write(engine.join("a"), "one").expect("write");
        run(&engine, &["add", "a"]);
        run(&engine, &["commit", "-qm", "old pin"]);
        let old_pin = run(&engine, &["rev-parse", "HEAD"]);
        std::fs::write(engine.join("a"), "two").expect("write");
        run(&engine, &["commit", "-qam", "new pin"]);
        let new_pin = run(&engine, &["rev-parse", "HEAD"]);

        let origin = root.path().join("origin");
        std::fs::create_dir(&origin).expect("origin dir");
        run(&origin, &["init", "-q"]);
        let url = engine.to_string_lossy().into_owned();
        run(&origin, &["submodule", "add", "-q", &url, "vendor/engine"]);
        run(&origin.join("vendor/engine"), &["checkout", "-q", &old_pin]);
        run(&origin, &["add", "vendor/engine"]);
        run(&origin, &["commit", "-qm", "core on the old pin"]);
        let old_core = run(&origin, &["rev-parse", "HEAD"]);
        run(&origin.join("vendor/engine"), &["checkout", "-q", &new_pin]);
        run(&origin, &["add", "vendor/engine"]);
        run(&origin, &["commit", "-qm", "core on the new pin"]);
        let new_core = run(&origin, &["rev-parse", "HEAD"]);

        // The deploy tree, as enrico had it: the engine checked out, its registration gone.
        let deploy = root.path().join("deploy");
        run(root.path(), &["clone", "-q", &origin.to_string_lossy(), "deploy"]);
        run(&deploy, &["submodule", "update", "-q", "--init"]);
        let name = run(&deploy, &["config", "-f", ".gitmodules", "--get-regexp", "path"]);
        let name = name.split_whitespace().next().expect("a submodule entry").trim_start_matches("submodule.").trim_end_matches(".path").to_string();
        run(&deploy, &["config", "--remove-section", &format!("submodule.{name}")]);
        assert_eq!(uninitialized(&run(&deploy, &["submodule", "status"])).len(), 1, "unregistered, as on enrico");
        let engine_head = || run(&deploy.join("vendor/engine"), &["rev-parse", "HEAD"]);

        for (core, pin) in [(&old_core, &old_pin), (&new_core, &new_pin), (&old_core, &old_pin)] {
            run(&deploy, &["checkout", "-q", "--detach", core]);
            assert!(!has_operator_work(&run(&deploy, &STATUS_ARGS)), "a moved pointer is the deploy's to align, not dirt");
            align(&deploy, &runner(&deploy)).expect("align");
            assert_eq!(&engine_head(), pin, "the engine checkout follows the core's pin");
            assert_eq!(run(&deploy, &STATUS_ARGS), "", "aligned: the tree reads clean");
        }

        // A gitlink STAGED by hand is an operator's work, though nothing inside changed.
        run(&deploy.join("vendor/engine"), &["checkout", "-q", &new_pin]);
        run(&deploy, &["add", "vendor/engine"]);
        assert!(has_operator_work(&run(&deploy, &STATUS_ARGS)), "a staged pointer is not the deploy's to move");
        run(&deploy, &["reset", "-q", "--", "vendor/engine"]);
        align(&deploy, &runner(&deploy)).expect("align");

        // A change inside the engine checkout is an operator's work.
        std::fs::write(deploy.join("vendor/engine/a"), "hand edit").expect("write");
        assert!(has_operator_work(&run(&deploy, &STATUS_ARGS)));

        // An absent submodule is never cloned by a deploy.
        let bare = root.path().join("bare");
        run(root.path(), &["clone", "-q", &origin.to_string_lossy(), "bare"]);
        align(&bare, &runner(&bare)).expect("align with nothing present");
        assert!(!bare.join("vendor/engine/.git").exists(), "no clone started");
    }
}
