//! The filesystem side of the generator: resolve the crate's `src/`, write the new
//! command file, and idempotently insert the `pub mod <child>;` lines that make it
//! reachable from the crate root. Kept apart from the pure renderer so the rendering
//! stays unit-testable without touching disk, and the disk effects live in one place.
//!
//! These functions are synchronous (small text writes); the command wraps them in
//! `spawn_blocking` so the async runtime is never blocked on I/O.

use std::path::{Path, PathBuf};

use super::ident::CommandIdent;
use crate::sdk_codegen::CommandError;

/// Resolve the continuum-core `src/` directory the generator writes into.
///
/// Defaults to `<CARGO_MANIFEST_DIR>/src` — the crate this binary was built from,
/// which is the only tree where scaffolding a new core command makes sense (a
/// shipped binary on a user's machine has no source tree, and won't be asked to).
/// An explicit `override_root` (pointing at a `src/`) wins, for out-of-tree checkouts.
/// Fails loud if the resolved `commands/` dir is absent — never silently writes into
/// the wrong place.
pub fn resolve_src_root(override_root: Option<&str>) -> Result<PathBuf, CommandError> {
    let root = match override_root {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p.trim()),
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src"),
    };
    let commands = root.join("commands");
    if !commands.is_dir() {
        return Err(CommandError::Invalid(format!(
            "no commands/ dir under '{}' — pass a src root that contains commands/ \
             (the generator writes into the continuum-core source tree)",
            root.display()
        )));
    }
    Ok(root)
}

/// Write the rendered command file under `src_root`. Refuses to clobber an existing
/// file unless `force` — a command name collision is almost always a mistake, and
/// the registry's duplicate-name panic would catch it at boot anyway. Returns the
/// absolute path written.
pub fn write_command_file(
    src_root: &Path,
    id: &CommandIdent,
    content: &str,
    force: bool,
) -> Result<PathBuf, CommandError> {
    let path = src_root.join(&id.rel_file);
    if path.exists() && !force {
        return Err(CommandError::Invalid(format!(
            "{} already exists — pick a different name or pass force=true",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CommandError::Internal(format!("create {}: {e}", parent.display())))?;
    }
    std::fs::write(&path, content)
        .map_err(|e| CommandError::Internal(format!("write {}: {e}", path.display())))?;
    Ok(path)
}

/// Ensure every ancestor `mod.rs` declares the child module, creating a category
/// `mod.rs` where one doesn't exist yet. Idempotent: a `pub mod <child>;` already
/// present is left untouched. Returns the mod files that were created or edited.
pub fn ensure_mod_lines(src_root: &Path, id: &CommandIdent) -> Result<Vec<PathBuf>, CommandError> {
    let mut touched = Vec::new();
    for edit in id.mod_wiring() {
        let mod_path = src_root.join(&edit.mod_file);
        let line = format!("pub mod {};", edit.child);

        let existing = if mod_path.exists() {
            std::fs::read_to_string(&mod_path)
                .map_err(|e| CommandError::Internal(format!("read {}: {e}", mod_path.display())))?
        } else {
            // A new category dir: seed a minimal mod.rs with a doc header.
            let cat = edit
                .mod_file
                .parent()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "commands".to_string());
            format!("//! `{cat}/` — command family.\n\n")
        };

        if mod_has_decl(&existing, &edit.child) {
            continue;
        }
        let updated = insert_mod_line(&existing, &line);
        if let Some(parent) = mod_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CommandError::Internal(format!("create {}: {e}", parent.display())))?;
        }
        std::fs::write(&mod_path, updated)
            .map_err(|e| CommandError::Internal(format!("write {}: {e}", mod_path.display())))?;
        touched.push(mod_path);
    }
    Ok(touched)
}

/// Whether the file already declares `mod <child>` (with any visibility), so we
/// never duplicate a line. Matches `mod x;` / `pub mod x;` / `pub(crate) mod x;`.
fn mod_has_decl(src: &str, child: &str) -> bool {
    src.lines().any(|l| {
        let l = l.trim();
        l.strip_prefix("pub(crate) ")
            .or_else(|| l.strip_prefix("pub "))
            .unwrap_or(l)
            == format!("mod {child};")
    })
}

/// Insert a `pub mod x;` line, keeping the mod block tidy: after the last existing
/// `pub mod` declaration if there is one, otherwise after the leading doc/comment
/// header, otherwise at end. Preserves all other file content (helpers, family
/// `command_objects`, etc.).
fn insert_mod_line(src: &str, line: &str) -> String {
    let mut lines: Vec<String> = src.lines().map(|s| s.to_string()).collect();

    // After the last `pub mod` line.
    if let Some(idx) = lines
        .iter()
        .rposition(|l| l.trim_start().starts_with("pub mod ") || l.trim_start().starts_with("mod "))
    {
        lines.insert(idx + 1, line.to_string());
        return join_with_trailing_newline(&lines);
    }

    // No existing mod lines: after the leading comment/blank header.
    let header_end = lines
        .iter()
        .position(|l| !l.trim_start().starts_with("//") && !l.trim().is_empty())
        .unwrap_or(lines.len());
    let mut block = vec![line.to_string()];
    if header_end < lines.len() {
        block.push(String::new());
    }
    lines.splice(header_end..header_end, block);
    join_with_trailing_newline(&lines)
}

fn join_with_trailing_newline(lines: &[String]) -> String {
    let mut s = lines.join("\n");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an already-present declaration (any visibility) is detected,
    // so wiring is idempotent and never appends a duplicate `pub mod` line.
    #[test]
    fn detects_existing_declaration() {
        let src = "//! doc\n\npub mod code;\npub mod help;\n";
        assert!(mod_has_decl(src, "code"));
        assert!(mod_has_decl(src, "help"));
        assert!(!mod_has_decl(src, "data"));
        assert!(mod_has_decl("pub(crate) mod x;", "x"));
    }

    // what this catches: a new line is inserted into the existing mod block (after the
    // last pub mod), leaving the header and other content intact.
    #[test]
    fn inserts_into_existing_mod_block() {
        let src = "//! doc\n\npub mod code;\npub mod help;\n";
        let out = insert_mod_line(src, "pub mod data;");
        assert!(out.contains("pub mod data;"));
        assert!(out.contains("pub mod code;"));
        // Inserted right after the last pub mod, not before the header.
        let body = out.lines().collect::<Vec<_>>();
        let last_mod = body.iter().rposition(|l| l.starts_with("pub mod")).unwrap();
        assert_eq!(body[last_mod], "pub mod data;");
    }

    // what this catches: a file with no mod lines yet gets the declaration after its
    // doc header — the case where a brand-new category mod.rs is seeded.
    #[test]
    fn inserts_after_header_when_no_mods_exist() {
        let src = "//! `data/` — command family.\n\n";
        let out = insert_mod_line(src, "pub mod list;");
        assert!(out.contains("pub mod list;"));
        assert!(out.starts_with("//! `data/`"));
    }
}
