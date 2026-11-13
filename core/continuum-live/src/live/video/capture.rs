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

use continuum_base::time::EpochMs;
use image::ImageBuffer;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::clog_warn;

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
}

/// Snapshot quality — lower = smaller payload, faster encoding.
/// 50 is adequate for AI vision models (they handle compression artifacts well).
const JPEG_QUALITY: u8 = 50;

/// Maximum snapshot age before it's considered stale (30 seconds).
const SNAPSHOT_MAX_AGE_MS: u64 = 30_000;

impl VideoFrameCapture {
    fn new() -> Self {
        Self {
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    /// Get or create the global singleton.
    pub fn instance() -> &'static Arc<VideoFrameCapture> {
        use std::sync::OnceLock;
        static INSTANCE: OnceLock<Arc<VideoFrameCapture>> = OnceLock::new();
        INSTANCE.get_or_init(|| Arc::new(VideoFrameCapture::new()))
    }

    /// Get the latest snapshot for a specific participant.
    pub async fn snapshot_participant(&self, identity: &str) -> Option<ParticipantSnapshot> {
        let snapshots = self.snapshots.lock().await;
        let snap = snapshots.get(identity)?;

        // Check freshness
        let now = EpochMs::now().millis();
        if now - snap.captured_at > SNAPSHOT_MAX_AGE_MS {
            return None;
        }

        Some(snap.clone())
    }

    /// Get snapshots for all participants with fresh frames.
    pub async fn snapshot_all(&self) -> Vec<ParticipantSnapshot> {
        let snapshots = self.snapshots.lock().await;
        let now = EpochMs::now().millis();
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
            return participants.into_iter().next();
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

    let mut grid: image::RgbImage =
        ImageBuffer::from_pixel(grid_w, grid_h, image::Rgb([32, 32, 32]));

    for (i, snap) in participants.iter().enumerate() {
        let col = (i as u32) % cols;
        let row = (i as u32) / cols;
        let x_offset = col * cell_w;
        let y_offset = row * cell_h;

        // Decode participant JPEG
        let reader =
            image::ImageReader::new(std::io::Cursor::new(&snap.jpeg)).with_guessed_format();
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

    let names: Vec<&str> = participants
        .iter()
        .map(|p| p.display_name.as_str())
        .collect();

    Some(ParticipantSnapshot {
        jpeg: jpeg_buf,
        hash,
        identity: "room-composite".to_string(),
        display_name: names.join(", "),
        width: grid_w,
        height: grid_h,
        captured_at: EpochMs::now().millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_ms_is_reasonable() {
        let ms = EpochMs::now().millis();
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
            captured_at: EpochMs::now().millis(),
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
                captured_at: EpochMs::now().millis(),
            }
        };

        let snaps = vec![
            make_snap("A"),
            make_snap("B"),
            make_snap("C"),
            make_snap("D"),
        ];
        let result = compose_grid(&snaps);
        assert!(result.is_some());
        let grid = result.unwrap();
        // 4 participants → 2x2 grid → 640x480
        assert_eq!(grid.width, 640);
        assert_eq!(grid.height, 480);
        assert!(!grid.jpeg.is_empty());
    }
}
