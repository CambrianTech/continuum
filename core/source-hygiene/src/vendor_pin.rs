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

    fn repo_root() -> std::path::PathBuf {
        // Anchored on the workspace manifest, not on `../..`. The old form was correct
        // only because this file sat two levels down, and it survived the move to
        // `core/source-hygiene` by coincidence rather than by design (Cormac, review of
        // #4346). A wrong-but-existing root would canonicalize fine and then read the
        // pin from the wrong tree.
        crate::repo_root().expect(
            "no ancestor of CARGO_MANIFEST_DIR carries the workspace Cargo.toml — the \
             vendored-pin check cannot locate the tree it is asserting about",
        )
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

    // what this catches: the pin moving to an engine that no longer parses `tail_tools` while
    // the core still offers a mind's learned tool head to every lane that reports the pinned
    // build (`llama_server::engine_features`): her tail verbs would arrive as text, not calls.
    #[test]
    fn the_pinned_engine_source_parses_tail_tools() {
        let parser = repo_root().join("core/vendor/llama.cpp/tools/server/server-common.cpp");
        let Ok(source) = std::fs::read_to_string(&parser) else {
            eprintln!("{} not checked out — nothing to check", parser.display());
            return;
        };
        assert!(
            source.contains(r#"json_value(body, "tail_tools""#),
            "the pinned llama.cpp ({}) does not parse `tail_tools`, but the core offers tail verbs to a \
             lane running exactly this build: move the pin to a fork commit that carries \
             CambrianTech/llama.cpp#75, or change `engine_features` in the same PR",
            parser.display()
        );
    }

    // what this catches: the pin moving to an engine that ignores `view_messages` and
    // `keep_view_checkpoints`. It would checkpoint no view end and save only the head
    // (`turn_admission::page_body` sends `keep_newest_checkpoints: 0` for that engine), so every
    // page-in would resume from the ~11k head again (card 3512f496).
    #[test]
    fn the_pinned_engine_source_checkpoints_the_view_end_and_pages_it() {
        let server = repo_root().join("core/vendor/llama.cpp/tools/server/server-context.cpp");
        let Ok(source) = std::fs::read_to_string(&server) else {
            eprintln!("{} not checked out — nothing to check", server.display());
            return;
        };
        for field in [r#""view_messages""#, r#""keep_view_checkpoints""#] {
            assert!(
                source.contains(field),
                "the pinned llama.cpp ({}) does not read {field}: move the pin to a fork commit that \
                 checkpoints the view end, or change `turn_admission::page_body` in the same PR",
                server.display()
            );
        }
    }
}
