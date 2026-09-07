//! The REMOTE-LANE adapter factory — slice 2 of card `1d2f65e7`.
//!
//! Slice 1 (#3799) made `persona/reassign-model --remote-peer` write a durable
//! [`PersonaModelOverride`] carrying `remote_peer`. That record was inert: the
//! allocator still resolved her locally. This is the half that makes it
//! load-bearing.
//!
//! ## What it does
//!
//! A decorator over [`PersonaAdapterFactory`]. For each profile it resolves the
//! persona's home, reads her override, and:
//!
//! - override absent, or present and LOCAL → delegate to the inner factory,
//!   byte-for-byte the prior behaviour;
//! - override present and [`PersonaModelOverride::is_remote`] → build an
//!   [`AircRemoteInferenceAdapter`] over [`AircLiveTransport`] pinned to that peer,
//!   so her inference crosses the grid and she never knows.
//!
//! Composing as a decorator rather than editing
//! [`materialize_adapters`](super::supervisor::materialize_adapters) keeps the
//! supervisor unaware of overrides, airc, and peers — it still just calls
//! `build_adapter`.
//!
//! ## An explicit remote assignment FAILS LOUD when it cannot be honoured
//!
//! This is the one place this file deliberately differs from the #2250 overflow
//! effector it descends from. That effector returned `None` (→ local adapter) on
//! any uncertainty, and was right to: overflow placement is OPPORTUNISTIC, so
//! "couldn't route off-box, stayed home" is the correct non-event.
//!
//! An override is not opportunistic. An operator ran `persona/reassign-model
//! --remote-peer` and the durable record says her brain runs on peer X. If airc is
//! not attached, or the peer id will not parse, running her on this host's model
//! instead is precisely the silent downgrade [[no-fallbacks-ever]] forbids — and on
//! the node this exists for, the local model is BELOW THE COGNITION FLOOR (IntelMac,
//! 2026-09-06: 39–42 character bare tool calls with an empty `intent`). A silent
//! local fallback there does not degrade her gracefully; it produces a citizen who
//! cannot hold her own name.
//!
//! So an unhonourable remote assignment returns `Err`, which
//! `materialize_adapters` surfaces as `SupervisorError::AdapterFactory { slot, role,
//! message }` and the persona does not reach hosted state. Loud, per-slot, and it
//! names the peer.
//!
//! ## What is still not proven here
//!
//! That her generation actually lands on the remote node. The unit path can prove
//! WHICH adapter is built and that an unhonourable assignment refuses; it cannot
//! prove the hop. That receipt is the smoke: her generation appearing in the REMOTE
//! node's `delib.generate.cache` rows tagged with her id. The hop itself was
//! measured on IntelMac 2026-09-06 — `ai/generate` addressed to a citizen on the M5
//! returned that node's adapter in 409 ms, with the reply enumerating the REMOTE
//! host's served models — so this builds on a demonstrated round trip, not a hoped-for
//! one.

use std::path::PathBuf;
use std::sync::Arc;

use airc_lib::Airc;

use crate::ai::adapter::AIProviderAdapter;
use crate::inference::airc_remote::adapter::AircRemoteInferenceAdapter;
use crate::inference::airc_remote::transport::AircLiveTransport;
use crate::persona::home::PersonaHome;
use crate::persona::inference_profile::PersonaInferenceProfile;
use crate::persona::supervisor::PersonaAdapterFactory;
use crate::persona::PersonaModelOverride;

/// Wraps a local adapter factory and re-homes any persona whose durable override
/// names a `remote_peer`.
pub struct RemoteLaneAdapterFactory {
    inner: Arc<dyn PersonaAdapterFactory>,
    continuum_root: PathBuf,
    /// The airc handle, read at BUILD time rather than captured at construction.
    ///
    /// Deliberately a `OnceCell` and not an `Option<Arc<Airc>>`: airc attaches
    /// asynchronously and typically AFTER the supervisor is constructed, so an
    /// `Option` snapshotted here would record "not attached" permanently and refuse
    /// every remote persona for the life of the process. Same shape the airc
    /// interceptor and `routing::airc_transport` already use.
    airc: Arc<tokio::sync::OnceCell<Arc<Airc>>>,
}

/// Is this persona's brain assigned to a REMOTE lane? The re-home that sweeps every
/// resident onto a newly served local model must skip her, or the binding this
/// factory built lasts only until the next serving edge (M5 2026-09-06 21:1xZ: Kira
/// and Mathis were bound to the 5090 at 21:06:44 and generated on local Ornith at
/// 21:10 after the lane came up). Reads the durable override; unreadable = not remote,
/// loudly — a re-home never guesses a citizen off-box.
pub(crate) fn is_remote_bound(continuum_root: &std::path::Path, persona_name: &str) -> bool {
    let airc_dir = crate::context::citizen_home_path(
        continuum_root,
        crate::identity::IdentityKind::Persona,
        None,
        persona_name,
    );
    let Some(root) = airc_dir.parent() else {
        return false;
    };
    let home = PersonaHome::from_root(root.to_path_buf());
    match PersonaModelOverride::load(&home) {
        Ok(over) => over.map(|o| o.is_remote()).unwrap_or(false), // unwrap_or: no override = local, by definition
        Err(e) => {
            tracing::warn!(persona = %persona_name, error = %e, "model override unreadable during re-home — treating as local");
            false
        }
    }
}

impl RemoteLaneAdapterFactory {
    pub fn new(
        inner: Arc<dyn PersonaAdapterFactory>,
        continuum_root: PathBuf,
        airc: Arc<tokio::sync::OnceCell<Arc<Airc>>>,
    ) -> Self {
        Self {
            inner,
            continuum_root,
            airc,
        }
    }

    /// Her home root (`…/citizens/personas/<name>/`), where `model_override.json`
    /// lives. Mirrors `commands::persona::reassign_model::resolve_home` — the writer
    /// and the reader must agree on the path or the override is written where nobody
    /// looks.
    fn resolve_home(&self, persona_name: &str) -> Option<PersonaHome> {
        let airc_dir = crate::context::citizen_home_path(
            &self.continuum_root,
            crate::identity::IdentityKind::Persona,
            None,
            persona_name,
        );
        airc_dir
            .parent()
            .map(|root| PersonaHome::from_root(root.to_path_buf()))
    }
}

#[async_trait::async_trait]
impl PersonaAdapterFactory for RemoteLaneAdapterFactory {
    async fn build_adapter(
        &self,
        profile: &PersonaInferenceProfile,
    ) -> Result<Arc<dyn AIProviderAdapter>, String> {
        let Some(home) = self.resolve_home(&profile.persona_name) else {
            // No resolvable home means no override to read — the prior behaviour
            // exactly. This is NOT the remote path failing; it is the remote path
            // never being reached.
            return self.inner.build_adapter(profile).await;
        };

        // A malformed override file is a LOUD failure, not a silent local run: it may
        // be the record that says she belongs on another machine.
        let over = PersonaModelOverride::load(&home).map_err(|e| {
            format!(
                "persona '{}': her model override is unreadable ({e}) — refusing to \
                 guess whether she belongs on this host or a peer",
                profile.persona_name
            )
        })?;

        let Some(over) = over.filter(|o| o.is_remote()) else {
            return self.inner.build_adapter(profile).await;
        };

        // From here she is explicitly assigned to a peer. Every failure below is an
        // Err, never a local adapter.
        let peer_str = over
            .remote_peer
            .as_deref()
            .expect("is_remote() implies remote_peer is Some");
        let peer = uuid::Uuid::parse_str(peer_str).map_err(|_| {
            format!(
                "persona '{}' is assigned model '{}' on peer '{peer_str}', which is not a \
                 peer UUID. Her durable override is unusable — re-run \
                 persona/reassign-model with a valid peer id.",
                profile.persona_name, over.model_id
            )
        })?;

        let airc = self.airc.get().ok_or_else(|| {
            format!(
                "persona '{}' is assigned model '{}' on peer {peer}, but airc is not \
                 attached on this host, so there is no wire to carry her inference. \
                 Refusing to run her on a local model instead — that would silently \
                 substitute a different brain for the one she was assigned.",
                profile.persona_name, over.model_id
            )
        })?;

        let transport = AircLiveTransport::new(Arc::clone(airc), peer);
        // The responder stamps the window its lane serves on every answer; when it
        // differs from what her override recorded, the record follows the wire so
        // her next re-host budgets against the real window (card 1ab60567). The
        // running adapter is not resized mid-flight — the spawn pin owns that.
        let learned_home = home.clone();
        let learned_over = over.clone();
        let learned_name = profile.persona_name.clone();
        let window_sink: Arc<dyn Fn(u32) + Send + Sync> = Arc::new(move |window: u32| {
            if learned_over.remote_context_window == Some(window) {
                return;
            }
            let updated = learned_over.clone().with_context_window(window);
            match updated.write(&learned_home) {
                Ok(()) => crate::probe!(
                    class = "remote_lane.window_learned",
                    persona = %learned_name,
                    window = window,
                    was = ?learned_over.remote_context_window,
                    "the responder serves a different window than her override recorded — record updated; takes effect at her next re-host"
                ),
                Err(e) => crate::probe!(
                    class = "remote_lane.window_learn_failed",
                    persona = %learned_name,
                    window = window,
                    error = %e,
                    "could not record the responder's served window"
                ),
            }
        });
        let adapter = AircRemoteInferenceAdapter::new(transport)
            .with_target_peer(peer.to_string())
            .with_model(over.model_id.clone())
            .with_window_sink(window_sink);

        crate::probe!(
            class = "persona.adapter.remote_lane",
            persona = %profile.persona_name,
            persona_id = %profile.persona_id,
            model = %over.model_id,
            peer = %peer,
            "persona's brain bound to a REMOTE lane — her inference crosses the grid"
        );

        Ok(Arc::new(adapter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Records whether the LOCAL path was taken. The whole point of several of these
    /// tests is that a remote assignment must never reach it.
    struct CountingInner {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl PersonaAdapterFactory for CountingInner {
        async fn build_adapter(
            &self,
            _profile: &PersonaInferenceProfile,
        ) -> Result<Arc<dyn AIProviderAdapter>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            // The value is irrelevant — every assertion here is about WHICH path ran.
            Err("local factory reached".to_string())
        }
    }

    fn profile(name: &str) -> PersonaInferenceProfile {
        use crate::persona::hw_tier_descriptor::HwTierCategory;
        use crate::persona::inference_profile::SamplingProfile;
        PersonaInferenceProfile {
            persona_id: uuid::Uuid::nil(),
            persona_name: name.to_string(),
            model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            gguf_local_path: None,
            tier_category: HwTierCategory::Compat,
            tier_id: "mac_intel_metal_discrete".to_string(),
            context_length: 2048,
            n_ubatch: 512,
            n_batch: 2048,
            n_seq_max: 1,
            n_gpu_layers: 0,
            sampling: SamplingProfile::chat_defaults(),
            chat_template: None,
            stop_sequences: vec![],
        }
    }

    /// Build a factory over a temp root, plus the call-counter for the inner factory.
    fn factory_over(root: &std::path::Path) -> (RemoteLaneAdapterFactory, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(CountingInner {
            calls: Arc::clone(&calls),
        });
        (
            RemoteLaneAdapterFactory::new(
                inner,
                root.to_path_buf(),
                // Empty cell = airc not attached, which is the state this node is in
                // whenever the attach task has not completed.
                Arc::new(tokio::sync::OnceCell::new()),
            ),
            calls,
        )
    }

    fn home_for(root: &std::path::Path, name: &str) -> PersonaHome {
        let airc_dir = crate::context::citizen_home_path(
            root,
            crate::identity::IdentityKind::Persona,
            None,
            name,
        );
        let home = PersonaHome::from_root(airc_dir.parent().unwrap().to_path_buf());
        home.ensure_exists().expect("mkdir home");
        home
    }

    // what this catches: the decorator swallowing the ordinary case. A persona with no
    // override at all must reach the inner factory unchanged — this wrapper sits in the
    // production path for EVERY persona, so a regression here breaks local hosting on
    // every node, not just the ones using remote lanes.
    #[tokio::test]
    async fn a_persona_with_no_override_is_delegated_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let (f, calls) = factory_over(tmp.path());
        home_for(tmp.path(), "Paige");

        let _ = f.build_adapter(&profile("Paige")).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1, "local factory must be used");
    }

    // what this catches: a LOCAL override being mistaken for a remote one. `is_remote()`
    // is the only discriminator, and a persona pinned to a local model must still go
    // down the unchanged path.
    #[tokio::test]
    async fn a_local_override_is_delegated_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let (f, calls) = factory_over(tmp.path());
        let home = home_for(tmp.path(), "Paige");
        PersonaModelOverride::new("some/local-model", None, 1)
            .write(&home)
            .expect("write override");

        let _ = f.build_adapter(&profile("Paige")).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1, "local factory must be used");
    }

    // what this catches — THE no-fallback guarantee, and the reason this file exists:
    // an explicit remote assignment that cannot be honoured must REFUSE, never quietly
    // run her on this host's model. On IntelMac the local model is below the cognition
    // floor (39-42 char bare tool calls, empty intent, bidirectional identity bleed —
    // cards b1846b3c / bb387821), so a silent local fallback does not degrade her, it
    // produces a citizen who cannot hold her own name.
    //
    // Asserts BOTH halves: it errors, AND the inner factory was never reached. Only
    // the second half distinguishes "refused" from "fell back and the fallback failed".
    #[tokio::test]
    async fn an_unhonourable_remote_assignment_refuses_instead_of_running_her_locally() {
        let tmp = tempfile::tempdir().unwrap();
        let (f, calls) = factory_over(tmp.path()); // cell is empty = airc not attached
        let home = home_for(tmp.path(), "Saoirse");
        let peer = "e5f4141d-1d95-4d62-8c19-97b5f8320837";
        PersonaModelOverride::new_remote("ornith-ai/Ornith-1.5-35B-A3B-GGUF", None, 1, peer)
            .write(&home)
            .expect("write override");

        let err = f
            .build_adapter(&profile("Saoirse"))
            .await
            .map(|_| ()) // Arc<dyn AIProviderAdapter> is not Debug; the Ok value is irrelevant here
            .expect_err("no airc means the assignment cannot be honoured");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "the LOCAL factory must never be reached for a remote assignment"
        );
        assert!(err.contains("Saoirse"), "must name the persona: {err}");
        assert!(err.contains(peer), "must name the peer: {err}");
        assert!(
            err.contains("airc is not attached"),
            "must say WHY it could not be honoured: {err}"
        );
    }

    // what this catches: a peer id that survived into the durable record failing at
    // materialisation with no clue where it came from. Refused by name, and again the
    // local factory is never reached.
    #[tokio::test]
    async fn a_malformed_peer_in_the_override_refuses_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let (f, calls) = factory_over(tmp.path());
        let home = home_for(tmp.path(), "Saoirse");
        PersonaModelOverride::new_remote("some/model", None, 1, "not-a-uuid")
            .write(&home)
            .expect("write override");

        let err = f
            .build_adapter(&profile("Saoirse"))
            .await
            .map(|_| ())
            .expect_err("a non-uuid peer cannot be routed to");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no local fallback");
        assert!(err.contains("not-a-uuid"), "must name the bad value: {err}");
    }

    // what this catches (M5 2026-09-06 21:1xZ): the serving-edge re-home sweeping a
    // remote-bound citizen back onto the local model. A persona with a remote override
    // on disk reads remote; one with a local override or none reads local.
    #[test]
    fn a_remote_override_on_disk_is_seen_by_the_rehome() {
        let root = tempfile::tempdir().expect("root");
        let home_root = crate::context::citizen_home_path(
            root.path(),
            crate::identity::IdentityKind::Persona,
            None,
            "Kira",
        );
        let home = PersonaHome::from_root(home_root.parent().expect("parent").to_path_buf());
        std::fs::create_dir_all(home_root.parent().expect("parent")).expect("mkdir");
        assert!(!is_remote_bound(root.path(), "Kira"), "no override = local");
        PersonaModelOverride::new_remote(
            "ggml-org/Qwen3.8-27B-GGUF".to_string(),
            Some("operator".to_string()),
            1,
            "ce8b9074-2fca-4347-a954-a1cf720cee55",
        )
        .write(&home)
        .expect("write override");
        assert!(is_remote_bound(root.path(), "Kira"), "a remote override reads remote");
        PersonaModelOverride::new("local/model".to_string(), Some("operator".to_string()), 2)
            .write(&home)
            .expect("write local");
        assert!(!is_remote_bound(root.path(), "Kira"), "a local override reads local");
    }
}
