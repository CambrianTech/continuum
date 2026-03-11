//! VideoFrameCapture — Captures latest video frames from LiveKit room participants.
//!
//! Subscribes to remote video tracks and stores the most recent frame per participant
//! as a JPEG-encoded snapshot. AI personas query this via IPC to "see" the live room.
//!
//! Architecture:
//!   LiveKit Room → TrackSubscribed (Video) → NativeVideoStream → I420 → RGBA → JPEG
//!   Cached per participant identity, content-addressed to skip identical frames.
//!
//! Thread safety: All state behind Arc<Mutex<>> — multiple agents can query concurrently.
//! Frame processing runs on tokio::spawn_blocking (CPU-bound I420→RGBA→JPEG conversion).

use image::{ImageBuffer, RgbaImage};
use livekit::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{clog_info, clog_warn};

/// Captured snapshot of a single participant's video frame.
#[derive(Clone)]
pub struct ParticipantSnapshot {
    /// JPEG-encoded frame data
    pub jpeg: Vec<u8>,
    /// Content hash (first 16 hex chars of SHA-256) for dedup
    pub hash: String,
    /// Participant identity (user_id)
    pub identity: String,
    /// Participant display name
    pub display_name: String,
    /// Frame dimensions
    pub width: u32,
    pub height: u32,
    /// When this frame was captured (epoch ms)
    pub captured_at: u64,
}

/// Singleton service that captures and caches video frames from LiveKit participants.
pub struct VideoFrameCapture {
    /// Latest snapshot per participant identity
    snapshots: Mutex<HashMap<String, ParticipantSnapshot>>,
    /// Track which participants have active capture tasks
    active_captures: Mutex<HashMap<String, ()>>,
}

/// Snapshot quality — lower = smaller payload, faster encoding.
/// 50 is adequate for AI vision models (they handle compression artifacts well).
const JPEG_QUALITY: u8 = 50;

/// Maximum frames per second to process per participant.
/// We don't need every frame — 1 fps is plenty for visual awareness.
/// Higher rates waste CPU on I420→RGBA→JPEG conversion.
const MAX_CAPTURE_FPS: f64 = 1.0;

/// Maximum snapshot age before it's considered stale (30 seconds).
const SNAPSHOT_MAX_AGE_MS: u64 = 30_000;

impl VideoFrameCapture {
    fn new() -> Self {
        Self {
            snapshots: Mutex::new(HashMap::new()),
            active_captures: Mutex::new(HashMap::new()),
        }
    }

    /// Get or create the global singleton.
    pub fn instance() -> &'static Arc<VideoFrameCapture> {
        use std::sync::OnceLock;
        static INSTANCE: OnceLock<Arc<VideoFrameCapture>> = OnceLock::new();
        INSTANCE.get_or_init(|| Arc::new(VideoFrameCapture::new()))
    }

    /// Start capturing frames from a remote video track.
    /// Spawns a background task that reads frames from the NativeVideoStream,
    /// rate-limits to MAX_CAPTURE_FPS, and stores JPEG snapshots.
    ///
    /// Idempotent — no-op if already capturing for this identity.
    pub async fn start_capture(
        self: &Arc<Self>,
        video_track: RemoteVideoTrack,
        identity: String,
        display_name: String,
    ) {
        // DISABLED: Video capture tasks allocate large I420→RGBA→JPEG buffers
        // on spawn_blocking threads. Investigating memory growth.
        clog_info!("👁 Video capture DISABLED for '{}' (memory investigation)", &identity[..8.min(identity.len())]);
        return;

        // Check if already capturing
        {
            let captures = self.active_captures.lock().await;
            if captures.contains_key(&identity) {
                return;
            }
        }

        // Register as active
        {
            let mut captures = self.active_captures.lock().await;
            captures.insert(identity.clone(), ());
        }

        let capture = self.clone();
        let id = identity.clone();
        let name = display_name.clone();

        tokio::spawn(async move {
            capture_video_stream(capture, video_track, id, name).await;
        });

        clog_info!(
            "👁 Started video capture for '{}' ({})",
            display_name,
            &identity[..8.min(identity.len())]
        );
    }

    /// Get the latest snapshot for a specific participant.
    pub async fn snapshot_participant(&self, identity: &str) -> Option<ParticipantSnapshot> {
        let snapshots = self.snapshots.lock().await;
        let snap = snapshots.get(identity)?;

        // Check freshness
        let now = epoch_ms();
        if now - snap.captured_at > SNAPSHOT_MAX_AGE_MS {
            return None;
        }

        Some(snap.clone())
    }

    /// Get snapshots for all participants with fresh frames.
    pub async fn snapshot_all(&self) -> Vec<ParticipantSnapshot> {
        let snapshots = self.snapshots.lock().await;
        let now = epoch_ms();
        snapshots
            .values()
            .filter(|s| now - s.captured_at <= SNAPSHOT_MAX_AGE_MS)
            .cloned()
            .collect()
    }

    /// Compose a grid JPEG of all participant snapshots.
    /// Returns None if no participants have fresh snapshots.
    pub async fn snapshot_room(&self) -> Option<ParticipantSnapshot> {
        let participants = self.snapshot_all().await;
        if participants.is_empty() {
            return None;
        }

        if participants.len() == 1 {
            return Some(participants.into_iter().next().unwrap());
        }

        // Compose grid on blocking thread (CPU-bound image manipulation)
        let result = tokio::task::spawn_blocking(move || compose_grid(&participants)).await;

        match result {
            Ok(Some(snap)) => Some(snap),
            Ok(None) => None,
            Err(e) => {
                clog_warn!("👁 Grid composition panicked: {}", e);
                None
            }
        }
    }

    /// Store a snapshot (called by the capture task).
    async fn store_snapshot(&self, snapshot: ParticipantSnapshot) {
        let mut snapshots = self.snapshots.lock().await;
        snapshots.insert(snapshot.identity.clone(), snapshot);
    }

    /// Remove capture registration when stream ends.
    async fn remove_capture(&self, identity: &str) {
        let mut captures = self.active_captures.lock().await;
        captures.remove(identity);
        // Also remove stale snapshot for this participant
        let mut snapshots = self.snapshots.lock().await;
        snapshots.remove(identity);
    }

    /// Remove snapshots for participants not in the active set.
    /// Call periodically or when participants leave.
    pub async fn evict_stale_snapshots(&self) {
        let active = self.active_captures.lock().await;
        let mut snapshots = self.snapshots.lock().await;
        let before = snapshots.len();
        snapshots.retain(|identity, _| active.contains_key(identity));
        let evicted = before - snapshots.len();
        if evicted > 0 {
            clog_info!(
                "👁 Evicted {} stale snapshots ({} active)",
                evicted,
                snapshots.len()
            );
        }
    }
}

/// Background task: reads video frames from a LiveKit remote track,
/// rate-limits captures, converts I420→RGBA→JPEG, and stores snapshots.
async fn capture_video_stream(
    capture: Arc<VideoFrameCapture>,
    video_track: RemoteVideoTrack,
    identity: String,
    display_name: String,
) {
    use livekit::webrtc::video_stream::native::NativeVideoStream;
    use tokio_stream::StreamExt;

    let video_stream = NativeVideoStream::new(video_track.rtc_track());

    let min_interval = std::time::Duration::from_secs_f64(1.0 / MAX_CAPTURE_FPS);
    let mut last_capture = std::time::Instant::now() - min_interval; // Allow immediate first capture
    let mut last_hash: Option<String> = None;
    let mut frame_count: u64 = 0;

    // NativeVideoStream implements livekit_runtime::Stream which is compatible with
    // tokio_stream::StreamExt via the StreamExt blanket impl on futures::Stream.
    // Pin it for safe polling.
    let mut pinned_stream = std::pin::pin!(video_stream);

    loop {
        let frame = match pinned_stream.next().await {
            Some(f) => f,
            None => break, // Stream ended (track unpublished or participant left)
        };

        frame_count += 1;

        // Rate limit — skip frames if we captured recently
        let elapsed = last_capture.elapsed();
        if elapsed < min_interval {
            continue;
        }

        let width = frame.buffer.width();
        let height = frame.buffer.height();

        if width == 0 || height == 0 {
            continue;
        }

        // Convert I420 → RGBA → JPEG on blocking thread
        let id = identity.clone();
        let name = display_name.clone();
        let prev_hash = last_hash.clone();

        let result = tokio::task::spawn_blocking(move || {
            // Convert to I420 first, then I420→RGBA manually.
            let i420 = frame.buffer.to_i420();
            let rgba_buf = i420_to_rgba(&i420, width, height);

            // Content hash for dedup (first 4KB of RGBA data)
            let hash_input_len = rgba_buf.len().min(4096);
            let hash = {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                rgba_buf[..hash_input_len].hash(&mut hasher);
                format!("{:016x}", hasher.finish())
            };

            // Skip if frame is identical to last capture
            if prev_hash.as_deref() == Some(&hash) {
                return (None, hash);
            }

            // RGBA → RGB → JPEG (JPEG doesn't support alpha channel)
            let img: RgbaImage = match ImageBuffer::from_raw(width, height, rgba_buf) {
                Some(img) => img,
                None => return (None, hash),
            };
            let rgb_img: image::RgbImage = image::DynamicImage::ImageRgba8(img).to_rgb8();

            let mut jpeg_buf = Vec::with_capacity((width * height) as usize);
            let mut cursor = std::io::Cursor::new(&mut jpeg_buf);
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut cursor,
                JPEG_QUALITY,
            );
            if let Err(e) = rgb_img.write_with_encoder(encoder) {
                clog_warn!("👁 JPEG encode failed for '{}': {}", &id[..8.min(id.len())], e);
                return (None, hash);
            }

            let snapshot = ParticipantSnapshot {
                jpeg: jpeg_buf,
                hash: hash.clone(),
                identity: id,
                display_name: name,
                width,
                height,
                captured_at: epoch_ms(),
            };

            (Some(snapshot), hash)
        })
        .await;

        match result {
            Ok((Some(snapshot), hash)) => {
                let jpeg_kb = snapshot.jpeg.len() / 1024;
                if frame_count <= 3 || frame_count.is_multiple_of(60) {
                    clog_info!(
                        "👁 Captured frame #{} from '{}': {}x{}, {}KB JPEG",
                        frame_count,
                        &identity[..8.min(identity.len())],
                        snapshot.width,
                        snapshot.height,
                        jpeg_kb
                    );
                }
                capture.store_snapshot(snapshot).await;
                last_hash = Some(hash);
                last_capture = std::time::Instant::now();
            }
            Ok((None, hash)) => {
                // Frame was identical or encoding failed — update hash anyway
                last_hash = Some(hash);
                last_capture = std::time::Instant::now();
            }
            Err(e) => {
                clog_warn!("👁 Frame processing panicked for '{}': {}", identity, e);
            }
        }
    }

    capture.remove_capture(&identity).await;
    clog_info!(
        "👁 Video capture ended for '{}' after {} frames",
        &identity[..8.min(identity.len())],
        frame_count
    );
}

/// Compose multiple participant snapshots into a single grid JPEG.
fn compose_grid(participants: &[ParticipantSnapshot]) -> Option<ParticipantSnapshot> {
    if participants.is_empty() {
        return None;
    }

    // Determine grid layout (try to keep aspect ratio reasonable)
    let count = participants.len();
    let cols = (count as f64).sqrt().ceil() as u32;
    let rows = (count as u32).div_ceil(cols);

    // Target cell size — scale each participant's frame to fit
    let cell_w: u32 = 320;
    let cell_h: u32 = 240;

    let grid_w = cols * cell_w;
    let grid_h = rows * cell_h;

    let mut grid: image::RgbImage = ImageBuffer::from_pixel(grid_w, grid_h, image::Rgb([32, 32, 32]));

    for (i, snap) in participants.iter().enumerate() {
        let col = (i as u32) % cols;
        let row = (i as u32) / cols;
        let x_offset = col * cell_w;
        let y_offset = row * cell_h;

        // Decode participant JPEG
        let reader = image::ImageReader::new(std::io::Cursor::new(&snap.jpeg))
            .with_guessed_format();
        let img = match reader {
            Ok(r) => match r.decode() {
                Ok(img) => img,
                Err(_) => continue,
            },
            Err(_) => continue,
        };

        // Resize to cell dimensions
        let resized = img.resize_exact(cell_w, cell_h, image::imageops::FilterType::Triangle);
        let rgb = resized.to_rgb8();

        // Overlay onto grid
        image::imageops::overlay(&mut grid, &rgb, x_offset as i64, y_offset as i64);
    }

    // Encode composite grid as JPEG (RGB — no alpha needed)
    let mut jpeg_buf = Vec::with_capacity((grid_w * grid_h) as usize);
    let mut cursor = std::io::Cursor::new(&mut jpeg_buf);
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, JPEG_QUALITY);
    if grid.write_with_encoder(encoder).is_err() {
        return None;
    }

    // Composite hash from all participant hashes
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for p in participants {
            p.hash.hash(&mut hasher);
        }
        format!("{:016x}", hasher.finish())
    };

    let names: Vec<&str> = participants.iter().map(|p| p.display_name.as_str()).collect();

    Some(ParticipantSnapshot {
        jpeg: jpeg_buf,
        hash,
        identity: "room-composite".to_string(),
        display_name: names.join(", "),
        width: grid_w,
        height: grid_h,
        captured_at: epoch_ms(),
    })
}

/// Convert I420 YUV buffer to RGBA pixel data.
/// I420 layout: full-resolution Y plane, half-resolution U and V planes.
fn i420_to_rgba(i420: &livekit::webrtc::video_frame::I420Buffer, width: u32, height: u32) -> Vec<u8> {
    let (data_y, data_u, data_v) = i420.data();
    let (stride_y, stride_u, stride_v) = i420.strides();

    let mut rgba = vec![0u8; (width * height * 4) as usize];

    for y in 0..height {
        for x in 0..width {
            let y_idx = (y * stride_y + x) as usize;
            let uv_x = (x / 2) as usize;
            let uv_y = (y / 2) as usize;
            let u_idx = uv_y * stride_u as usize + uv_x;
            let v_idx = uv_y * stride_v as usize + uv_x;

            let y_val = data_y.get(y_idx).copied().unwrap_or(0) as f32;
            let u_val = data_u.get(u_idx).copied().unwrap_or(128) as f32 - 128.0;
            let v_val = data_v.get(v_idx).copied().unwrap_or(128) as f32 - 128.0;

            // BT.601 YUV→RGB conversion
            let r = (y_val + 1.402 * v_val).clamp(0.0, 255.0) as u8;
            let g = (y_val - 0.344136 * u_val - 0.714136 * v_val).clamp(0.0, 255.0) as u8;
            let b = (y_val + 1.772 * u_val).clamp(0.0, 255.0) as u8;

            let out_idx = ((y * width + x) * 4) as usize;
            rgba[out_idx] = r;
            rgba[out_idx + 1] = g;
            rgba[out_idx + 2] = b;
            rgba[out_idx + 3] = 255;
        }
    }

    rgba
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_ms_is_reasonable() {
        let ms = epoch_ms();
        // Should be after 2024 (in milliseconds)
        assert!(ms > 1_700_000_000_000);
    }

    #[test]
    fn test_compose_grid_empty() {
        assert!(compose_grid(&[]).is_none());
    }

    #[test]
    fn test_compose_grid_single() {
        // Create a minimal 2x2 JPEG (RGB — JPEG doesn't support alpha)
        let img: image::RgbImage = ImageBuffer::from_pixel(2, 2, image::Rgb([255, 0, 0]));
        let mut jpeg = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut jpeg);
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 50);
        img.write_with_encoder(encoder).unwrap();

        let snap = ParticipantSnapshot {
            jpeg,
            hash: "abc123".to_string(),
            identity: "test-user".to_string(),
            display_name: "Test".to_string(),
            width: 2,
            height: 2,
            captured_at: epoch_ms(),
        };

        // Single participant → returns as-is (no grid composition)
        // (compose_grid is only called from snapshot_room which short-circuits for len==1)
        let result = compose_grid(&[snap]);
        assert!(result.is_some());
        let grid = result.unwrap();
        assert_eq!(grid.width, 320);
        assert_eq!(grid.height, 240);
    }

    #[test]
    fn test_compose_grid_four() {
        let make_snap = |name: &str| {
            let img: image::RgbImage = ImageBuffer::from_pixel(4, 4, image::Rgb([0, 128, 255]));
            let mut jpeg = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut jpeg);
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 50);
            img.write_with_encoder(encoder).unwrap();

            ParticipantSnapshot {
                jpeg,
                hash: format!("hash-{}", name),
                identity: format!("id-{}", name),
                display_name: name.to_string(),
                width: 4,
                height: 4,
                captured_at: epoch_ms(),
            }
        };

        let snaps = vec![make_snap("A"), make_snap("B"), make_snap("C"), make_snap("D")];
        let result = compose_grid(&snaps);
        assert!(result.is_some());
        let grid = result.unwrap();
        // 4 participants → 2x2 grid → 640x480
        assert_eq!(grid.width, 640);
        assert_eq!(grid.height, 480);
        assert!(!grid.jpeg.is_empty());
    }
}
