use super::*;

pub(super) fn reader_loop(mut stream: UnixStream, pending: Arc<Mutex<HashMap<u64, Arc<PendingRequest>>>>) {
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut data = Vec::new();

    // Audio processing state per speaker (call_id + speaker_id → VAD state)
    let mut audio_processors: HashMap<String, AudioProcessor> = HashMap::new();
    // Inbound binary media channels (bound once via MediaChannelOpened).
    let mut in_channels: HashMap<u16, InMediaBinding> = HashMap::new();

    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => {
                clog_warn!("🌉 Bridge disconnected");
                break;
            }
            Ok(n) => n,
            Err(e) => {
                clog_warn!("🌉 Bridge read error: {}", e);
                break;
            }
        };

        data.extend_from_slice(&buf[..n]);

        while data.len() >= 4 {
            let frame_len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
            if data.len() < 4 + frame_len {
                break;
            }

            // BINARY MEDIA plane first (2026-09-01): one byte decides, the
            // frame is processed as a BORROW of the socket buffer — no
            // to_vec, no JSON, no double-parse. The old shape parsed every
            // 20ms audio frame as JSON TWICE (response-then-event) with four
            // identity strings each; at 50fps/speaker that was the
            // eat-the-cpu-at-the-boundary mistake the airc design forbids.
            if let Some((kind, channel, payload)) =
                continuum_bridge_protocol::parse_media_frame(&data[4..4 + frame_len])
            {
                match in_channels.get_mut(&channel) {
                    Some(binding) if binding.kind != kind => {
                        clog_warn!(
                            "🌉 media frame kind {:?} ≠ bound {:?} on ch {} — dropped",
                            kind,
                            binding.kind,
                            channel
                        );
                    }
                    Some(binding) => match kind {
                        continuum_bridge_protocol::MediaKind::AudioPcm => {
                            binding.frames += 1;
                            let samples: Vec<i16> = payload
                                .chunks_exact(2)
                                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                                .collect();
                            if let Some(tx) = HUMAN_AUDIO_TX.get() {
                                let saturated = tx
                                    .try_send(HumanAudioChunk {
                                        call_id: binding.call_id.clone(),
                                        user_id: binding.speaker_id.clone(),
                                        samples,
                                    })
                                    .is_err();
                                if saturated && binding.frames % 500 == 1 {
                                    clog_warn!(
                                        "🎤 human-audio forwarder saturated — dropping frames from '{}' to stay current",
                                        binding.speaker_name
                                    );
                                }
                            } else if binding.frames == 1 {
                                clog_warn!(
                                    "🎤 no human-audio forwarder installed — speech from '{}' cannot reach STT",
                                    binding.speaker_name
                                );
                            }
                        }
                        continuum_bridge_protocol::MediaKind::VideoJpeg => {
                            if let Some((_w, _h, jpeg)) =
                                continuum_bridge_protocol::parse_video_payload(payload)
                            {
                                crate::media::perception_ingest::try_enqueue(
                                    crate::media::perception_ingest::IngestFrame {
                                        received_at_ms: EpochMs::now().millis(),
                                        call_id: binding.call_id.to_string(),
                                        speaker_id: binding.speaker_id.to_string(),
                                        jpeg: jpeg.to_vec(),
                                        mime: "image/jpeg".to_string(),
                                    },
                                );
                            }
                        }
                        continuum_bridge_protocol::MediaKind::VideoRgba => {
                            clog_warn!("🌉 VideoRgba is outbound-only — dropped (ch {channel})");
                        }
                    },
                    None => {
                        clog_warn!("🌉 media frame on unbound in-channel {channel} — dropped");
                    }
                }
                data.drain(..4 + frame_len);
                continue;
            }

            let frame_data = data[4..4 + frame_len].to_vec();
            data.drain(..4 + frame_len);

            let (json_bytes, binary) = continuum_bridge_protocol::decode_frame(&frame_data);

            // Try as BridgeResponse first (has request_id)
            if let Ok(response) = serde_json::from_slice::<BridgeResponse>(json_bytes) {
                let mut map = pending.lock().unwrap();
                if let Some(req) = map.remove(&response.request_id) {
                    let mut resp = req.response.lock().unwrap();
                    *resp = Some(response);
                    req.signal.notify_one();
                }
                continue;
            }

            // Try as BridgeEvent (pushed from bridge)
            if let Ok(event) = serde_json::from_slice::<BridgeEvent>(json_bytes) {
                handle_bridge_event(event, binary, &mut audio_processors, &mut in_channels);
            }
        }
    }
}

/// One human speaker's PCM chunk (16k mono i16, the core-wide audio contract),
/// hopping from the bridge reader THREAD into async land where the CallServer
/// lives. Bounded; a saturated channel drops frames (stay-current rule).
pub struct HumanAudioChunk {
    pub call_id: std::sync::Arc<str>,
    pub user_id: std::sync::Arc<str>,
    pub samples: Vec<i16>,
}

/// The installed forwarder for bridge-heard human audio. Filled once at IPC
/// startup (beside the CallManager's construction) by
/// [`install_human_audio_forwarder`]; absent = the reader warns on a speaker's
/// first frame instead of silently eating speech.
pub(super) static HUMAN_AUDIO_TX: std::sync::OnceLock<tokio::sync::mpsc::Sender<HumanAudioChunk>> =
    std::sync::OnceLock::new();

/// Install the channel the bridge reader forwards human audio through.
pub fn install_human_audio_forwarder(tx: tokio::sync::mpsc::Sender<HumanAudioChunk>) {
    let _ = HUMAN_AUDIO_TX.set(tx);
}

/// Per-speaker audio processing state for the LEGACY JSON AudioFrame arm
/// (kept as wire-compat while older bridges drain; the binary plane's
/// per-channel state lives in [`InMediaBinding`]). track_sid was dropped when
/// the accumulate-into-VAD path moved to the CallServer — routing needs only
/// call + speaker identity.
pub(super) struct AudioProcessor {
    pub(super) call_id: String,
    pub(super) speaker_id: String,
    pub(super) speaker_name: String,
    pub(super) frame_count: u64,
}

impl AudioProcessor {
    pub(super) fn new(call_id: String, speaker_id: String, speaker_name: String) -> Self {
        Self {
            call_id,
            speaker_id,
            speaker_name,
            frame_count: 0,
        }
    }
}

/// Handle a pushed event from the bridge.
pub(super) fn handle_bridge_event(
    event: BridgeEvent,
    binary: Option<&[u8]>,
    processors: &mut HashMap<String, AudioProcessor>,
    in_channels: &mut HashMap<u16, InMediaBinding>,
) {
    match event {
        BridgeEvent::MediaChannelOpened {
            channel,
            kind,
            call_id,
            speaker_id,
            speaker_name,
            track_sid: _,
        } => {
            // Identity binds ONCE per (speaker, kind); every subsequent frame
            // on this channel is header + payload only. The Strings arriving
            // here are the LAST per-speaker identity allocations on this path.
            clog_info!(
                "🌉 media in-channel {} bound: {:?} from '{}' in call {}",
                channel,
                kind,
                speaker_name,
                &call_id[..8.min(call_id.len())]
            );
            in_channels.insert(
                channel,
                InMediaBinding {
                    kind,
                    call_id: call_id.into(),
                    speaker_id: speaker_id.into(),
                    speaker_name: speaker_name.into(),
                    frames: 0,
                },
            );
        }
        BridgeEvent::AudioFrame {
            call_id,
            speaker_id,
            speaker_name,
            track_sid: _,
            sample_count: _,
        } => {
            // Decode PCM samples from binary payload
            let samples: Vec<i16> = match binary {
                Some(bytes) => bytes
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect(),
                None => return, // No audio data
            };

            let key = format!("{}:{}", call_id, speaker_id);
            let processor = processors.entry(key).or_insert_with(|| {
                clog_info!(
                    "🎤 New audio processor for '{}' in call {}",
                    speaker_name,
                    &call_id[..8.min(call_id.len())]
                );
                AudioProcessor::new(call_id.clone(), speaker_id.clone(), speaker_name.clone())
            });

            processor.frame_count += 1;
            if processor.frame_count == 1 || processor.frame_count % 3000 == 0 {
                let max_amp = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
                clog_info!(
                    "🎤 Audio frame #{} from '{}': {} samples, max_amp={}",
                    processor.frame_count,
                    processor.speaker_name,
                    samples.len(),
                    max_amp
                );
            }

            // THE WIRE THAT WAS NEVER RUN (closed 2026-09-01): these frames
            // used to accumulate into perfect VAD-sized chunks and die in a
            // TODO — the bridge heard the first audio frame and citizens never
            // answered speech, because the pipeline simply STOPPED here. Now
            // each frame forwards to the CallServer's existing VAD →
            // speech-end → transcription path (the human's handle + VAD were
            // minted by her WS-control join all along). This loop is a plain
            // thread, so the hop to async land is one bounded channel; a full
            // channel drops the frame (stay current, never backlog — the same
            // rule the transcription semaphore applies downstream).
            if let Some(tx) = HUMAN_AUDIO_TX.get() {
                let chunk = HumanAudioChunk {
                    call_id: processor.call_id.as_str().into(),
                    user_id: processor.speaker_id.as_str().into(),
                    samples,
                };
                if tx.try_send(chunk).is_err() && processor.frame_count % 500 == 1 {
                    clog_warn!(
                        "🎤 human-audio forwarder saturated — dropping frames from '{}' to stay current",
                        processor.speaker_name
                    );
                }
            } else if processor.frame_count == 1 {
                clog_warn!(
                    "🎤 no human-audio forwarder installed — speech from '{}' cannot reach STT",
                    processor.speaker_name
                );
            }
        }
        BridgeEvent::ParticipantJoined {
            call_id,
            identity,
            name,
        } => {
            clog_info!(
                "👤 Bridge: participant joined call {}: {} ({})",
                &call_id[..8.min(call_id.len())],
                name,
                &identity[..8.min(identity.len())]
            );
        }
        BridgeEvent::ParticipantLeft {
            ref call_id,
            ref identity,
        } => {
            clog_info!(
                "👤 Bridge: participant left call {}: {}",
                &call_id[..8.min(call_id.len())],
                &identity[..8.min(identity.len())]
            );
            // Clean up audio processor for this speaker
            let key = format!("{}:{}", call_id, identity);
            processors.remove(&key);
        }
        BridgeEvent::ListenerReady { call_id } => {
            clog_info!(
                "🎤 Bridge: STT listener ready for call {}",
                &call_id[..8.min(call_id.len())]
            );
        }
        BridgeEvent::RoomDisconnected { call_id, reason } => {
            clog_warn!(
                "🌉 Bridge: room disconnected for call {}: {}",
                &call_id[..8.min(call_id.len())],
                reason
            );
            // Clean up all processors for this call
            processors.retain(|k, _| !k.starts_with(&format!("{}:", call_id)));
        }
        BridgeEvent::VideoFrame {
            call_id,
            speaker_id,
            speaker_name,
            width,
            height,
        } => {
            if let Some(jpeg) = binary {
                #[cfg(feature = "livekit-webrtc")]
                {
                    // When livekit-webrtc is enabled, VideoFrameCapture uses LiveKit directly.
                    // The bridge path is for when livekit-webrtc is disabled. Mark the
                    // bridge-path bindings as used so this cfg doesn't emit unused-variable
                    // warnings (which also trip a rustc 1.94 annotate-snippets render ICE here).
                    let _ = (&call_id, &speaker_id, &speaker_name, &width, &height, &jpeg);
                }
                #[cfg(not(feature = "livekit-webrtc"))]
                {
                    static FRAME_COUNT: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(0);
                    let count = FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
                    if count == 0 || count % 60 == 0 {
                        clog_info!(
                            "👁 Video frame #{} from '{}': {}x{} ({}KB JPEG)",
                            count,
                            speaker_name,
                            width,
                            height,
                            jpeg.len() / 1024
                        );
                    }
                    // #192: hand the already-encoded frame to perception ingest — a
                    // NON-BLOCKING post from this reader thread (no reactor here) onto
                    // the drain's mpsc; a tokio task fans it out to each persona-viewer's
                    // PerceptionBuffer. Drops silently until the drain is installed. This
                    // is a COPY the bridge already made — the human display plane (the
                    // LiveKit video track) is never touched, and perception samples it at
                    // ~2 Hz regardless of the frame rate ([[perceive-the-room-as-it-is-now]]).
                    crate::media::perception_ingest::try_enqueue(
                        crate::media::perception_ingest::IngestFrame {
                            received_at_ms: EpochMs::now().millis(),
                            call_id,
                            speaker_id,
                            jpeg: jpeg.to_vec(),
                            mime: "image/jpeg".to_string(),
                        },
                    );
                }
            }
        }
        BridgeEvent::AgentConnected {
            call_id, user_id, ..
        } => {
            clog_info!(
                "🔊 Bridge: agent connected in call {}: {}",
                &call_id[..8.min(call_id.len())],
                &user_id[..8.min(user_id.len())]
            );
        }
        BridgeEvent::AgentDisconnected {
            call_id,
            user_id,
            reason,
        } => {
            clog_info!(
                "🔊 Bridge: agent disconnected from call {}: {} ({})",
                &call_id[..8.min(call_id.len())],
                &user_id[..8.min(user_id.len())],
                reason
            );
        }
        _ => {}
    }
}

// =============================================================================
// RMS calculation for mouth weight animation
// =============================================================================
#[cfg(feature = "avatar-3d")]
pub(super) fn calculate_rms_weights(samples: &[i16], sample_rate: u32, window_ms: u32) -> Vec<f32> {
    let window_size = (sample_rate as usize * window_ms as usize) / 1000;
    if window_size == 0 || samples.is_empty() {
        return vec![];
    }
    samples
        .chunks(window_size)
        .map(|chunk| {
            let sum_sq: f64 = chunk.iter().map(|&s| (s as f64) * (s as f64)).sum();
            let rms = (sum_sq / chunk.len() as f64).sqrt();
            // Mouth-open weight from windowed RMS. Divisor is the sensitivity: at
            // /8000 the mouth barely cracked open on normal speech (peak ~0.3), so
            // lip-sync read as near-static. /4000 (2x gain) opens the mouth clearly
            // on loud vowels (→1.0, capped) while quiet syllables stay small — a
            // natural, visible range instead of a subtle flutter.
            (rms / 4000.0).min(1.0) as f32
        })
        .collect()
}
