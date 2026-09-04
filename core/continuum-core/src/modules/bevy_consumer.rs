//! `BevyConsumer` — the avatar renderer's face to the
//! [`ResourceGovernor`](crate::resources), and the THIRD and final consumer that
//! closes the trait (#56). It is the hybrid of the two that came before:
//!
//! - **Fat and reclaimable, like serving (#79).** A running Bevy renderer holds
//!   ~3GB of wgpu device + ECS world + Metal pipeline residency. When nothing is
//!   on a call, that is pure reclaimable slack: [`shutdown`] it and the next call
//!   transparently restarts it (~5s cold start, hidden behind call setup).
//! - **Load-bearing during a live call, like voice
//!   ([`VoiceConsumer`](super::live_session_consumer)).** The renderer's output
//!   texture **is the video feed into LiveKit** — each rendered frame is read back
//!   and handed to a `NativeVideoSource` track. Tear the renderer down mid-call
//!   and the human's avatar freezes. So under pressure while a call is live this
//!   consumer [`refuses`](ReclaimOutcome::refused), and serving tiers down first.
//!
//! # The two signals that gate the refusal
//!
//! A refusal must fire whenever the renderer is feeding a live view, so it reads
//! two independent truths and refuses if EITHER is live:
//!
//! - [`AudioResourceLifecycle::active_count`] — the authoritative "a human is on a
//!   call" gate (a video call carries an audio session).
//! - the renderer's own `active_slots` — Bevy slots actively producing frames for
//!   a video track right now. Belt to the session-count braces: even if the audio
//!   session count is momentarily zero, a slot still rendering means a track is
//!   still being fed, so we do not pull the rug.
//!
//! # Honest footprint — measured, and honest about what it CANNOT measure
//!
//! The residency reported is the **tracked** Rendering-subsystem VRAM
//! ([`GpuMemoryManager::used_bytes`]`(`[`Rendering`](GpuSubsystem::Rendering)`)`) —
//! the render targets + loaded model bytes the renderer accounts through
//! `allocate()`. That is real and measured, but it **materially undercounts** the
//! true ~3GB: wgpu device/queue overhead, decoded GPU meshes/textures, the ECS
//! world and Metal pipeline caches are not centrally accounted. Per
//! [`fallbacks-are-illegal`] we report the measured number and NAME the gap in the
//! detail string rather than fabricate a 3GB constant. (Closing that accounting
//! gap — reporting true rendering VRAM — is its own follow-on.)
//!
//! # No new task, no global coupling in tests
//!
//! All renderer reads and the teardown lever are process-global free functions
//! ([`bevy_renderer`](crate::live::video::bevy_renderer)); they are abstracted
//! behind the injectable [`RenderSurface`] so a unit test can prove the
//! refuse/shed disposition without spinning up a real Bevy OS thread. Like the
//! other two consumers this owns no tick, no thread, no lock across an await — the
//! governor's daemon drives it.
//!
//! [`shutdown`]: crate::live::video::bevy_renderer::shutdown
//! [`AudioResourceLifecycle::active_count`]: crate::live::audio::resource_lifecycle::AudioResourceLifecycle::active_count
//! [`fallbacks-are-illegal`]: crate::resources

use std::sync::Arc;

use async_trait::async_trait;

use crate::gpu::{GpuMemoryManager, GpuSubsystem};
use crate::live::audio::resource_lifecycle::AudioResourceLifecycle;
use crate::resources::{
    ConsumerFootprint, ReclaimOutcome, ReclaimReason, ReclaimRequest, ResourceConsumer,
    ResourceKind,
};

/// The renderer surface the consumer observes and drives — every read and the
/// teardown lever, abstracted so tests don't depend on the process-global Bevy
/// singleton.
#[async_trait]
pub trait RenderSurface: Send + Sync {
    /// Is the Bevy renderer currently up (holding its ECS/wgpu residency)?
    fn is_running(&self) -> bool;
    /// Bevy slots actively producing frames for a live video track right now.
    fn rendering_slots(&self) -> u32;
    /// Tracked Rendering-subsystem VRAM residency (partial — device/ECS overhead
    /// is not centrally counted).
    fn tracked_vram_bytes(&self) -> u64;
    /// Tear the renderer down — the caller guarantees no call is live — and return
    /// the tracked residency released. Physical device/ECS VRAM frees as the Bevy
    /// thread exits; the number returned is the tracked claim this consumer gives
    /// back.
    async fn shed(&self) -> u64;
}

/// The production surface: reads the process-global Bevy renderer and the shared
/// GPU manager; sheds with the exact `shutdown() + reset_slot_pool()` pair the
/// `voice/resource-unload` command uses — only ever called here when no call is
/// live.
pub struct GlobalRenderSurface {
    gpu: Arc<GpuMemoryManager>,
}

impl GlobalRenderSurface {
    pub fn new(gpu: Arc<GpuMemoryManager>) -> Self {
        Self { gpu }
    }
}

#[async_trait]
impl RenderSurface for GlobalRenderSurface {
    fn is_running(&self) -> bool {
        crate::live::video::bevy_renderer::is_running()
    }

    fn rendering_slots(&self) -> u32 {
        crate::live::video::bevy_renderer::try_get()
            .map(|sys| {
                sys.memory_stats
                    .active_slots
                    .load(std::sync::atomic::Ordering::Relaxed) as u32
            })
            .unwrap_or(0)
    }

    fn tracked_vram_bytes(&self) -> u64 {
        self.gpu.used_bytes(GpuSubsystem::Rendering)
    }

    async fn shed(&self) -> u64 {
        let before = self.gpu.used_bytes(GpuSubsystem::Rendering);
        if crate::live::video::bevy_renderer::is_running() {
            crate::live::video::bevy_renderer::shutdown();
            crate::live::avatar::reset_slot_pool();
        }
        before
    }
}

/// The avatar renderer as a peer [`ResourceConsumer`] — reclaimable when idle,
/// protected while its texture is feeding a live LiveKit video track.
pub struct BevyConsumer {
    lifecycle: Arc<AudioResourceLifecycle>,
    surface: Arc<dyn RenderSurface>,
}

impl BevyConsumer {
    /// Wire the production consumer over the process-global renderer.
    pub fn new(lifecycle: Arc<AudioResourceLifecycle>, gpu: Arc<GpuMemoryManager>) -> Self {
        Self {
            lifecycle,
            surface: Arc::new(GlobalRenderSurface::new(gpu)),
        }
    }

    /// Inject a custom surface — tests drive the refuse/shed disposition (and
    /// assert the renderer is NEVER shed while a call is live) without a real Bevy
    /// thread.
    pub fn with_surface(
        lifecycle: Arc<AudioResourceLifecycle>,
        surface: Arc<dyn RenderSurface>,
    ) -> Self {
        Self { lifecycle, surface }
    }
}

#[async_trait]
impl ResourceConsumer for BevyConsumer {
    fn consumer_id(&self) -> &str {
        "render"
    }

    fn footprint(&self) -> Vec<ConsumerFootprint> {
        if !self.surface.is_running() {
            // Honest: renderer down → no residency to report.
            return Vec::new();
        }
        let bytes = self.surface.tracked_vram_bytes();
        let slots = self.surface.rendering_slots();
        vec![ConsumerFootprint {
            kind: ResourceKind::Vram,
            bytes,
            detail: format!(
                "renderer up, {slots} slot(s) rendering; tracked render VRAM (device/ECS overhead not centrally counted)"
            ),
        }]
    }

    async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome {
        // Shutdown: the whole box is going down — tear the renderer down and report
        // the tracked residency released. No call left to protect.
        if request.reason == ReclaimReason::Shutdown {
            let freed = self.surface.shed().await;
            return ReclaimOutcome::released(freed);
        }

        // Pressure / Rebalance with a live view: REFUSE. The renderer's texture is
        // the video feed into LiveKit; tearing it down freezes the avatar
        // mid-call. Refuse if a call is live OR a slot is actively rendering —
        // named refusal, not a silent zero. Serving tiers down first; a live
        // call's Pinned lease is never even selected by the ledger.
        let sessions = self.lifecycle.active_count();
        let slots = self.surface.rendering_slots();
        if sessions > 0 || slots > 0 {
            return ReclaimOutcome::refused(format!(
                "{sessions} live call(s), {slots} slot(s) rendering — the renderer's texture is the live video feed; tearing it down would freeze the avatar mid-call"
            ));
        }

        // Idle: safe to shed. The renderer restarts transparently (~5s) on the next
        // call, hidden behind call setup.
        let freed = self.surface.shed().await;
        ReclaimOutcome::released(freed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::live_session_consumer::{VoiceConsumer, VoiceReclaimLever};
    use crate::resources::{
        DaemonConfig, GovernorConfig, LeaseRequest, MockCapacitySource, ReclaimPolicy,
        ReclaimStatus, ResourceDaemon,
    };
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::time::Duration;

    /// A fake renderer surface with controllable running/slots/vram and a shed
    /// counter — lets a test drive the reclaim path AND assert the renderer is
    /// never shed while a call is live (the anti-kick proof) without a Bevy thread.
    struct FakeSurface {
        running: AtomicBool,
        slots: AtomicU32,
        vram: AtomicU64,
        sheds: AtomicU32,
    }
    impl FakeSurface {
        fn new(running: bool, slots: u32, vram: u64) -> Arc<Self> {
            Arc::new(Self {
                running: AtomicBool::new(running),
                slots: AtomicU32::new(slots),
                vram: AtomicU64::new(vram),
                sheds: AtomicU32::new(0),
            })
        }
        fn sheds(&self) -> u32 {
            self.sheds.load(Ordering::SeqCst)
        }
    }
    #[async_trait]
    impl RenderSurface for FakeSurface {
        fn is_running(&self) -> bool {
            self.running.load(Ordering::SeqCst)
        }
        fn rendering_slots(&self) -> u32 {
            self.slots.load(Ordering::SeqCst)
        }
        fn tracked_vram_bytes(&self) -> u64 {
            self.vram.load(Ordering::SeqCst)
        }
        async fn shed(&self) -> u64 {
            self.sheds.fetch_add(1, Ordering::SeqCst);
            self.running.store(false, Ordering::SeqCst);
            self.slots.store(0, Ordering::SeqCst);
            self.vram.swap(0, Ordering::SeqCst)
        }
    }

    fn pressure(bytes: u64) -> ReclaimRequest {
        ReclaimRequest {
            kind: ResourceKind::Vram,
            target_bytes: bytes,
            deadline_ms: 0,
            reason: ReclaimReason::Pressure,
        }
    }

    // what this catches: THE anti-kick core — while a call is live, a pressure
    // reclaim is refused with a named reason and frees nothing, and the renderer is
    // never shed (the avatar keeps rendering, the video feed survives).
    #[tokio::test]
    async fn active_call_refuses_render_reclaim() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new());
        lifecycle.on_session_start();
        let surface = FakeSurface::new(true, 1, 3_000_000);
        let bevy = BevyConsumer::with_surface(lifecycle.clone(), surface.clone());

        let out = bevy.reclaim(pressure(3_000_000)).await;

        assert_eq!(out.status, ReclaimStatus::Refused);
        assert_eq!(out.freed_bytes, 0);
        assert!(
            out.detail.unwrap().contains("freeze the avatar"),
            "named refusal"
        );
        assert_eq!(
            surface.sheds(),
            0,
            "the renderer is never shed while a call is live"
        );
        assert!(surface.is_running(), "renderer still up");
    }

    // what this catches: the belt-to-the-braces gate — even with ZERO audio
    // sessions, a slot still actively rendering means a video track is still being
    // fed, so the reclaim is still refused. Session-count alone is not the only
    // liveness signal.
    #[tokio::test]
    async fn rendering_slot_alone_refuses_reclaim() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new()); // no sessions
        let surface = FakeSurface::new(true, 1, 3_000_000); // but a slot is rendering
        let bevy = BevyConsumer::with_surface(lifecycle.clone(), surface.clone());

        let out = bevy.reclaim(pressure(3_000_000)).await;

        assert_eq!(out.status, ReclaimStatus::Refused);
        assert_eq!(
            surface.sheds(),
            0,
            "a rendering slot alone protects the renderer"
        );
    }

    // what this catches: the "constantly kicking" scenario — under SUSTAINED
    // pressure (10 back-to-back asks) the renderer is refused every time and never
    // torn down mid-call.
    #[tokio::test]
    async fn repeated_pressure_never_kills_the_renderer_during_a_call() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new());
        lifecycle.on_session_start();
        let surface = FakeSurface::new(true, 1, 3_000_000);
        let bevy = BevyConsumer::with_surface(lifecycle.clone(), surface.clone());

        for _ in 0..10 {
            let out = bevy.reclaim(pressure(3_000_000)).await;
            assert_eq!(out.status, ReclaimStatus::Refused);
            assert_eq!(out.freed_bytes, 0);
        }
        assert_eq!(surface.sheds(), 0, "never shed across 10 asks");
        assert!(surface.is_running(), "renderer survived all of it");
    }

    // what this catches: the SMART other half — when NOTHING is on a call and no
    // slot is rendering, the consumer cooperates: sheds the ~3GB renderer and
    // reports the freed residency as Released. Not a stubborn always-refuse.
    #[tokio::test]
    async fn idle_pressure_sheds_the_renderer() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new()); // no sessions
        let surface = FakeSurface::new(true, 0, 3_000_000_000); // running but idle
        let bevy = BevyConsumer::with_surface(lifecycle.clone(), surface.clone());

        let out = bevy.reclaim(pressure(2_000_000_000)).await;

        assert_eq!(out.status, ReclaimStatus::Released);
        assert_eq!(
            out.freed_bytes, 3_000_000_000,
            "reports the tracked residency released"
        );
        assert_eq!(surface.sheds(), 1, "shed exactly once when idle");
        assert!(!surface.is_running(), "renderer torn down");
    }

    // what this catches: Shutdown tears the renderer down even with a live call —
    // the box is going down, nothing left to protect.
    #[tokio::test]
    async fn shutdown_tears_down_even_during_a_call() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new());
        lifecycle.on_session_start();
        let surface = FakeSurface::new(true, 1, 3_000_000_000);
        let bevy = BevyConsumer::with_surface(lifecycle.clone(), surface.clone());

        let req = ReclaimRequest {
            kind: ResourceKind::Vram,
            target_bytes: 3_000_000_000,
            deadline_ms: 0,
            reason: ReclaimReason::Shutdown,
        };
        let out = bevy.reclaim(req).await;

        assert_eq!(out.status, ReclaimStatus::Released);
        assert_eq!(out.freed_bytes, 3_000_000_000);
        assert_eq!(surface.sheds(), 1);
    }

    // what this catches: footprint reports the tracked Rendering VRAM ONLY while
    // the renderer is up, and reports nothing when it is down — the honest
    // "renderer torn down → zero resident" state.
    #[tokio::test]
    async fn footprint_reports_tracked_vram_only_when_running() {
        let lifecycle = Arc::new(AudioResourceLifecycle::new());
        let up =
            BevyConsumer::with_surface(lifecycle.clone(), FakeSurface::new(true, 2, 53_000_000));
        let fp = up.footprint();
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].kind, ResourceKind::Vram);
        assert_eq!(fp[0].bytes, 53_000_000);
        assert!(fp[0].detail.contains("2 slot(s) rendering"));

        let down = BevyConsumer::with_surface(lifecycle.clone(), FakeSurface::new(false, 0, 0));
        assert!(
            down.footprint().is_empty(),
            "renderer down → report nothing"
        );
    }

    // ---- the crown jewel: all THREE consumers, one live call ------------------

    /// A serving stand-in: Graceful, fully reclaimable. (daemon.rs's
    /// `ScriptedConsumer` is `#[cfg(test)]`-private to its own module.)
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

    /// A no-op voice shed lever (voice refuses while the call is live, so it is
    /// never actually pulled — this just satisfies the constructor).
    struct NoopVoiceLever;
    #[async_trait]
    impl VoiceReclaimLever for NoopVoiceLever {
        async fn shed_idle_models(&self) -> u64 {
            0
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

    fn board_total(board: &crate::resources::LeaseBoard) -> u64 {
        board.leases.iter().map(|l| l.bytes).sum()
    }

    // what this catches: the full "smart system" with ALL THREE consumers and one
    // live audio+video call. Serving (Graceful), the voice pipeline (Pinned), and
    // the Bevy renderer (Pinned) all hold VRAM. A squeeze puts the box over budget.
    // The daemon must reclaim SERVING and leave BOTH sides of the live call
    // untouched: neither Pinned lease shrinks, the voice models are not shed, and
    // the renderer is not torn down (its video feed into LiveKit survives). If
    // arbitration ever picked either live lease, this fails — the call would have
    // been kicked.
    #[tokio::test]
    async fn under_pressure_serving_tiers_down_and_neither_call_nor_render_is_kicked() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 24_000));

        // One live audio+video call — shared lifecycle, 1 session.
        let lifecycle = Arc::new(AudioResourceLifecycle::new());
        lifecycle.on_session_start();

        // Voice: TTS resident, a Pinned lease.
        let gpu = {
            let mgr = GpuMemoryManager::simulated("test-gpu", 24_000);
            mgr.account_external(GpuSubsystem::Tts, 3_000);
            Arc::new(mgr)
        };
        let voice = Arc::new(VoiceConsumer::with_lever(
            lifecycle.clone(),
            gpu.clone(),
            Arc::new(NoopVoiceLever),
        ));

        // Renderer: up, a slot rendering, a Pinned lease.
        let surface = FakeSurface::new(true, 1, 3_000);
        let bevy = Arc::new(BevyConsumer::with_surface(
            lifecycle.clone(),
            surface.clone(),
        ));

        // Serving: fully reclaimable, Graceful.
        let serving = ReleasablePeer::new("serving", 8_000);

        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![voice.clone(), bevy.clone(), serving.clone()],
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
        let render = daemon
            .acquire(&lease("render", 3_000, ReclaimPolicy::Pinned))
            .unwrap();
        assert_eq!(daemon.board().leases.len(), 3);

        // Squeeze VRAM to 7GB — granted (14GB) is 7GB over. Reclaiming serving fully
        // (8GB) leaves the two 3GB live leases = 6GB, within the 7GB ceiling.
        src.set_ceiling(7_000);

        let settled = settle(&daemon, |b| board_total(b) <= 7_000).await;
        assert!(
            settled,
            "daemon should reclaim serving to get within budget"
        );

        // Neither side of the live call was touched — the whole point.
        let board = daemon.board();
        assert_eq!(
            board
                .leases
                .iter()
                .find(|l| l.lease_id == call.lease_id)
                .map(|l| l.bytes),
            Some(3_000),
            "the live call's voice Pinned lease is never shrunk"
        );
        assert_eq!(
            board
                .leases
                .iter()
                .find(|l| l.lease_id == render.lease_id)
                .map(|l| l.bytes),
            Some(3_000),
            "the live call's render Pinned lease is never shrunk"
        );
        assert_eq!(surface.sheds(), 0, "the renderer was never torn down");
        assert!(surface.is_running(), "the video feed into LiveKit survives");
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
}
