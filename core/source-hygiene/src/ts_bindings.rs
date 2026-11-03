//! Every committed TypeScript binding is one a Rust type still exports.
//!
//! ts-rs writes `protocol/typescript/<module>/<Type>.ts` when a crate's tests run, and the CI drift
//! step (`git diff --exit-code protocol/typescript/`) catches a regenerated binding that changed.
//! A binding whose type was DELETED is never regenerated, so it stays committed behind a green
//! check, with its barrel line, and every TypeScript client still sees the dead type. On 2026-11-03,
//! 38 such files were committed (the threat classifier's, the cargo tool's, the base-model ladder's),
//! and #5726 was about to add six more. This rule counts them; the count may only fall.

use std::collections::BTreeSet;
use std::path::Path;

/// The `protocol/typescript/`-relative files a Rust source's `export_to` attributes name.
/// A path ending in `/` is ts-rs's directory form: the file is the next item's name plus `.ts`.
pub fn exported_bindings(source: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = source;
    while let Some(at) = rest.find("export_to") {
        rest = &rest[at + "export_to".len()..];
        let Some(literal) = rest.trim_start().strip_prefix('=').map(str::trim_start) else { continue };
        let Some(literal) = literal.strip_prefix('"') else { continue };
        let Some(end) = literal.find('"') else { continue };
        let path = &literal[..end];
        let Some((_, rel)) = path.split_once("protocol/typescript/") else { continue };
        match rel.ends_with('/') || rel.is_empty() {
            false => {
                out.insert(rel.to_string());
            }
            true => {
                if let Some(name) = next_item_name(&literal[end..]) {
                    out.insert(format!("{rel}{name}.ts"));
                }
            }
        }
    }
    out
}

/// The name of the first `struct`, `enum` or `type` item after an attribute.
fn next_item_name(after: &str) -> Option<&str> {
    let mut words = after.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty());
    while let Some(word) = words.next() {
        if matches!(word, "struct" | "enum" | "type") {
            return words.next();
        }
    }
    None
}

/// Committed bindings no Rust type exports. Barrels (`index.ts`) are kept in step with their
/// directory by `continuum-core`'s `generated_barrel_sync`, so they are not bindings here.
pub fn orphans(committed: &BTreeSet<String>, exported: &BTreeSet<String>) -> Vec<String> {
    committed.iter().filter(|f| !f.ends_with("index.ts") && !exported.contains(*f)).cloned().collect()
}

/// Every `.rs` file under `dir`, skipping build output and vendored trees.
fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !matches!(name.to_str(), Some("target" | "vendor" | "node_modules" | ".git")) {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every `.ts` file under `protocol/typescript/`, relative to it.
fn committed_bindings(root: &Path) -> BTreeSet<String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, out);
            } else if path.extension().is_some_and(|e| e == "ts") {
                if let Ok(rel) = path.strip_prefix(base) {
                    out.insert(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    let base = root.join("protocol/typescript");
    let mut out = BTreeSet::new();
    walk(&base, &base, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the parser missing a form ts-rs accepts, which would report a live
    // binding as an orphan (and invite deleting it) or hide a real one.
    #[test]
    fn both_export_forms_are_read() {
        let src = r#"
            #[ts(export, export_to = "../../../protocol/typescript/rag/LlmMessage.ts")]
            pub struct LlmMessage;
            #[ts(export, export_to = "../../../protocol/typescript/genome/")]
            #[serde(rename_all = "camelCase")]
            pub enum Tier { A }
            #[ts(export, export_to = "bindings/Local.ts")]
            pub struct Local;
        "#;
        let got = exported_bindings(src);
        let want: BTreeSet<String> = ["rag/LlmMessage.ts", "genome/Tier.ts"].map(String::from).into();
        assert_eq!(got, want);
        let committed: BTreeSet<String> = ["rag/LlmMessage.ts", "rag/index.ts", "rag/RagContext.ts"].map(String::from).into();
        assert_eq!(orphans(&committed, &got), vec!["rag/RagContext.ts".to_string()]);
    }

    // what this catches: a deleted ts-rs type leaving its binding committed (the drift step
    // cannot see it). Each deletion lowers the count; a new orphan raises it and fails.
    #[test]
    fn no_committed_binding_outlives_its_type() {
        let root = crate::repo_root().expect("the audited tree has a workspace root");
        let mut sources = Vec::new();
        for top in ["core", "client", "apps"] {
            rust_files(&root.join(top), &mut sources);
        }
        assert!(!sources.is_empty(), "no Rust sources under {}: the scan read nothing", root.display());
        let exported: BTreeSet<String> = sources
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .flat_map(|s| exported_bindings(&s))
            .collect();
        let committed = committed_bindings(&root);
        assert!(!committed.is_empty(), "no bindings under protocol/typescript: the scan read nothing");
        let found = orphans(&committed, &exported);
        for f in found.iter().take(8) {
            eprintln!("  orphan binding: protocol/typescript/{f}");
        }
        crate::ratchet::hold("orphan_ts_bindings", found.len(), || {
            "A committed protocol/typescript binding has no Rust type exporting it. When you delete \
             a ts-rs type, delete its .ts file and its barrel line in the same change."
                .to_string()
        });
    }
}
