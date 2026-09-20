//! The vendored llama.cpp pin is a DECISION, recorded in `core/vendor/LLAMA_CPP_PIN`.
//!
//! 2026-09-14: #4007 pinned the fork at 965d38a9 (context checkpoints ride with a slot
//! page — restores warm, 4.3 s → 0.16 s). A later branch, cut from a worktree whose
//! submodule sat at the OLD commit and staged with `git add -A core`, carried the
//! pointer back to 76dfd6482 through its squash merge; the next deploy rebuilt the
//! old server and every restore came back cold again — silently, the scoreboard
//! and the citizens paid for it for an hour. A submodule pointer regresses without
//! anyone touching it on purpose; this test makes the pin an explicit file that a
//! PR must edit to move, and refuses a tree whose pointer disagrees with it.
#[cfg(test)]
mod tests {
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repo root")
    }

    // what this catches: a squash merge (or a stale worktree) moving the vendored
    // llama.cpp pointer away from the recorded pin — the 2026-09-14 regression that
    // rebuilt the pre-checkpoint server and made every KV restore cold again.
    #[test]
    fn the_vendored_llama_cpp_pointer_equals_the_recorded_pin() {
        let root = repo_root();
        let recorded = std::fs::read_to_string(root.join("core/vendor/LLAMA_CPP_PIN"))
            .expect("core/vendor/LLAMA_CPP_PIN exists")
            .trim()
            .to_string();
        let out = std::process::Command::new("git")
            .args(["ls-tree", "HEAD", "core/vendor/llama.cpp"])
            .current_dir(&root)
            .output()
            .expect("git ls-tree");
        let line = String::from_utf8_lossy(&out.stdout);
        let Some(pointer) = line.split_whitespace().nth(2) else {
            eprintln!("not a git checkout (or no submodule row): {line:?} — nothing to check");
            return;
        };
        assert_eq!(
            pointer, recorded,
            "core/vendor/llama.cpp points at {pointer} but core/vendor/LLAMA_CPP_PIN records {recorded}. \
             Moving the pin is a decision: edit the file in the same commit (`git submodule update` \
             in your worktree first — a stale worktree submodule staged with `git add -A` is how it regressed)."
        );
    }
}
