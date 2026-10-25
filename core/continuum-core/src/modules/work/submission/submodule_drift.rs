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

/// PURE: whether the card asks for this submodule: its title or body names the path or the
/// path's last component (e.g. "llama.cpp").
pub(super) fn card_names(title: &str, body: Option<&str>, path: &str) -> bool {
    let leaf = path.rsplit('/').next().filter(|leaf| !leaf.is_empty()).map_or(path, |leaf| leaf);
    let text = format!("{title}\n{}", body.unwrap_or_default()).to_lowercase(); // unwrap_or_default: a card with no body names nothing there
    text.contains(&path.to_lowercase()) || text.contains(&leaf.to_lowercase())
}

/// The refusal for moves the card does not name, or None.
pub(super) fn refuse_unasked(moves: &[SubmoduleMove], title: &str, body: Option<&str>) -> Option<String> {
    let unasked: Vec<&SubmoduleMove> = moves.iter().filter(|m| !card_names(title, body, &m.path)).collect();
    if unasked.is_empty() {
        return None;
    }
    let short = |c: &Option<String>| c.as_deref().map_or("none".to_string(), |c| c.chars().take(10).collect());
    let listed: Vec<String> = unasked.iter().map(|m| format!("{} ({} -> {})", m.path, short(&m.from), short(&m.to))).collect();
    let restore: Vec<String> = unasked.iter().map(|m| format!("git submodule update {}", m.path)).collect();
    Some(format!(
        "submission refused: your checkout moves submodule {}, which this card does not name. \
         Restore it from the repo root ({}), then submit again. If the bump is intended, it \
         belongs on a card that names it.",
        listed.join(", "),
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
    // pointer move. The move is found with both ends; a card that does not name it refuses,
    // naming the path and the restore command; a card that names it (by path or leaf) passes;
    // a patch with no submodule passes.
    #[test]
    fn an_unasked_submodule_move_is_refused_by_name() {
        let moves = submodule_moves(SAHAR);
        assert_eq!(
            moves,
            vec![SubmoduleMove {
                path: "core/vendor/llama.cpp".into(),
                from: Some("4a79fed98b95576a3e5965c7fcd421637c88a6ca".into()),
                to: Some("76dfd6482e608eba4906d82d2cef4eda1b1181af".into()),
            }]
        );
        let refusal = refuse_unasked(&moves, "#3940 legacycore is decided by parsing", None).unwrap_or_default();
        assert!(refusal.contains("core/vendor/llama.cpp (4a79fed98b -> 76dfd6482e)"), "{refusal}");
        assert!(refusal.contains("git submodule update core/vendor/llama.cpp"), "{refusal}");
        assert_eq!(refuse_unasked(&moves, "Upgrade llama.cpp to b5", None), None, "named by its leaf");
        assert_eq!(refuse_unasked(&moves, "engine", Some("bump core/vendor/llama.cpp")), None, "named by path");
        assert!(submodule_moves("diff --git a/a b/a\n-x\n+y\n").is_empty(), "no submodule, nothing to refuse");
        let dirty = "diff --git a/core/vendor/llama.cpp b/core/vendor/llama.cpp\n-Subproject commit 4a79fed98b\n+Subproject commit 4a79fed98b-dirty\n";
        assert!(submodule_moves(dirty).is_empty(), "a dirty submodule moves no pointer");
    }
}
