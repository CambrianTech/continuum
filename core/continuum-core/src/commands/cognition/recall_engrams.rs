//! `cognition/recall-engrams` — query a persona's admitted-engram store (typed,
//! dep-holding).
//!
//! Four recall modes, selected by `kind`:
//!   - `recent` (default) + `limit`      → newest-first N engrams
//!   - `by_id` + `id`                    → exact lookup by uuid
//!   - `by_keyword` + `keyword` + `limit`→ case-insensitive substring match
//!   - `by_origin` + `origin` + `limit`  → filter by origin (chat|airc|tool|self_reflection)
//!
//! Each mode's required field is validated inside its branch — a missing `id` on a
//! `by_id` recall (etc.) fails loud, no silent default. Captures the owning module's
//! [`CognitionState`](crate::modules::cognition::CognitionState). Read-only: `NotFound`
//! when the persona has no cognition engine.
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::{Engram, EngramOriginKind};
use crate::sdk_codegen::CommandError;

fn default_kind() -> String {
    "recent".to_string()
}

fn default_limit() -> usize {
    10
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/RecallEngramsParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecallEngramsParams {
    /// Persona whose engram store is queried.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Recall mode: `recent` | `by_id` | `by_keyword` | `by_origin` (default `recent`).
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Max engrams to return (ignored by `by_id`; default 10).
    #[serde(default = "default_limit")]
    #[ts(type = "number")]
    pub limit: usize,
    /// Exact engram id — required when `kind = by_id`.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub id: Option<Uuid>,
    /// Substring to match — required when `kind = by_keyword`.
    #[serde(default)]
    #[ts(optional)]
    pub keyword: Option<String>,
    /// Origin filter (`chat` | `airc` | `tool` | `self_reflection`) — required when
    /// `kind = by_origin`.
    #[serde(default)]
    #[ts(optional)]
    pub origin: Option<String>,
}

/// The recalled engrams and their count.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/RecallEngramsResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecallEngramsResult {
    pub engrams: Vec<Engram>,
    #[ts(type = "number")]
    pub count: usize,
}

crate::action_command! {
    /// Query the persona's admitted-engram store by recency, id, keyword, or origin.
    /// Host-invoked.
    pub struct RecallEngrams { state: Arc<CognitionState> }
    name: "cognition/recall-engrams",
    access: Internal,
    params: RecallEngramsParams,
    output: RecallEngramsResult,
    run(this, _ctx, p) => {
        // LIVE personas register their mind in `persona_workspace::global()` (the
        // production spawn path); the `CognitionState` map is the IPC-era registry.
        // Live-first, IPC-fallback — the SAME resolution `forget-context` and
        // `redact-memory` use. Without this, recall-engrams returned "No cognition"
        // for a persona that `cognition/personas` lists as resident (glass-boxed
        // 2026-07-17: the eval fork and the learn-mode transfer both found her via
        // the live registry, but this command only looked in the IPC map).
        let live = crate::cognition::persona_workspace::global()
            .get(&p.persona_id)
            .and_then(|cycle| cycle.acting().map(|a| a.admission.clone()));
        let admission = match live {
            Some(a) => a,
            None => this
                .state
                .personas
                .get(&p.persona_id)
                .map(|persona| persona.admission.clone())
                .ok_or_else(|| {
                    CommandError::NotFound(format!("No cognition for {}", p.persona_id))
                })?,
        };

        let engrams = match p.kind.as_str() {
            "recent" => admission.recall_recent(p.limit),
            "by_id" => {
                let id = p
                    .id
                    .ok_or_else(|| CommandError::Invalid("by_id recall requires 'id'".into()))?;
                admission.recall_by_id(id).into_iter().collect()
            }
            "by_keyword" => {
                let keyword = p.keyword.as_deref().ok_or_else(|| {
                    CommandError::Invalid("by_keyword recall requires 'keyword'".into())
                })?;
                admission.recall_by_keyword(keyword, p.limit)
            }
            "by_origin" => {
                let origin_str = p.origin.as_deref().ok_or_else(|| {
                    CommandError::Invalid("by_origin recall requires 'origin'".into())
                })?;
                let origin_kind = match origin_str {
                    "chat" => EngramOriginKind::Chat,
                    "airc" => EngramOriginKind::Airc,
                    "tool" => EngramOriginKind::Tool,
                    "self_reflection" => EngramOriginKind::SelfReflection,
                    "agent" => EngramOriginKind::Agent,
                    other => {
                        return Err(CommandError::Invalid(format!(
                            "unknown origin kind '{other}'; expected one of: \
                             chat, airc, tool, self_reflection, agent"
                        )))
                    }
                };
                admission.recall_by_origin_kind(origin_kind, p.limit)
            }
            other => {
                return Err(CommandError::Invalid(format!(
                    "unknown recall kind '{other}'; expected one of: \
                     recent, by_id, by_keyword, by_origin"
                )))
            }
        };

        let count = engrams.len();
        Ok(RecallEngramsResult { engrams, count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. recall-engrams is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(RecallEngrams::NAME, "cognition/recall-engrams");
        assert_eq!(RecallEngrams::ACCESS, AccessLevel::Internal);
    }
}
