//! `code/study` — she studies code she chooses: files or directories of her own workspace,
//! read at its committed HEAD, cut into whole-item spans and put into her `study` bucket
//! with her own hands. The bucket's trigger decides when they train and whether that grows
//! a gene she has or mints a new one, the same as for her lived turns. Nobody else calls it
//! for her: what she studies is her choice (Joel, 2026-10-11: genes are part of a mind,
//! never rigged by an agent or a user).

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::code::CodeState;
use crate::modules::code_commands::{ensure_engine, resolve_readable_path};
use crate::modules::dataset::Corpus;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// The bucket her studies go into: one trait, so the competence decision at job-create
/// separates what she studied by signature, never by a name she has to invent.
const STUDY_TRAIT: &str = "study";

pub struct CodeStudy {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/code/CodeStudyParams.ts")]
pub struct CodeStudyParams {
    /// Files or directories in your workspace to study, as you would pass them to code/read.
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/code/CodeStudyResult.ts")]
pub struct CodeStudyResult {
    /// `owner/name@sha`: what was read.
    pub source: String,
    /// How many passages went into your study bucket.
    #[ts(type = "number")]
    pub passages: usize,
    /// Files under your paths left out: not source or prose, secret-shaped, or not UTF-8.
    #[ts(type = "number")]
    pub skipped: usize,
    /// What the bucket did with them (filled toward its threshold, or dispatched training).
    pub outcome: String,
}

#[async_trait]
impl ActionCommand for CodeStudy {
    const NAME: &'static str = "code/study";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str = "Study code you choose: the files or directories you name in your workspace, \
         at its committed HEAD, become passages in your study bucket, and your genes learn them \
         when the bucket trains. Use it for code you keep needing and do not yet know well.";
    type Params = CodeStudyParams;
    type Output = CodeStudyResult;

    async fn run(&self, ctx: &Ctx, p: CodeStudyParams) -> Result<CodeStudyResult, CommandError> {
        let Some(persona) = ctx.caller.as_ref().map(|c| c.peer_id.as_uuid()) else {
            return Err(CommandError::Invalid(
                "code/study trains the caller's own genes; the operator has no study bucket".into(),
            ));
        };
        if p.paths.is_empty() {
            return Err(CommandError::Invalid("name the files or directories you want to study".into()));
        }
        let who = crate::modules::code_commands::caller_id(ctx);
        ensure_engine(&self.state, &who).await?;
        let root = self
            .state
            .file_engines
            .get(&who)
            .map(|e| e.workspace_root())
            .ok_or_else(|| CommandError::Internal("workspace vanished after provisioning".into()))?;
        let root = root.canonicalize().map_err(|e| CommandError::Internal(format!("your workspace: {e}")))?;
        // every path through the one resolver her reads use: her scope, refused by name outside it
        let mut relative = Vec::with_capacity(p.paths.len());
        for path in &p.paths {
            let absolute = resolve_readable_path(&self.state, ctx, path).await?;
            let inside = absolute.strip_prefix(&root).map_err(|_| {
                CommandError::Invalid(format!("{path} is not in your workspace: study reads your workspace's committed source"))
            })?;
            relative.push(inside.to_string_lossy().into_owned());
        }
        let base_model = crate::inference::llama_server::current_serving()
            .active_model
            .ok_or_else(|| CommandError::Invalid("no model is being served here, so there is no base to study on".into()))?;
        let persona_name = crate::persona::PersonaAircRuntimeRegistry::try_global()
            .and_then(|r| r.get(persona))
            .map(|r| r.agent_name().to_string())
            .ok_or_else(|| CommandError::Invalid("your runtime is not registered yet; try again once you are seated".into()))?;
        let corpus = tokio::task::spawn_blocking(move || {
            Corpus::read(&root, "HEAD", &relative, &[], crate::modules::dataset::default_example_tokens())
        })
        .await
        .map_err(|e| CommandError::Internal(format!("reading the source did not finish: {e}")))?
        .map_err(CommandError::Invalid)?;
        let source = format!("{}@{}", corpus.label, corpus.sha);
        let passages = corpus.examples.len();
        let skipped = corpus.skipped;
        let examples: Vec<serde_json::Value> = corpus
            .examples
            .into_iter()
            .map(|(prompt, completion)| {
                serde_json::json!({ "prompt": prompt, "completion": completion,
                    "metadata": { "source": "study", "repo": source, "domain": STUDY_TRAIT } })
            })
            .collect();
        let params = serde_json::json!({
            "personaId": persona,
            "personaName": persona_name,
            "baseModel": base_model,
            "traitKind": STUDY_TRAIT,
            "examples": examples,
            "source": "studied",
        });
        let receipt = crate::persona::training_producer::submit_as(persona, params)
            .await
            .map_err(CommandError::Internal)?;
        if !receipt.success {
            return Err(CommandError::Invalid(format!(
                "your study bucket refused the passages: {}",
                receipt.error.as_deref().unwrap_or("no reason given") // unwrap_or: a refusal without a message still refuses
            )));
        }
        crate::probe!(
            class = "genome.study.submitted",
            persona = %persona,
            source = %source,
            passages = passages,
            "she chose to study source; it went into her study bucket"
        );
        Ok(CodeStudyResult {
            source,
            passages,
            skipped,
            outcome: receipt.outcome.unwrap_or_else(|| "accepted".into()), // unwrap_or_else: an accepting receipt without an outcome label is still an acceptance
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: study run for the operator, who has no genes of his own: it would
    // file passages under a nil persona. Only an identified caller studies, into her bucket.
    #[tokio::test]
    async fn the_operator_has_no_study_bucket() {
        let state = CodeState::new(
            Arc::new(dashmap::DashMap::new()),
            Arc::new(dashmap::DashMap::new()),
            tokio::runtime::Handle::current(),
        );
        let cmd = CodeStudy { state: Arc::new(state) };
        let refused = cmd.run(&Ctx::default(), CodeStudyParams { paths: vec!["src".into()] }).await;
        assert!(matches!(refused, Err(CommandError::Invalid(m)) if m.contains("own genes")));
    }
}
