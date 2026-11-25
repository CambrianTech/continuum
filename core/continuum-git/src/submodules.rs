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
///
/// It also writes `submodule.recurse=false` into the tree's config, so no other git command
/// in it reaches into a submodule by itself and this stays the one place they move. A fetch
/// otherwise fetches the submodule commit every fetched branch pins (`submodule.recurse`, or
/// `fetch.recurseSubmodules` set or at its on-demand default; an explicit `false` here stops
/// all three, measured by the test below), so one branch pinning a commit its submodule's
/// remote never got stops every node's deploy (the IntelMac, 2026-11-25 03:06Z: a resident's
/// branch pinned fork commit f8eddaf2, "upload-pack: not our ref"). The tip's own pin is
/// fetched by the `submodule update` below, so a deploy needs nothing this skips.
pub fn align(repo: &Path, git: &dyn Fn(&[&str]) -> Result<String, String>) -> Result<(), String> {
    git(&["config", "submodule.recurse", "false"])?;
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

    // what this catches (the IntelMac, 2026-11-25 03:06Z): a deploy that cannot fetch because
    // some other branch of origin pins a submodule commit its remote never received, with
    // `submodule.recurse` set or under git's on-demand default. A tree its last deploy
    // aligned fetches with the consumer's own unchanged command, deploys its tip, and aligns its pin.
    #[test]
    fn another_branch_pinning_a_missing_submodule_commit_does_not_stop_a_deploy() {
        let _git_running = crate::GIT_PROCESSES.lock().unwrap_or_else(|poisoned| poisoned.into_inner()); // unwrap_or_else: a neighbour's failure must not fail this test too
        let root = tempfile::tempdir().expect("tempdir");
        let engine = root.path().join("engine");
        std::fs::create_dir(&engine).expect("engine dir");
        run(&engine, &["init", "-q"]);
        std::fs::write(engine.join("a"), "one").expect("write");
        run(&engine, &["add", "a"]);
        run(&engine, &["commit", "-qm", "the published pin"]);

        let origin = root.path().join("origin");
        std::fs::create_dir(&origin).expect("origin dir");
        run(&origin, &["init", "-q", "-b", "canary"]);
        run(&origin, &["submodule", "add", "-q", &engine.to_string_lossy(), "vendor/engine"]);
        run(&origin, &["commit", "-qm", "core on the published pin"]);

        // Deploy trees as nodes have them, every one cloned before the bad branch exists: each
        // fetches it once (a failed fetch already moved the refs, so a second one has nothing
        // left to recurse into and would prove nothing). One sets `submodule.recurse`, as the
        // IntelMac's tree does; one sets `fetch.recurseSubmodules` itself; one keeps git's on-demand default.
        let tree = |name: &str, config: Option<(&str, &str)>| {
            let dir = root.path().join(name);
            run(root.path(), &["clone", "-q", &origin.to_string_lossy(), name]);
            run(&dir, &["submodule", "update", "-q", "--init"]);
            if let Some((key, value)) = config {
                run(&dir, &["config", key, value]);
            }
            dir
        };
        let configs = [Some(("submodule.recurse", "true")), Some(("fetch.recurseSubmodules", "on-demand")), None];
        let trees: Vec<_> = configs.iter().enumerate().map(|(i, config)| (tree(&format!("plain-{i}"), *config), tree(&format!("deploy-{i}"), *config))).collect();
        // Each deploy tree was aligned by its last deploy, as every node's is.
        for (_, deploy) in &trees {
            align(deploy, &runner(deploy)).expect("the last deploy's align");
        }

        // Another branch pins an engine commit that only its author's checkout ever had.
        run(&origin.join("vendor/engine"), &["checkout", "-q", "-b", "unpushed"]);
        std::fs::write(origin.join("vendor/engine/a"), "never pushed").expect("write");
        run(&origin.join("vendor/engine"), &["commit", "-qam", "a pin its remote never got"]);
        run(&origin, &["checkout", "-q", "-b", "resident"]);
        run(&origin, &["add", "vendor/engine"]);
        run(&origin, &["commit", "-qm", "a branch pinning the unpushed commit"]);
        run(&origin, &["checkout", "-q", "canary"]);
        // The tip moves its pin to a newly PUBLISHED engine commit, which no deploy tree has yet:
        // `align` must still fetch it with recursion off.
        std::fs::write(engine.join("a"), "two").expect("write");
        run(&engine, &["commit", "-qam", "the next published pin"]);
        let published = run(&engine, &["rev-parse", "HEAD"]);
        run(&origin.join("vendor/engine"), &["fetch", "-q", "origin"]);
        run(&origin.join("vendor/engine"), &["checkout", "-q", &published]);
        run(&origin, &["add", "vendor/engine"]);
        run(&origin, &["commit", "-qm", "the tip a node deploys, on the next pin"]);
        let tip = run(&origin, &["rev-parse", "HEAD"]);

        for (plain, deploy) in &trees {
            // the engine commit lives only in origin's checkout, never in the engine remote
            assert!(runner(plain)(&["fetch", "--quiet", "origin"]).is_err(), "test: a plain fetch reaches the missing pin ({})", plain.display());
            // the deploy consumer's own commands, unchanged: git_in runs plain git
            let git = runner(deploy);
            git(&["fetch", "--quiet", "origin"]).expect("an aligned tree's fetch never reaches into submodules");
            git(&["checkout", "--quiet", "--detach", &tip]).expect("the deploy checks out its tip");
            align(deploy, &git).expect("align the tip's own pin");
            assert_eq!(run(&deploy.join("vendor/engine"), &["rev-parse", "HEAD"]), published, "the tip's pin, as published");
        }
    }
}
