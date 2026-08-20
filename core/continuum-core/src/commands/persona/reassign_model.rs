//! `persona/reassign-model` — assign a persona a new base model, durably, and make
//! it real on this host right now.
//!
//! ## The concern this owns
//!
//! "Run Asha on the 14B coder from now on." That is two facts that must move
//! together: a *durable per-persona* preference (so she resumes on that model next
//! boot) and a *live host* change (so she is on it this session, not after a
//! restart). This verb writes the first and triggers the second — atomically
//! enough that you can never persist an assignment the host can't actually serve.
//!
//! ## How it composes (commands from commands)
//!
//! It does not re-implement the fit math or the serving swap. It **invokes
//! [`serving/pin`](crate::commands::serving::pin)** through the substrate executor:
//!
//! 1. Resolve the persona's [`PersonaHome`] from `continuum_root` + her agent name.
//!    Unknown persona ⇒ [`CommandError::NotFound`] (a typo never mints a stray
//!    override dir).
//! 2. Compose `serving/pin {model_id}` — its fit-gate refuses a model that is
//!    unknown, not downloaded, or won't fit a lane in the current budget. If the
//!    pin is refused, we propagate it loud and **persist nothing** — the
//!    reassignment is rejected as a whole, never silently downgraded.
//! 3. Only after the pin proves the model servable do we write her
//!    [`PersonaModelOverride`] — the durable record the allocator reads at the
//!    highest precedence next boot.
//!
//! This is the per-persona dual of the host-level pin: `serving/pin` says "this
//! *host* serves model Y"; the override says "this *persona* is assigned model Y".
//! `persona/reassign-model` is the verb that sets both, in the safe order.
//!
//! ## Single-serve honesty
//!
//! On a single-serve host (one supervised `llama-server`) the pin re-homes the
//! shared base for everyone on the node — per-persona base divergence arrives with
//! multi-base serving, which this verb will compose unchanged. The *override* is
//! already per-persona and durable today, so the moment multi-base serving lands,
//! each persona resumes on her own assigned base with no change here.
//!
//! ## Gating
//!
//! `Privileged` — it dictates GPU residency (via the pin) and rewrites a citizen's
//! durable base-model assignment. It also composes `serving/pin`, which is itself
//! `Privileged`; an `AiSafe` surface here would fail the inner gate, so the levels
//! are kept consistent.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::context::citizen_home_path;
use crate::identity::IdentityKind;
use crate::persona::home::PersonaHome;
use crate::persona::PersonaModelOverride;
use crate::runtime::{CommandExecutor, LateBound};
use crate::sdk_codegen::CommandError;

#[cfg(test)]
use crate::sdk_codegen::Ctx;

/// Which persona to reassign, and to which base model.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaReassignModelParams.ts"
)]
pub struct PersonaReassignModelParams {
    /// The persona's agent name as it appears on disk (e.g. `"Asha"`) — the
    /// `<name>` segment of her home dir. Fails loud if no such persona has a home.
    pub persona: String,
    /// The base model id to assign, as it appears in `models/list`. Must be
    /// downloaded and must fit a serving lane on this host — `serving/pin`'s
    /// fit-gate refuses it loud otherwise, and nothing is persisted.
    pub model_id: String,
    /// Who is making the assignment: an operator user-id, or the persona's own id
    /// when she reassigns herself as a tool. Recorded on the override for audit.
    #[serde(default)]
    pub set_by: Option<String>,
}

/// What `persona/reassign-model` did: the durable assignment that now sticks, and
/// the live host change that backs it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ReassignModelReport.ts"
)]
pub struct ReassignModelReport {
    /// The persona reassigned — echoed so the caller can confirm.
    pub persona: String,
    /// The base model she is now assigned to (and pinned on this host).
    pub model_id: String,
    /// What the host was serving before the pin (`None` if nothing was live) — the
    /// promote/demote "from", surfaced from the composed `serving/pin` report.
    pub previous_model: Option<String>,
    /// `true` once the durable per-persona override is written — i.e. the
    /// assignment will survive a restart, not just this session.
    pub override_persisted: bool,
    /// Human-readable summary.
    pub detail: String,
}

/// Resolve a persona's home from the substrate root + her agent name.
///
/// [`citizen_home_path`] returns her `airc/` subdir
/// (`…/citizens/personas/<name>/airc`); her home root is its parent
/// (`…/citizens/personas/<name>/`), which is what [`PersonaHome`] wraps and where
/// `model_override.json` lives alongside `seed.json` + `engrams.sqlite`.
fn resolve_home(continuum_root: &Path, persona: &str) -> Option<PersonaHome> {
    let airc_dir = citizen_home_path(continuum_root, IdentityKind::Persona, None, persona);
    airc_dir
        .parent()
        .map(|root| PersonaHome::from_root(root.to_path_buf()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

crate::action_command! {
    /// Assign a persona a new base model durably AND make it live on this host now.
    /// Composes serving/pin: resolves her home, fit-gates + force-serves the model
    /// (failing loud and persisting nothing if it is unknown, not downloaded, or
    /// won't fit), then writes her durable per-persona override so she resumes on it
    /// next boot. The per-persona dual of serving/pin — the override binds the
    /// persona, the pin binds the host.
    pub struct PersonaReassignModel {
        continuum_root: PathBuf,
        executor: Arc<LateBound<CommandExecutor>>,
    }
    name: "persona/reassign-model",
    access: Privileged,
    params: PersonaReassignModelParams,
    output: ReassignModelReport,
    run(this, _ctx, p) => {
        // 1. Resolve her home FIRST — a typo must never reach the serving layer or
        //    mint a stray override dir. NotFound names where we looked.
        let home = resolve_home(&this.continuum_root, &p.persona).ok_or_else(|| {
            CommandError::Internal(format!(
                "could not resolve a home path for persona '{}' under {}",
                p.persona,
                this.continuum_root.display()
            ))
        })?;
        if !home.root().exists() {
            return Err(CommandError::NotFound(format!(
                "no persona named '{}' has a home at {} — call persona/instances/list to see who exists",
                p.persona,
                home.root().display()
            )));
        }

        // 2. Compose serving/pin — its fit-gate is the single source of "can this
        //    host actually serve that model". If it refuses, the reassignment is
        //    refused as a whole and NOTHING is persisted (no silent downgrade).
        let executor = this
            .executor
            .require()
            .map_err(CommandError::Internal)?
            .clone();
        let pin_outcome = executor
            .execute(
                "serving/pin",
                serde_json::json!({ "model_id": p.model_id }),
            )
            .await;
        let pin_value = match pin_outcome {
            Ok(result) => result.to_json_value().map_err(CommandError::Internal)?,
            Err(e) => {
                return Err(CommandError::Denied(format!(
                    "cannot reassign '{}' to '{}': serving/pin refused it — {e}. \
                     Nothing was changed; the persona keeps her current model.",
                    p.persona, p.model_id
                )));
            }
        };
        let previous_model = pin_value
            .get("previous_model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 3. The model is proven servable and now pinned — persist the durable
        //    per-persona assignment. A disk failure here leaves the host live on the
        //    model but the record un-persisted: fail loud and say exactly that, so
        //    the operator knows it won't survive a restart (we do NOT silently
        //    unpin — that would hide the disk fault behind a "reverted" lie).
        let override_record =
            PersonaModelOverride::new(p.model_id.clone(), p.set_by.clone(), now_ms());
        override_record.write(&home).map_err(|e| {
            CommandError::Internal(format!(
                "host is now serving '{}' for '{}' but persisting her durable assignment failed: {e}. \
                 The reassignment is LIVE this session but will NOT survive a restart — fix the disk \
                 error and re-run persona/reassign-model.",
                p.model_id, p.persona
            ))
        })?;

        let detail = match &previous_model {
            Some(prev) if prev == &p.model_id => format!(
                "'{}' was already serving '{}'; pin held and her durable assignment is now recorded",
                p.persona, p.model_id
            ),
            Some(prev) => format!(
                "reassigned '{}' to '{}' (host was serving '{}'); pinned now and persisted for next boot",
                p.persona, p.model_id, prev
            ),
            None => format!(
                "reassigned '{}' to '{}' (nothing was serving); pinned now and persisted for next boot",
                p.persona, p.model_id
            ),
        };

        Ok(ReassignModelReport {
            persona: p.persona,
            model_id: p.model_id,
            previous_model,
            override_persisted: true,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    fn cmd_with_root(root: PathBuf) -> PersonaReassignModel {
        PersonaReassignModel {
            continuum_root: root,
            // An empty LateBound: the executor is never reached by the tests below
            // (they short-circuit at the home-existence gate before the compose).
            executor: Arc::new(LateBound::new("test::executor")),
        }
    }

    // what this catches: the wire name mirrors the file path — the routing contract
    // that lets the typed registry dispatch `persona/reassign-model` to this command.
    #[test]
    fn name_mirrors_path() {
        assert_eq!(PersonaReassignModel::NAME, "persona/reassign-model");
        assert!(matches!(
            PersonaReassignModel::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: reassigning a persona who has no home on disk fails loud as
    // NotFound BEFORE any serving change or override write — a typo'd name can never
    // pin the host or mint a stray override dir. This runs without a live executor,
    // proving the home gate precedes the compose.
    #[tokio::test]
    async fn unknown_persona_is_not_found_before_any_change() {
        let tmp = tempfile::tempdir().unwrap();
        let cmd = cmd_with_root(tmp.path().to_path_buf());
        let err = cmd
            .run(
                &Ctx::default(),
                PersonaReassignModelParams {
                    persona: "Nonesuch".to_string(),
                    model_id: "qwen3-coder-14b".to_string(),
                    set_by: None,
                },
            )
            .await
            .expect_err("a persona with no home must fail loud");
        assert!(matches!(err, CommandError::NotFound(_)), "got {err:?}");
    }

    // what this catches: when the persona DOES exist but the compose seam (the
    // executor) is not installed, we fail loud Internal and persist NOTHING — the
    // override file is never written when the pin couldn't even be attempted. Proves
    // the override is gated on a real serving outcome, never written speculatively.
    #[tokio::test]
    async fn missing_executor_fails_loud_and_persists_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        // Create the persona's home so we get past the existence gate.
        let home = resolve_home(&root, "Asha").expect("home resolves");
        home.ensure_exists().expect("mkdir home");

        let cmd = cmd_with_root(root);
        let err = cmd
            .run(
                &Ctx::default(),
                PersonaReassignModelParams {
                    persona: "Asha".to_string(),
                    model_id: "qwen3-coder-14b".to_string(),
                    set_by: Some("operator".to_string()),
                },
            )
            .await
            .expect_err("no executor installed must fail loud");
        assert!(matches!(err, CommandError::Internal(_)), "got {err:?}");
        // The override must NOT have been written — the pin was never attempted.
        assert!(
            PersonaModelOverride::load(&home).expect("load").is_none(),
            "no override may be persisted when the serving pin couldn't be attempted"
        );
    }
}
