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
    /// The airc peer whose lane serves `model_id`, when her brain is to run OFF-BOX.
    ///
    /// Omit it (the default) and the assignment is local: `serving/pin` fit-gates the
    /// model on THIS host and refuses loud if it is unknown, not downloaded, or won't
    /// fit — unchanged behaviour.
    ///
    /// Pass it and the local fit-gate is deliberately SKIPPED, because the point is
    /// that this host cannot serve the model: the named peer does. That is the whole
    /// value for a node below the cognition floor — measured on IntelMac 2026-09-06,
    /// whose only local model returns 39–42 character bare tool calls with an empty
    /// intent, while a cross-grid `ai/generate` addressed to a citizen on another node
    /// answered in 409 ms from that node's adapter.
    ///
    /// This is the DURABLE half only. Materialising her adapter as an
    /// `AircRemoteInferenceAdapter` pinned to this peer happens where the allocator
    /// reads the override (`persona/allocator.rs`, via `commands/persona/allocate.rs`)
    /// and is the next slice of card `1d2f65e7`. Until that lands, the record persists
    /// and the allocator still resolves her locally — so this flag is inert rather
    /// than half-wired, and `models_remote` in the report says so.
    #[serde(default)]
    pub remote_peer: Option<String>,
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
    /// The peer serving her model when the assignment is OFF-BOX; `None` for a
    /// local assignment. Present so a caller can tell the two apart without
    /// re-reading the override — a remote assignment did NOT fit-gate this host and
    /// did NOT pin anything here, so `previous_model` is meaninglessly `None` for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remote_peer: Option<String>,
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
        //
        //    SKIPPED ENTIRELY for a remote assignment. The fit-gate asks "can THIS
        //    host hold it", and for `remote_peer` the answer is expected to be no —
        //    that is the reason the peer was named. Running it anyway would refuse
        //    every off-box assignment on exactly the nodes that need one. This is a
        //    deliberate bypass of a gate, not a fallback: the local path keeps its
        //    gate unchanged, and the remote path is a different question that this
        //    gate cannot answer ([[substrate-gate-vs-persona-cognition]]).
        let (pin_composed, previous_model) = match p.remote_peer.as_deref() {
            None => {
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
                let previous = pin_value
                    .get("previous_model")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                (true, previous)
            }
            Some(peer) => {
                // The peer id must be a peer UUID. Refuse a malformed one HERE rather
                // than persisting a record that can never resolve to a route — the
                // same reason the airc interceptor refuses a bad `aircPeer` by name
                // in ~1ms instead of timing out at 30s.
                if uuid::Uuid::parse_str(peer).is_err() {
                    return Err(CommandError::Invalid(format!(
                        "remote_peer '{peer}' is not a peer UUID — pass the peer id of a \
                         citizen or node that serves '{}'. Nothing was persisted.",
                        p.model_id
                    )));
                }
                (false, None)
            }
        };

        // 3. The model is proven servable and now pinned — persist the durable
        //    per-persona assignment. A disk failure here leaves the host live on the
        //    model but the record un-persisted: fail loud and say exactly that, so
        //    the operator knows it won't survive a restart (we do NOT silently
        //    unpin — that would hide the disk fault behind a "reverted" lie).
        let override_record = match p.remote_peer.as_deref() {
            None => PersonaModelOverride::new(p.model_id.clone(), p.set_by.clone(), now_ms()),
            Some(peer) => PersonaModelOverride::new_remote(
                p.model_id.clone(),
                p.set_by.clone(),
                now_ms(),
                peer,
            ),
        };
        override_record.write(&home).map_err(|e| {
            CommandError::Internal(format!(
                "host is now serving '{}' for '{}' but persisting her durable assignment failed: {e}. \
                 The reassignment is LIVE this session but will NOT survive a restart — fix the disk \
                 error and re-run persona/reassign-model.",
                p.model_id, p.persona
            ))
        })?;

        let detail = match p.remote_peer.as_deref() {
            Some(peer) => format!(
                "'{}' is assigned '{}' served by peer {peer} — her brain runs OFF-BOX. \
                 This host was NOT fit-gated and is not serving it. Persisted for next boot. \
                 NOTE: adapter materialisation is the next slice of card 1d2f65e7; until it \
                 lands the allocator still resolves her locally, so this record is durable \
                 but not yet load-bearing.",
                p.persona, p.model_id
            ),
            None => match &previous_model {
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
            },
        };

        let _ = pin_composed;

        Ok(ReassignModelReport {
            persona: p.persona,
            model_id: p.model_id,
            previous_model,
            override_persisted: true,
            remote_peer: p.remote_peer,
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
                    remote_peer: None,
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
                    remote_peer: None,
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
    // what this catches: a REMOTE assignment must not be fit-gated by this host.
    // Proven by the absence of an executor — `missing_executor_fails_loud_and_persists_nothing`
    // above shows a LOCAL reassign fails Internal when the compose seam is missing,
    // because it must reach `serving/pin`. The same call with `remote_peer` set
    // SUCCEEDS on the same bare command, which is only possible if the pin was never
    // attempted. That is the fit-gate skip, tested by consequence rather than by
    // asserting on an internal branch.
    //
    // Card 1d2f65e7. The gate asks "can THIS host hold it"; for an off-box assignment
    // the answer is expected to be no, so running it would refuse every remote
    // assignment on exactly the nodes that need one (IntelMac, whose only local model
    // is below the cognition floor).
    #[tokio::test]
    async fn a_remote_assignment_skips_the_local_fit_gate_and_records_the_peer() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let home = resolve_home(&root, "Asha").expect("home resolves");
        home.ensure_exists().expect("mkdir home");

        let peer = "e5f4141d-1d95-4d62-8c19-97b5f8320837";
        let cmd = cmd_with_root(root);
        let report = cmd
            .run(
                &Ctx::default(),
                PersonaReassignModelParams {
                    persona: "Asha".to_string(),
                    model_id: "ornith-ai/Ornith-1.5-35B-A3B-GGUF".to_string(),
                    set_by: Some("operator".to_string()),
                    remote_peer: Some(peer.to_string()),
                },
            )
            .await
            .expect("a remote assignment must not need a local serving pin");

        assert_eq!(report.remote_peer.as_deref(), Some(peer));
        assert!(report.override_persisted);
        assert!(
            report.previous_model.is_none(),
            "nothing was pinned here, so there is no previous local model: {:?}",
            report.previous_model
        );

        let persisted = PersonaModelOverride::load(&home)
            .expect("load")
            .expect("a remote assignment persists an override");
        assert_eq!(persisted.remote_peer.as_deref(), Some(peer));
        assert!(persisted.is_remote());
        assert_eq!(persisted.model_id, "ornith-ai/Ornith-1.5-35B-A3B-GGUF");
    }

    // what this catches: a peer id that can never resolve to a route being written to
    // disk, where it would fail later at adapter-materialisation time with no clue
    // where it came from. Refused up front and NOTHING is persisted — the same shape
    // as the airc interceptor refusing a malformed `aircPeer` by name in ~1ms instead
    // of letting it become a 30s timeout (measured on IntelMac, four runs, 2026-09-06).
    #[tokio::test]
    async fn a_malformed_remote_peer_is_refused_before_anything_is_persisted() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let home = resolve_home(&root, "Asha").expect("home resolves");
        home.ensure_exists().expect("mkdir home");

        let cmd = cmd_with_root(root);
        let err = cmd
            .run(
                &Ctx::default(),
                PersonaReassignModelParams {
                    persona: "Asha".to_string(),
                    model_id: "ornith-ai/Ornith-1.5-35B-A3B-GGUF".to_string(),
                    set_by: None,
                    remote_peer: Some("not-a-uuid".to_string()),
                },
            )
            .await
            .expect_err("a non-uuid peer must be refused");
        assert!(matches!(err, CommandError::Invalid(_)), "got {err:?}");
        assert!(
            err.to_string().contains("not-a-uuid"),
            "the refusal must name the bad value: {err}"
        );
        assert!(
            PersonaModelOverride::load(&home).expect("load").is_none(),
            "a refused remote assignment must persist nothing"
        );
    }

}
