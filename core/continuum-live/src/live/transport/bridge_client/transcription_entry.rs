use super::*;

/// Captured transcription from STT.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TranscriptionEntry {
    pub call_id: String,
    pub speaker_id: String,
    pub speaker_name: String,
    pub text: String,
    pub timestamp_ms: u64,
}

pub type TranscriptionBuffer = Arc<tokio::sync::Mutex<VecDeque<TranscriptionEntry>>>;

/// Pending request — waiting for response from bridge.
pub(super) struct PendingRequest {
    pub(super) response: Mutex<Option<BridgeResponse>>,
    pub(super) signal: Condvar,
}

/// Bridge client — proxies LiveKit operations to livekit-bridge process.
///
/// Two planes over one socket (the data-plane doctrine, Joel 2026-07-30):
/// - **Control plane** — [`Self::send_command`]: request/response, registers a
///   pending entry and blocks for the ack. Session lifecycle verbs (join, agent
///   create/remove, ambient add/remove) live here. On the grid this is where a
///   remote caller acquires the media HANDLE — grid calls are commands.
/// - **Media plane** — [`Self::send_media`]: fire-and-forget through a bounded
///   queue drained by a dedicated pump thread. PCM and video frames live here.
///   A stalled bridge drops frames (loud, counted) instead of stalling the
///   caller behind a 30s control timeout — media must never starve, and stale
///   media is garbage anyway.
pub struct LiveKitAgentManager {
    /// Shared write access to the bridge socket (control writes + media pump).
    pub(super) writer: Arc<Mutex<Option<UnixStream>>>,
    /// Pending command responses keyed by request_id.
    pub(super) pending: Arc<Mutex<HashMap<u64, Arc<PendingRequest>>>>,
    /// Media-plane queue head. Replaced on every reconnect; the old pump thread
    /// exits when its receiver disconnects.
    pub(super) media_tx: Mutex<Option<std::sync::mpsc::SyncSender<MediaFrame>>>,
    /// Outbound binary media channels: (call, user, kind) → channel, bound once
    /// via `OpenMediaOut` — identity leaves the frame path entirely.
    pub(super) out_channels: Mutex<std::collections::HashMap<(String, String, u8), u16>>,
    pub(super) next_out_channel: AtomicU64,
    /// Frames dropped because the media queue was full (bridge stalled). Loud
    /// via rate-limited warn in [`Self::send_media`]; MUST stay 0 steady-state.
    pub(super) media_dropped: Arc<AtomicU64>,
    /// Enqueue-side per-second tally (probe summary, never a row per frame).
    pub(super) enqueue_tally: Mutex<super::super::pump_tally::PumpTally>,
    /// Drop-side per-second tally (same rule: a summary, never a row per frame).
    pub(super) drop_tally: Mutex<super::super::pump_tally::PumpTally>,
    /// Bridge socket path.
    pub(super) bridge_socket_path: String,
    /// LiveKit URL (for logging only — bridge has the actual connection).
    pub(super) livekit_url: String,
    /// Request ID counter.
    pub(super) next_request_id: AtomicU64,
    /// Transcription buffer (populated by event handler).
    pub(super) transcription_buffer: TranscriptionBuffer,
    /// Whether the reader thread is running.
    pub(super) reader_started: Mutex<bool>,
}

/// One queued media-plane frame on the BINARY plane (2026-09-01): a fixed
/// 8-byte header ([len][magic][kind][channel]), optional 4-byte video dims,
/// and the payload MOVED from the producer — at 30fps HD RGBA the payload is
/// ~3.7MB and is never copied after the pump hands it over (forward-on, no
/// copies — the airc law). The writer emits the parts back-to-back; identity
/// strings bound once at channel open, never per frame.
pub(super) struct MediaFrame {
    pub(super) header: [u8; 8],
    pub(super) dims: Option<[u8; 4]>,
    pub(super) payload: Vec<u8>,
    pub(super) kind: &'static str,
}

impl MediaFrame {
    pub(super) fn new(
        kind: continuum_bridge_protocol::MediaKind,
        channel: u16,
        dims: Option<(u16, u16)>,
        payload: Vec<u8>,
        label: &'static str,
    ) -> Self {
        let dim_len = if dims.is_some() { 4 } else { 0 };
        let total = 1 + 1 + 2 + dim_len + payload.len();
        let mut header = [0u8; 8];
        header[0..4].copy_from_slice(&(total as u32).to_le_bytes());
        header[4] = continuum_bridge_protocol::MEDIA_MAGIC;
        header[5] = kind as u8;
        header[6..8].copy_from_slice(&channel.to_le_bytes());
        let dims = dims.map(|(w, h)| {
            let mut d = [0u8; 4];
            d[0..2].copy_from_slice(&w.to_le_bytes());
            d[2..4].copy_from_slice(&h.to_le_bytes());
            d
        });
        Self {
            header,
            dims,
            payload,
            kind: label,
        }
    }

    pub(super) fn wire_len(&self) -> usize {
        8 + self.dims.map_or(0, |_| 4) + self.payload.len()
    }
}

/// Media queue depth. Sized for burst absorption (~a second of mixed PCM chunks
/// and video frames), small enough that a stalled bridge surfaces as drops
/// within one breath rather than buffering minutes of stale media.
pub(super) const MEDIA_QUEUE_DEPTH: usize = 32;

/// i16 PCM → little-endian wire bytes in ONE allocation. The former per-site
/// `flat_map(to_le_bytes).collect()` has no useful size hint, so `collect`
/// re-allocates its way up the buffer on the per-turn audio hot path — the
/// "needless copies" class of the data-plane doctrine. This reserves exactly
/// `len * 2` up front; on little-endian targets LLVM lowers the loop to a memcpy.
pub(super) fn pcm_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
