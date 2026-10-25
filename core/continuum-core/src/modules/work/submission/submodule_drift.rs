//! A derived submission carries only what the card is about (card b3ecd1ac). work/submit
//! diffs her working tree, so a submodule pointer she moved, or that drifted under her,
//! rides along: Sahar's 3b33dd85 (2026-10-25) carried core/vendor/llama.cpp
//! 4a79fed98b -> 76dfd6482e beside a shutdown fix, an engine upgrade nobody asked for.

/// One submodule pointer the patch moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SubmoduleMove {
    pub path: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// PURE: every submodule pointer the unified diff moves (`Subproject commit` lines under one
/// `diff --git` header).
pub(super) fn submodule_moves(patch: &str) -> Vec<SubmoduleMove> {
    let mut moves: Vec<SubmoduleMove> = Vec::new();
    let mut path: Option<String> = None;
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            path = rest.rsplit_once(" b/").map(|(_, b)| b.to_string());
            continue;
        }
        let (side, commit) = match (line.strip_prefix("-Subproject commit "), line.strip_prefix("+Subproject commit ")) {
            (Some(c), _) => ('-', c),
            (_, Some(c)) => ('+', c),
            _ => continue,
        };
        let Some(p) = path.clone() else { continue };
        let entry = match moves.iter_mut().find(|m| m.path == p) {
            Some(entry) => entry,
            None => {
                moves.push(SubmoduleMove { path: p, from: None, to: None });
                let last = moves.len() - 1;
                &mut moves[last]
            }
        };
        let commit = commit.trim().trim_end_matches("-dirty").to_string();
        if side == '-' { entry.from = Some(commit) } else { entry.to = Some(commit) }
    }
    // A dirty submodule (`X` -> `X-dirty`) moves no pointer: only a changed commit is a move.
    moves.retain(|m| m.from != m.to);
    moves
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
    let paths: Vec<String> = unlisted.iter().map(|m| format!("\"{}\"", m.path)).collect();
    let restore: Vec<String> = unlisted
        .iter()
        .map(|m| format!("git checkout {base_sha} -- {0} && git submodule update {0}", m.path))
        .collect();
    Some(format!(
        "submission refused: your checkout moves submodule {}, which you did not list. If the \
         move is yours, submit again with submodules: [{}]. If not, restore it from the repo \
         root ({}), commit, and submit again.",
        named.join(", "),
        paths.join(", "),
        restore.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAHAR: &str = "diff --git a/core/x.rs b/core/x.rs\n--- a/core/x.rs\n+++ b/core/x.rs\n@@ -1 +1 @@\n-a\n+b\n\
diff --git a/core/vendor/llama.cpp b/core/vendor/llama.cpp\nindex 4a79fed98b..76dfd6482e 160000\n--- a/core/vendor/llama.cpp\n\
+++ b/core/vendor/llama.cpp\n@@ -1 +1 @@\n-Subproject commit 4a79fed98b95576a3e5965c7fcd421637c88a6ca\n\
+Subproject commit 76dfd6482e608eba4906d82d2cef4eda1b1181af\n";

    // what this catches (card b3ecd1ac): Sahar's shutdown fix carried an unasked llama.cpp
    // pointer move. The move is found with both ends; a move she did not list refuses, naming
    // the path, both ends, how to list it, and a restore to the BASE pointer; a listed move
    // rides; a patch with no submodule, or only a dirty one, passes.
    #[test]
    fn an_unlisted_submodule_move_is_refused_by_name_and_a_listed_one_rides() {
        let moves = submodule_moves(SAHAR);
        assert_eq!(
            moves,
            vec![SubmoduleMove {
                path: "core/vendor/llama.cpp".into(),
                from: Some("4a79fed98b95576a3e5965c7fcd421637c88a6ca".into()),
                to: Some("76dfd6482e608eba4906d82d2cef4eda1b1181af".into()),
            }]
        );
        let base = "1111111111111111111111111111111111111111";
        let refusal = refuse_unlisted(&moves, &[], base).unwrap_or_default();
        assert!(
            refusal.contains("core/vendor/llama.cpp (4a79fed98b95576a3e5965c7fcd421637c88a6ca -> 76dfd6482e608eba4906d82d2cef4eda1b1181af)"),
            "{refusal}"
        );
        assert!(refusal.contains("submodules: [\"core/vendor/llama.cpp\"]"), "{refusal}");
        assert!(
            refusal.contains(&format!("git checkout {base} -- core/vendor/llama.cpp && git submodule update core/vendor/llama.cpp")),
            "{refusal}"
        );
        assert_eq!(refuse_unlisted(&moves, &["core/vendor/llama.cpp".into()], base), None, "a listed move rides");
        assert_eq!(refuse_unlisted(&moves, &["core/vendor/llama.cpp/".into()], base), None, "a trailing slash is the same path");
        assert!(refuse_unlisted(&moves, &["llama.cpp".into()], base).is_some(), "the leaf alone is not the path");
        assert!(submodule_moves("diff --git a/a b/a\n-x\n+y\n").is_empty(), "no submodule, nothing to refuse");
        let dirty = "diff --git a/core/vendor/llama.cpp b/core/vendor/llama.cpp\n-Subproject commit 4a79fed98b\n+Subproject commit 4a79fed98b-dirty\n";
        assert!(submodule_moves(dirty).is_empty(), "a dirty submodule moves no pointer");
    }
}
