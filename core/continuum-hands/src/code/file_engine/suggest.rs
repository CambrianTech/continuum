//! The nearest existing paths for a path that was not found.
use super::*;

/// Directories never worth walking to suggest a path — they are enormous, they
/// are never the file she meant, and on SOMEONE ELSE'S COMPUTER walking them is
/// the difference between a helpful error and a stalled laptop.
pub(super) const SUGGEST_SKIP_DIRS: &[&str] = &[
    ".git", "node_modules", "target", "__pycache__", ".venv", "venv", "build",
    "dist", ".tox", ".mypy_cache", ".pytest_cache", ".cargo", "site-packages",
];

/// Hard ceiling on entries visited while looking for a near-miss. A suggestion
/// is a courtesy; it must never become the expensive part of an error path.
pub(super) const SUGGEST_MAX_ENTRIES: usize = 20_000;
pub(super) const SUGGEST_MAX_DEPTH: usize = 12;
pub(super) const SUGGEST_MAX_RESULTS: usize = 3;

/// Bounded hunt for what she probably MEANT, ranked best-first.
///
/// Why this exists (glass-boxed 2026-08-28, the five no-candidate-patch solve
/// runs): a citizen spent 30 acts on django-15252, found the correct source file
/// on her FIRST act, and still produced no diff — because 3 of her 8 reads were
/// paths that do not exist (`django/db/backends/creation.py` missing the `base/`
/// segment, `django/migration/commands/migrate.py` for `migrations`). Each miss
/// cost a full act out of a 30-act budget and returned nothing but
/// "File not found", so nothing pushed her back on track and she ran out of
/// budget before writing. The substrate already does exactly this for mistyped
/// COMMAND names ("did you mean"); a mistyped PATH deserves the same courtesy,
/// and it is recoverable from the tree she is standing in.
///
/// Ranking mirrors how a person recovers: same file name somewhere else in the
/// tree first (the `base/` case), then a near-miss on the name itself (the
/// `migration`/`migrations` case).
pub(super) fn suggest_nearest_paths(root: &Path, wanted: &str) -> Vec<String> {
    let wanted_name = Path::new(wanted)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if wanted_name.is_empty() {
        return Vec::new();
    }
    let mut exact_name: Vec<String> = Vec::new();
    let mut near_name: Vec<String> = Vec::new();
    let mut visited = 0usize;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > SUGGEST_MAX_DEPTH || visited >= SUGGEST_MAX_ENTRIES {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited >= SUGGEST_MAX_ENTRIES {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                if !name.starts_with('.') && !SUGGEST_SKIP_DIRS.contains(&name.as_str()) {
                    stack.push((entry.path(), depth + 1));
                }
                continue;
            }
            let lower = name.to_lowercase();
            let rel = entry
                .path()
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().to_string())
                .unwrap_or(name.clone());
            if lower == wanted_name {
                if exact_name.len() < SUGGEST_MAX_RESULTS {
                    exact_name.push(rel);
                }
            } else if near_name.len() < SUGGEST_MAX_RESULTS
                && (lower.starts_with(&wanted_name) || wanted_name.starts_with(&lower))
            {
                near_name.push(rel);
            }
        }
        if exact_name.len() >= SUGGEST_MAX_RESULTS {
            break;
        }
    }
    exact_name.extend(near_name);
    exact_name.truncate(SUGGEST_MAX_RESULTS);
    exact_name
}
