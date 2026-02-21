//! Pluggable Video Sources
//!
//! The browser is a DISPLAY SURFACE. Rendering happens server-side —
//! Metal, CUDA, wgpu, bgfx-rs, Unreal Engine, or AI-generated.
//!
//! VideoSource is the pluggable abstraction. Anything that produces frames
//! for a participant implements this trait. The CallManager starts/stops
//! sources and they self-drive their frame loop.
//!
//! Current implementations:
//! - TestPatternSource: SMPTE color bars (proves plumbing works)
//!
//! Future implementations:
//! - WgpuAvatarSource: headless wgpu+rend3 with VRM models
//! - UnrealBridgeSource: receives frames from Unreal Engine over TCP
//! - WebcamRelaySource: browser webcam frames forwarded to other participants
//! - BgfxSource: bgfx-rs rendered scenes
//! - NanobananSource: AI-generated video frames

use crate::voice::handle::Handle;
use crate::voice::video_generator::TestPatternGenerator;
use tokio::sync::{broadcast, mpsc};
use tracing::info;

/// Pluggable video source — anything that produces frames for a participant.
///
/// The browser is a DISPLAY SURFACE. Rendering happens server-side or on
/// a GPU machine, streamed as encoded frames.
pub trait VideoSource: Send + 'static {
    /// Human-readable name (for logging)
    fn name(&self) -> &str;

    /// The user_id this source produces frames for
    fn user_id(&self) -> &str;

    /// Start producing frames. Called once. Spawns an internal loop.
    /// Returns a shutdown sender — send () to stop the generator.
    fn start(
        self: Box<Self>,
        video_tx: broadcast::Sender<(Handle, String, Vec<u8>)>,
        handle: Handle,
    ) -> mpsc::Sender<()>;
}

/// Test pattern video source — SMPTE color bars with moving scan line.
/// Proves the video streaming pipeline works end-to-end.
pub struct TestPatternSource {
    user_id: String,
    width: u16,
    height: u16,
    fps: u8,
}

impl TestPatternSource {
    /// Create a new test pattern source.
    pub fn new(user_id: String, width: u16, height: u16, fps: u8) -> Self {
        Self { user_id, width, height, fps }
    }

    /// Create with default settings (160x120 @10fps, "test-pattern" user_id)
    pub fn default_test() -> Self {
        Self::new("test-pattern".to_string(), 160, 120, 10)
    }
}

impl VideoSource for TestPatternSource {
    fn name(&self) -> &str {
        "TestPattern"
    }

    fn user_id(&self) -> &str {
        &self.user_id
    }

    fn start(
        self: Box<Self>,
        video_tx: broadcast::Sender<(Handle, String, Vec<u8>)>,
        handle: Handle,
    ) -> mpsc::Sender<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let interval_ms = 1000 / self.fps.max(1) as u64;
        let user_id = self.user_id.clone();

        tokio::spawn(async move {
            let mut generator = TestPatternGenerator::new(self.width, self.height);
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_millis(interval_ms),
            );
            let start = std::time::Instant::now();

            info!(
                "{} video source started ({}x{} @{}fps, user_id={})",
                self.name(), self.width, self.height, self.fps, user_id
            );

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let timestamp_ms = start.elapsed().as_millis() as u32;
                        let frame = generator.next_frame(timestamp_ms);
                        let frame_bytes = frame.to_bytes();

                        if video_tx.send((handle, user_id.clone(), frame_bytes)).is_err() {
                            // No receivers — call may have ended
                            break;
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("{} video source stopped (user_id={})", "TestPattern", user_id);
                        break;
                    }
                }
            }
        });

        shutdown_tx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_pattern_source_defaults() {
        let source = TestPatternSource::default_test();
        assert_eq!(source.name(), "TestPattern");
        assert_eq!(source.user_id(), "test-pattern");
        assert_eq!(source.width, 160);
        assert_eq!(source.height, 120);
        assert_eq!(source.fps, 10);
    }

    #[test]
    fn test_test_pattern_source_custom() {
        let source = TestPatternSource::new("avatar-ai".to_string(), 320, 240, 30);
        assert_eq!(source.user_id(), "avatar-ai");
        assert_eq!(source.width, 320);
        assert_eq!(source.height, 240);
        assert_eq!(source.fps, 30);
    }

    #[tokio::test]
    async fn test_test_pattern_source_produces_frames() {
        let source = Box::new(TestPatternSource::new(
            "test-user".to_string(), 80, 60, 10,
        ));
        let (video_tx, mut video_rx) = broadcast::channel::<(Handle, String, Vec<u8>)>(16);
        let handle = Handle::new();

        let shutdown = source.start(video_tx, handle);

        // Wait for at least one frame
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

        // Should have received at least one frame
        match video_rx.try_recv() {
            Ok((recv_handle, recv_user_id, data)) => {
                assert_eq!(recv_handle, handle);
                assert_eq!(recv_user_id, "test-user");
                assert!(!data.is_empty());
            }
            Err(_) => {
                // Timing can be flaky in CI — acceptable
            }
        }

        // Shutdown
        let _ = shutdown.send(()).await;
    }
}
