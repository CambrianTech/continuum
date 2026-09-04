//! `VoiceConsumer` — the live-voice pipeline's face to the
//! [`ResourceGovernor`](crate::resources), and the outlier-B that validates the
//! [`ResourceConsumer`] trait against a consumer whose reclaim disposition is the
//! *opposite* of serving's (#56).
//!
//! Serving is eminently reclaimable: its whole reason to hold VRAM is fungible
//! inference it can tier-down or unload on demand (#79). A **live human call** is
//! the opposite — the STT/TTS models resident for the length of a call are load-
//! bearing for a person mid-conversation, and reclaiming them mid-call kills
//! Whisper under them (the exact hazard the audio idle-watcher's own comments
//! warn about: *"60s was killing Whisper mid-conversation"*). So this consumer's
//! answer to pressure is not "shrink" — it is:
//!
//! - **A live call is active → [`refuse`](ReclaimOutcome::refused).** A named
//!   refusal, never a silent zero. The authority keeps re-asking (patient
//!   backpressure, [`Refused`] folds to zero freed) and reclaims serving's
//!   tier-down-able lease first. And structurally: a live call holds a **`Pinned`**
//!   lease, which the ledger never even *selects* as a victim — so under normal
//!   arbitration this consumer's `reclaim` is never called for a live call at all.
//!   The refusal is the belt to the Pinned-lease braces: the guarantee holds even
//!   if a call somehow leased reclaimable bytes.
//! - **Idle (no call) → shed and [`release`](ReclaimOutcome::released).** With no
//!   one to disturb, the consumer pulls the same teardown the 5s idle-watcher runs
//!   ([`AudioResourceLifecycle::shutdown_all_adapters`]), just *now* because the
//!   authority needs the bytes. Models reload transparently on the next call.
//! - **[`Shutdown`] → end the pipeline and release.** The box is going down; there
//!   is no call left to protect.
//!
//! This is why *"it's not gonna wreck the experience by constantly kicking"* is a
//! property of the substrate, not a hope: no matter how many times the authority
//! asks under sustained pressure, a live call is refused every time and serving is
//! what tiers down instead.
//!
//! # Honest footprint, one measured source
//!
//! The resident cost this consumer reports is its **TTS weights in VRAM**, read
//! straight from the one VRAM accountant
//! ([`GpuMemoryManager::used_bytes`]`(`[`Tts`](GpuSubsystem::Tts)`)`) — the same
//! atomic `gpu/stats` reports, never a hardcoded model size. When no call is live
//! and the models are unloaded, that reads zero and the consumer reports nothing.
//! (STT/audio-buffer host-RAM residency is not yet centrally accounted; this
//! reports what is measured and no more — [`fallbacks-are-illegal`] means we do
//! not fabricate the rest.)
//!
//! # No new task, no parallel allocator
//!
//! Like [`ServingConsumer`](super::serving_consumer::ServingConsumer), this is a
//! thin adapter over handles the runtime already owns — the audio lifecycle's
//! session counter and the GPU manager — plus an injected shed lever. It owns no
//! tick, no thread, no lock across an await; the governor's daemon drives it.
//!
//! [`Refused`]: crate::resources::ReclaimStatus::Refused
//! [`Shutdown`]: crate::resources::ReclaimReason::Shutdown
//! [`fallbacks-are-illegal`]: crate::resources

use std::sync::Arc;

use async_trait::async_trait;

use crate::gpu::{GpuMemoryManager, GpuSubsystem};
use crate::live::audio::resource_lifecycle::AudioResourceLifecycle;
use crate::resources::{
    ConsumerFootprint, ReclaimOutcome, ReclaimReason, ReclaimRequest, ResourceConsumer,
    ResourceKind,
};

/// The lever the consumer pulls to actually free voice residency: shut the idle
/// STT/TTS adapters down and report the VRAM the GPU released. Injected as a trait
/// so the consumer stays pure and a test can drive the reclaim path without real
/// audio models loaded.
#[async_trait]
pub trait VoiceReclaimLever: Send + Sync {
    /// Free the voice pipeline's resident models — only ever called while **no
    /// call is live** — and return the bytes actually reclaimed.
    async fn shed_idle_models(&self) -> u64;
}

/// The production lever: the exact teardown the idle-watcher runs
/// ([`AudioResourceLifecycle::shutdown_all_adapters`]), driven by pressure instead
/// of the idle timer. Bytes freed = measured TTS residency before − after, read
/// from the GPU manager — the honest scan-confirmed delta, never a model constant.
pub struct AdapterShutdownLever {
    gpu: Arc<GpuMemoryManager>,
}

impl AdapterShutdownLever {
    pub fn new(gpu: Arc<GpuMemoryManager>) -> Self {
        Self { gpu }
    }
}

#[async_trait]
impl VoiceReclaimLever for AdapterShutdownLever {
    async fn shed_idle_models(&self) -> u64 {
        let before = self.gpu.used_bytes(GpuSubsystem::Tts);
        AudioResourceLifecycle::shutdown_all_adapters().await;
        let after = self.gpu.used_bytes(GpuSubsystem::Tts);
        before.saturating_sub(after)
    }
}

/// Voice as a peer [`ResourceConsumer`]. Reads live-session count from the audio
/// lifecycle (the refuse-or-shed decision), footprint from the GPU manager's TTS
/// subsystem, and frees via an injected [`VoiceReclaimLever`].
pub struct VoiceConsumer {
    lifecycle: Arc<AudioResourceLifecycle>,
    gpu: Arc<GpuMemoryManager>,
    shed: Arc<dyn VoiceReclaimLever>,
}

impl VoiceConsumer {
    /// Wire the production consumer: shed via [`AdapterShutdownLever`] over the
    /// shared GPU manager.
    pub fn new(lifecycle: Arc<AudioResourceLifecycle>, gpu: Arc<GpuMemoryManager>) -> Self {
        let shed = Arc::new(AdapterShutdownLever::new(gpu.clone()));
        Self {
            lifecycle,
            gpu,
            shed,
        }
    }

    /// Inject a custom shed lever — tests drive the reclaim path (and assert it is
    /// NEVER pulled while a call is live) without loading real audio models.
    pub fn with_lever(
        lifecycle: Arc<AudioResourceLifecycle>,
        gpu: Arc<GpuMemoryManager>,
        shed: Arc<dyn VoiceReclaimLever>,
    ) -> Self {
        Self {
            lifecycle,
            gpu,
            shed,
        }
    }
}

#[async_trait]
impl ResourceConsumer for VoiceConsumer {
    fn consumer_id(&self) -> &str {
        "voice"
    }

    fn footprint(&self) -> Vec<ConsumerFootprint> {
        let bytes = self.gpu.used_bytes(GpuSubsystem::Tts);
        if bytes == 0 {
            // Honest: no models resident (no call, or idle-swept) → report nothing.
            return Vec::new();
        }
        let sessions = self.lifecycle.active_count();
        vec![ConsumerFootprint {
            kind: ResourceKind::Vram,
            bytes,
            detail: format!("{sessions} live voice session(s); TTS models resident"),
        }]
    }

    async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome {
        // Shutdown: the whole box is going down — end the pipeline and report what
        // was freed. There is no call left to protect.
        if request.reason == ReclaimReason::Shutdown {
            self.lifecycle.reset_sessions();
            let freed = self.shed.shed_idle_models().await;
            return ReclaimOutcome::released(freed);
        }

        // Pressure / Rebalance with a live call: REFUSE. Reclaiming STT/TTS while a
        // human is mid-conversation drops the call — the "don't wreck the
        // experience by constantly kicking" failure. Named refusal, not a silent
        // zero; the governor re-asks patiently and tiers serving down first.
        let sessions = self.lifecycle.active_count();
        if sessions > 0 {
            return ReclaimOutcome::refused(format!(
                "{sessions} live voice session(s) active — reclaiming STT/TTS would drop a call mid-conversation"
            ));
        }

        // Idle: safe to shed now, ahead of the 5s idle timer, because the authority
        // needs the bytes. Models reload transparently on the next call.
        let freed = self.shed.shed_idle_models().await;
        ReclaimOutcome::released(freed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{
        DaemonConfig, GovernorConfig, LeaseRequest, MockCapacitySource, ReclaimPolicy,
        ReclaimStatus, ResourceDaemon,
    };
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
    use std::time::Duration;

    /// A shed lever that counts pulls and frees a fixed amount — lets a test both
    /// drive the idle-reclaim path AND assert the lever is never pulled while a
    /// call is live (the anti-kick proof).
    struct CountingLever {
        freed: u64,
        pulls: AtomicU32,
    }
    impl CountingLever {
        fn new(freed: u64) -> Arc<Self> {
            Arc::new(Self {
                freed,
                pulls: AtomicU32::new(0),
            })
        }
        fn pulls(&self) -> u32 {
            self.pulls.load(Ordering::SeqCst)
        }
    }
    #[async_trait]
    impl VoiceReclaimLever for CountingLever {
        async fn shed_idle_models(&self) -> u64 {
            self.pulls.fetch_add(1, Ordering::SeqCst);
            self.freed
        }
    }

    fn gpu(tts_resident: u64) -> Arc<GpuMemoryManager> {
        let mgr = GpuMemoryManager::simulated("test-gpu", 24_000);
        if tts_resident > 0 {
            mgr.account_external(GpuSubsystem::Tts, tts_resident);
        }
        Arc::new(mgr)
    }

    fn pressure(kind: ResourceKind, bytes: u64) -> ReclaimRequest {
        ReclaimRequest {
            kind,
            target_bytes: bytes,
            deadline_ms: 0,
            reason: ReclaimReason::Pressure,
        }
    }

    // what this catches: THE anti-kick core — a live call refuses a pressure
    // reclaim with a named reason and frees nothing, and the shed lever is never
    // pulled (STT/TTS stay up, the call survives).
    #[tokio::test]
    async fn active_call_refuses_pressure_reclaim() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new());
        lifecycle.on_session_start(); // a human is on a call
        let lever = CountingLever::new(9_999);
        let voice = VoiceConsumer::with_lever(lifecycle.clone(), gpu(0), lever.clone());

        let out = voice.reclaim(pressure(ResourceKind::Vram, 4_000)).await;

        assert_eq!(out.status, ReclaimStatus::Refused);
        assert_eq!(out.freed_bytes, 0, "a live call frees nothing");
        assert!(
            out.detail.unwrap().contains("would drop a call"),
            "named refusal"
        );
        assert_eq!(
            lever.pulls(),
            0,
            "the shed lever is never pulled while a call is live"
        );
        assert_eq!(lifecycle.active_count(), 1, "the call is untouched");
    }

    // what this catches: the "constantly kicking" scenario the user named — under
    // SUSTAINED pressure (10 back-to-back asks) a live call is refused every single
    // time and never dropped. If the refuse were ever a shed, the call dies.
    #[tokio::test]
    async fn repeated_pressure_never_kicks_a_live_call() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new());
        lifecycle.on_session_start();
        let lever = CountingLever::new(9_999);
        let voice = VoiceConsumer::with_lever(lifecycle.clone(), gpu(0), lever.clone());

        for _ in 0..10 {
            let out = voice.reclaim(pressure(ResourceKind::Vram, 8_000)).await;
            assert_eq!(out.status, ReclaimStatus::Refused);
            assert_eq!(out.freed_bytes, 0);
        }
        assert_eq!(lever.pulls(), 0, "never shed across 10 asks");
        assert_eq!(lifecycle.active_count(), 1, "the call survived all of it");
    }

    // what this catches: the SMART other half — when NO call is live the consumer
    // cooperates: it sheds ahead of the idle timer and reports the freed delta as
    // Released. It is not a stubborn always-refuse; it yields whenever it is safe.
    #[tokio::test]
    async fn idle_pressure_sheds_and_releases() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new()); // no sessions
        let lever = CountingLever::new(42_000);
        let voice = VoiceConsumer::with_lever(lifecycle.clone(), gpu(0), lever.clone());

        let out = voice.reclaim(pressure(ResourceKind::Vram, 40_000)).await;

        assert_eq!(out.status, ReclaimStatus::Released);
        assert_eq!(out.freed_bytes, 42_000, "reports what the lever freed");
        assert_eq!(lever.pulls(), 1, "shed exactly once when idle");
    }

    // what this catches: Shutdown ends the pipeline and releases even with a live
    // call — the box is going down, there is nothing left to protect, so the
    // session count is reset and the models are shed.
    #[tokio::test]
    async fn shutdown_ends_pipeline_even_with_a_live_call() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new());
        lifecycle.on_session_start();
        lifecycle.on_session_start();
        let lever = CountingLever::new(3_000);
        let voice = VoiceConsumer::with_lever(lifecycle.clone(), gpu(0), lever.clone());

        let req = ReclaimRequest {
            kind: ResourceKind::Vram,
            target_bytes: 3_000,
            deadline_ms: 0,
            reason: ReclaimReason::Shutdown,
        };
        let out = voice.reclaim(req).await;

        assert_eq!(out.status, ReclaimStatus::Released);
        assert_eq!(out.freed_bytes, 3_000);
        assert_eq!(lever.pulls(), 1);
        assert_eq!(lifecycle.active_count(), 0, "sessions ended on shutdown");
    }

    // what this catches: footprint reports the MEASURED TTS residency from the GPU
    // manager (not a model constant), and reports NOTHING when models are unloaded
    // — the honest "zero resident" state, not a fabricated floor.
    #[tokio::test]
    async fn footprint_reports_measured_tts_residency() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new());
        lifecycle.on_session_start();

        let loaded = VoiceConsumer::new(lifecycle.clone(), gpu(60_000));
        let fp = loaded.footprint();
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].kind, ResourceKind::Vram);
        assert_eq!(
            fp[0].bytes, 60_000,
            "measured from the GPU manager's TTS subsystem"
        );
        assert!(fp[0].detail.contains("1 live voice session"));

        let unloaded = VoiceConsumer::new(lifecycle.clone(), gpu(0));
        assert!(
            unloaded.footprint().is_empty(),
            "nothing resident → report nothing"
        );
    }

    // ---- the crown jewel: end-to-end through the real ResourceDaemon -----------

    /// A serving stand-in: a Graceful, fully-reclaimable peer. Distinct from
    /// daemon.rs's `ScriptedConsumer` (that fixture is `#[cfg(test)]`-private to
    /// its own module); this is the minimal releasable peer this scenario needs.
    struct ReleasablePeer {
        id: String,
        held: AtomicU64,
        reclaims: AtomicU32,
    }
    impl ReleasablePeer {
        fn new(id: &str, held: u64) -> Arc<Self> {
            Arc::new(Self {
                id: id.into(),
                held: AtomicU64::new(held),
                reclaims: AtomicU32::new(0),
            })
        }
    }
    #[async_trait]
    impl ResourceConsumer for ReleasablePeer {
        fn consumer_id(&self) -> &str {
            &self.id
        }
        fn footprint(&self) -> Vec<ConsumerFootprint> {
            vec![ConsumerFootprint {
                kind: ResourceKind::Vram,
                bytes: self.held.load(Ordering::SeqCst),
                detail: "serving stand-in".into(),
            }]
        }
        async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome {
            self.reclaims.fetch_add(1, Ordering::SeqCst);
            let before = self.held.load(Ordering::SeqCst);
            let freed = before.min(request.target_bytes);
            self.held.store(before - freed, Ordering::SeqCst);
            ReclaimOutcome::released(freed)
        }
    }

    fn lease(consumer: &str, bytes: u64, policy: ReclaimPolicy) -> LeaseRequest {
        LeaseRequest {
            consumer_id: consumer.into(),
            kind: ResourceKind::Vram,
            bytes,
            ttl_ms: 60_000,
            reclaim_policy: policy,
        }
    }

    async fn settle(
        daemon: &ResourceDaemon,
        mut pred: impl FnMut(&crate::resources::LeaseBoard) -> bool,
    ) -> bool {
        for _ in 0..200 {
            if pred(&daemon.board()) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    // what this catches: the whole "smart system" claim, end-to-end through the
    // real ResourceDaemon. Serving (Graceful) and a LIVE voice call (Pinned) both
    // hold VRAM. A squeeze puts the box over budget. The daemon must reclaim
    // SERVING and leave the live call completely untouched: voice's Pinned lease is
    // never even selected (the ledger filters active Pinned), so VoiceConsumer's
    // shed lever is never pulled and its lease bytes never shrink. If the
    // arbitration ever picked the call, this fails — the call would have been
    // "kicked."
    #[tokio::test]
    async fn under_pressure_serving_tiers_down_and_the_live_call_is_never_kicked() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 24_000));

        // A live human call: 1 active session, TTS resident, a Pinned lease.
        let lifecycle = Arc::new(AudioResourceLifecycle::new());
        lifecycle.on_session_start();
        let lever = CountingLever::new(3_000);
        let voice = Arc::new(VoiceConsumer::with_lever(
            lifecycle.clone(),
            gpu(3_000),
            lever.clone(),
        ));

        // Serving: fully reclaimable, Graceful.
        let serving = ReleasablePeer::new("serving", 8_000);

        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![voice.clone(), serving.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 50,
                },
            },
        );

        daemon
            .acquire(&lease("serving", 8_000, ReclaimPolicy::Graceful))
            .unwrap();
        let call = daemon
            .acquire(&lease("voice", 3_000, ReclaimPolicy::Pinned))
            .unwrap();
        assert_eq!(daemon.board().leases.len(), 2);

        // Squeeze VRAM to 5GB — granted (11GB) is now 6GB over the ceiling.
        src.set_ceiling(5_000);

        // The daemon settles by reclaiming serving down to within the ceiling.
        let settled = settle(&daemon, |b| !board_over(b, 5_000)).await;
        assert!(
            settled,
            "daemon should reclaim serving to get back within budget"
        );

        // The live call was never touched — the whole point.
        let board = daemon.board();
        let call_lease = board.leases.iter().find(|l| l.lease_id == call.lease_id);
        assert_eq!(
            call_lease.map(|l| l.bytes),
            Some(3_000),
            "the live call's Pinned lease is never shrunk"
        );
        assert_eq!(
            lever.pulls(),
            0,
            "VoiceConsumer's shed lever was never pulled"
        );
        assert_eq!(
            lifecycle.active_count(),
            1,
            "the human is still on the call"
        );
        assert!(
            serving.reclaims.load(Ordering::SeqCst) >= 1,
            "serving is what got reclaimed"
        );
        assert!(
            serving.held.load(Ordering::SeqCst) < 8_000,
            "serving gave up VRAM (tiered down) — it is the reclaimable one, not the call"
        );
    }

    fn board_over(board: &crate::resources::LeaseBoard, ceiling: u64) -> bool {
        board.leases.iter().map(|l| l.bytes).sum::<u64>() > ceiling
    }
}
