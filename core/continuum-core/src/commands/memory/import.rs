//! `memory/import` — bulk-import a directory of text/markdown files into a persona's
//! corpus, one memory per file. The REPRODUCIBLE primitive behind seeding an agent's
//! memory — it replaces the one-off migration shell loop with a first-class command, so
//! any agent can populate its corpus the same way ([[managed-product-everything-self-provisions-no-operator-steps]]).
//!
//! Reuses [`super::remember::build_agent_record`] so every imported memory carries the
//! IDENTICAL agent provenance a single `memory/remember` would (memory_type `agent`,
//! `source = agent:<peer>`, context, tags) — one record shape, one place (compression).

use std::path::Path;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::remember::build_agent_record;
use crate::log_info;
use crate::logging::TimingGuard;
use crate::memory::CorpusMemory;
use crate::modules::memory::MemoryState;
use crate::sdk_codegen::CommandError;

fn default_pattern() -> String {
    "*.md".to_string()
}
fn default_strip_frontmatter() -> bool {
    true
}
fn default_importance() -> f64 {
    0.6
}

/// Params for `memory/import`. Flat + CLI-friendly, mirroring `memory/remember` plus the
/// directory-scan inputs.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryImportParams.ts"
)]
pub struct MemoryImportParams {
    /// The corpus to import into (for an agent: its airc peer id).
    pub persona_id: String,
    /// Directory of files to import — one memory per matching file.
    pub source_dir: String,
    /// Project / room scope for the imported memories (recall `room_id` + a tag).
    pub scope: String,
    /// Filename glob — matches files ending with the pattern's tail (default `*.md` ⇒ `.md`).
    #[serde(default = "default_pattern")]
    pub pattern: String,
    /// Strip a leading YAML `---` frontmatter block, keeping `description:` + body so the
    /// fact stays semantically matchable (default true). No frontmatter ⇒ imported verbatim.
    #[serde(default = "default_strip_frontmatter")]
    pub strip_frontmatter: bool,
    /// The session that produced the import (traceability); `None` for a migration.
    #[serde(default)]
    #[ts(optional)]
    pub session: Option<String>,
    /// Importance (0..1) applied to every imported memory.
    #[serde(default = "default_importance")]
    pub importance: f64,
}

/// Counts from one import run.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/memory/ImportResult.ts")]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// Pure: turn a raw file's text into memory content. When `strip_frontmatter` and the file
/// opens with a YAML `---` block, drop the block but prepend its `description:` line to the
/// body (matchability). No/ malformed frontmatter, or stripping disabled ⇒ the text verbatim.
pub(super) fn extract_memory_content(raw: &str, strip_frontmatter: bool) -> String {
    if !strip_frontmatter || !raw.starts_with("---") {
        return raw.trim().to_string();
    }
    let mut lines = raw.lines();
    let _open = lines.next(); // the opening ---
    let mut frontmatter = String::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        frontmatter.push_str(line);
        frontmatter.push('\n');
    }
    if !closed {
        return raw.trim().to_string(); // malformed — import verbatim rather than lose it
    }
    let body = lines.collect::<Vec<_>>().join("\n");
    let description = frontmatter
        .lines()
        .find_map(|l| l.strip_prefix("description:").map(|d| d.trim().to_string()))
        .unwrap_or_default();
    if description.is_empty() {
        body.trim().to_string()
    } else {
        format!("{description}\n\n{}", body.trim())
    }
}

crate::action_command! {
    /// Bulk-import a directory of files into a persona's corpus, one memory per file — the
    /// reproducible way to seed an agent's memory. Reuses `memory/remember`'s record shaping
    /// and durable-first persistence. Returns imported / skipped / failed counts.
    pub struct MemoryImport { state: Arc<MemoryState> }
    name: "memory/import",
    access: AiSafe,
    params: MemoryImportParams,
    output: ImportResult,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_import");
        // Hydrate durable truth once before the batch so appends land on a warm corpus.
        super::hydrate_corpus_if_missing(&this.state, &p.persona_id).await?;

        let suffix = p.pattern.trim_start_matches('*').to_string();
        let entries = std::fs::read_dir(Path::new(&p.source_dir)).map_err(|e| {
            CommandError::Internal(format!("memory/import: cannot read {}: {e}", p.source_dir))
        })?;

        let mut imported = 0usize;
        let mut skipped = 0usize;
        let mut failed = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !suffix.is_empty() && !name.ends_with(&suffix) {
                continue;
            }
            let raw = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => {
                    failed += 1;
                    continue;
                }
            };
            let content = extract_memory_content(&raw, p.strip_frontmatter);
            if content.trim().is_empty() {
                skipped += 1;
                continue;
            }
            let id = uuid::Uuid::new_v4().to_string();
            let timestamp = chrono::Utc::now().to_rfc3339();
            let record = build_agent_record(
                &p.persona_id,
                content,
                &p.scope,
                p.session.clone(),
                p.importance,
                id,
                timestamp,
            );
            let memory = CorpusMemory { record, embedding: None };
            // Durable FIRST, cache second — exactly as memory/remember does.
            match super::persist_memory(&this.state, &p.persona_id, &memory).await {
                Ok(_) => {
                    if this
                        .state
                        .memory_manager
                        .append_memory(&p.persona_id, memory)
                        .is_ok()
                    {
                        imported += 1;
                    } else {
                        failed += 1;
                    }
                }
                Err(_) => failed += 1,
            }
        }

        log_info!(
            "module", "memory_import",
            "imported {imported} skipped {skipped} failed {failed} for {} from {}",
            p.persona_id, p.source_dir
        );
        Ok(ImportResult { imported, skipped, failed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: frontmatter stripping keeps description+body (semantically
    // matchable), returns raw text when there's no frontmatter, and honors strip=false —
    // the exact content extraction the corpus-seeding migration relied on.
    #[test]
    fn extract_strips_frontmatter_keeps_description_else_verbatim() {
        let raw = "---\nname: x\ndescription: the one-liner\n---\n\nThe fact body.\n";
        assert_eq!(
            extract_memory_content(raw, true),
            "the one-liner\n\nThe fact body."
        );
        // no frontmatter ⇒ verbatim (trimmed)
        assert_eq!(extract_memory_content("just text", true), "just text");
        // stripping disabled ⇒ verbatim (keeps the --- block)
        assert!(extract_memory_content(raw, false).starts_with("---"));
        // frontmatter with no description ⇒ just the body
        let no_desc = "---\nname: y\n---\n\nOnly a body.\n";
        assert_eq!(extract_memory_content(no_desc, true), "Only a body.");
    }
}
