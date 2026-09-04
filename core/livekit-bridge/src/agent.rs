//! LiveKit agent management — connects AI personas to LiveKit rooms.
//!
//! Extracted from continuum-core's livekit_agent.rs. This module handles ALL
//! LiveKit WebRTC operations: room connections, audio/video track publishing,
//! STT listener audio streaming. No ort, no VAD, no STT — just transport.

use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit_api::access_token::{AccessToken, VideoGrants};

use continuum_bridge_protocol::{ParticipantMetadata, ParticipantRole, SAMPLE_RATE, SAMPLES_PER_10MS};

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{info, warn};

/// Event with optional binary payload (e.g., PCM audio samples).
pub struct EventWithPayload {
    pub event: continuum_bridge_protocol::BridgeEvent,
    pub binary: Option<Vec<u8>>,
}

// =============================================================================
// Constants
// =============================================================================

const DEV_API_KEY: &str = "devkey";
const DEV_API_SECRET: &str = "secret";

// =============================================================================
// Agent — one per AI persona per call
// =============================================================================

/// Server-side LiveKit participant for an AI persona.
/// Publishes TTS audio and avatar video into a LiveKit room.
pub struct Agent {
    room: Room,
    audio_source: NativeAudioSource,
    audio_track_sid: String,
    video_source: Mutex<Option<NativeVideoSource>>,
    identity: String,
    display_name: String,
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl Agent {
    /// Connect to a LiveKit room as an AI persona.
    pub async fn connect(
        livekit_url: &str,
        call_id: &str,
        persona_id: &str,
        persona_name: &str,
    ) -> Result<Self, String> {
        let metadata = ParticipantMetadata::new(ParticipantRole::AiPersona);
        let token = generate_token(persona_id, persona_name, call_id, true, &metadata)?;

        let mut room_opts = RoomOptions::default();
        room_opts.auto_subscribe = false;
        let (room, _room_events) = Room::connect(livekit_url, &token, room_opts)
            .await
            .map_err(|e| format!("Failed to connect agent: {}", e))?;

        info!("🔊 Agent '{}' connected to room '{}'", persona_name, call_id);

        // Create and publish audio track
        let audio_source = NativeAudioSource::new(
            AudioSourceOptions::default(),
            SAMPLE_RATE,
            1,      // mono
            30_000, // 30s buffer
        );
        let audio_track = LocalAudioTrack::create_audio_track(
            &format!("{}-voice", persona_id),
            RtcAudioSource::Native(audio_source.clone()),
        );
        let audio_pub = room
            .local_participant()
            .publish_track(
                LocalTrack::Audio(audio_track),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("Failed to publish audio track: {}", e))?;

        let audio_track_sid: String = audio_pub.sid().into();
        info!("🔊 Audio track published: {}", &audio_track_sid[..8.min(audio_track_sid.len())]);

        let (shutdown_tx, _shutdown_rx) = tokio::sync::watch::channel(false);

        Ok(Self {
            room,
            audio_source,
            audio_track_sid,
            video_source: Mutex::new(None),
            identity: persona_id.to_string(),
            display_name: persona_name.to_string(),
            shutdown_tx,
        })
    }

    /// Feed TTS audio (i16 PCM 16kHz mono) to the LiveKit room.
    pub async fn speak(&self, samples: Vec<i16>) -> Result<(), String> {
        let chunk_size = SAMPLES_PER_10MS as usize;
        for chunk in samples.chunks(chunk_size) {
            let frame = AudioFrame {
                data: Cow::Borrowed(chunk),
                sample_rate: SAMPLE_RATE,
                num_channels: 1,
                samples_per_channel: chunk.len() as u32,
            };
            self.audio_source
                .capture_frame(&frame)
                .await
                .map_err(|e| format!("Failed to capture audio frame: {}", e))?;
        }
        Ok(())
    }

    /// Publish RGBA video frame as avatar.
    pub async fn publish_video_frame(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let mut vs = self.video_source.lock().await;
        let source = match vs.as_ref() {
            Some(s) => s,
            None => {
                // Lazily create video source on first frame
                let new_source = NativeVideoSource::new(VideoResolution {
                    width,
                    height,
                }, false);
                let video_track = LocalVideoTrack::create_video_track(
                    &format!("{}-avatar", self.identity),
                    RtcVideoSource::Native(new_source.clone()),
                );
                self.room
                    .local_participant()
                    .publish_track(
                        LocalTrack::Video(video_track),
                        TrackPublishOptions::default(),
                    )
                    .await
                    .map_err(|e| format!("Failed to publish video track: {}", e))?;
                info!("📹 Video track published for '{}'", self.display_name);
                *vs = Some(new_source);
                vs.as_ref().unwrap()
            }
        };

        // Convert RGBA to I420
        let mut buffer = I420Buffer::new(width, height);
        rgba_to_i420_into(rgba, &mut buffer, width, height);

        let frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            buffer,
            timestamp_us: 0,
        };
        source.capture_frame(&frame);
        Ok(())
    }

    /// Publish transcription subtitle via data channel.
    pub async fn publish_transcription(&self, text: &str) -> Result<(), String> {
        let payload = serde_json::json!({
            "speaker_id": self.identity,
            "speaker_name": self.display_name,
            "text": text,
            "language": "en",
            "final": true,
        });
        self.room
            .local_participant()
            .publish_data(DataPacket {
                payload: payload.to_string().into_bytes(),
                topic: Some("transcription".to_string()),
                reliable: true,
                ..Default::default()
            })
            .await
            .map_err(|e| format!("Failed to publish transcription: {}", e))?;
        Ok(())
    }

    pub fn audio_track_sid(&self) -> &str {
        &self.audio_track_sid
    }

    pub async fn disconnect(&self) {
        info!("🔊 Agent '{}' disconnecting", self.identity);
        let _ = self.shutdown_tx.send(true);
        let _ = self.room.close().await;
    }
}

// =============================================================================
// AgentManager — manages agents + STT listeners across calls
// =============================================================================

type AgentKey = (String, String);

/// Manages all LiveKit agents and STT listeners.
pub struct AgentManager {
    agents: RwLock<HashMap<AgentKey, Arc<Agent>>>,
    listeners: RwLock<HashMap<String, Arc<Room>>>,
    livekit_url: String,
    /// Channel to send CONTROL events back to core (JSON plane).
    audio_tx: mpsc::UnboundedSender<EventWithPayload>,
    /// Channel to send continuous MEDIA frames back to core (binary plane —
    /// identity bound once via MediaChannelOpened, then [magic][kind][ch]+payload).
    media_tx: mpsc::UnboundedSender<(continuum_bridge_protocol::MediaKind, u16, Vec<u8>)>,
}

/// Bridge-assigned inbound channel ids (its own send-direction namespace).
static NEXT_IN_CHANNEL: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(1);

impl AgentManager {
    pub fn new(
        livekit_url: String,
        audio_tx: mpsc::UnboundedSender<EventWithPayload>,
        media_tx: mpsc::UnboundedSender<(continuum_bridge_protocol::MediaKind, u16, Vec<u8>)>,
    ) -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            listeners: RwLock::new(HashMap::new()),
            livekit_url,
            audio_tx,
            media_tx,
        }
    }

    /// Connect an agent for a persona.
    pub async fn join_room(
        &self,
        call_id: &str,
        user_id: &str,
        display_name: &str,
    ) -> Result<String, String> {
        let key = (call_id.to_string(), user_id.to_string());

        // Check if already connected
        {
            let agents = self.agents.read().await;
            if let Some(agent) = agents.get(&key) {
                return Ok(agent.audio_track_sid().to_string());
            }
        }

        let agent = Agent::connect(&self.livekit_url, call_id, user_id, display_name).await?;
        let sid = agent.audio_track_sid().to_string();
        let agent = Arc::new(agent);
        self.agents.write().await.insert(key, agent);
        Ok(sid)
    }

    /// Disconnect an agent.
    pub async fn leave_room(&self, call_id: &str, user_id: &str) {
        let key = (call_id.to_string(), user_id.to_string());
        if let Some(agent) = self.agents.write().await.remove(&key) {
            agent.disconnect().await;
        }
    }

    /// Disconnect all agents for a call.
    pub async fn leave_all(&self, call_id: &str) {
        let keys: Vec<AgentKey> = {
            let agents = self.agents.read().await;
            agents.keys().filter(|(c, _)| c == call_id).cloned().collect()
        };
        for (c, u) in keys {
            self.leave_room(&c, &u).await;
        }
    }

    /// Feed TTS audio to an agent.
    pub async fn speak(
        &self,
        call_id: &str,
        user_id: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let key = (call_id.to_string(), user_id.to_string());
        let agents = self.agents.read().await;
        let agent = agents.get(&key).ok_or("Agent not found")?;
        agent.speak(samples).await
    }

    /// Publish transcription subtitle for an agent.
    pub async fn publish_transcription(
        &self,
        call_id: &str,
        user_id: &str,
        text: &str,
    ) -> Result<(), String> {
        let key = (call_id.to_string(), user_id.to_string());
        let agents = self.agents.read().await;
        let agent = agents.get(&key).ok_or("Agent not found")?;
        agent.publish_transcription(text).await
    }

    /// Publish video frame for an agent.
    pub async fn publish_video_frame(
        &self,
        call_id: &str,
        user_id: &str,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let key = (call_id.to_string(), user_id.to_string());
        let agents = self.agents.read().await;
        let agent = agents.get(&key).ok_or("Agent not found")?;
        agent.publish_video_frame(rgba, width, height).await
    }

    /// Start STT listener — subscribes to human audio tracks and streams
    /// raw PCM back to core via the audio_tx channel.
    pub async fn start_listener(&self, call_id: &str) -> Result<(), String> {
        {
            let listeners = self.listeners.read().await;
            if listeners.contains_key(call_id) {
                info!("🎤 STT listener already active for {}", &call_id[..8.min(call_id.len())]);
                return Ok(());
            }
        }

        let listener_id = format!("stt-{}", &call_id[..8.min(call_id.len())]);
        let metadata = ParticipantMetadata::new(ParticipantRole::SttListener);
        let token = generate_token(&listener_id, "STT", call_id, true, &metadata)?;

        let (room, mut room_events) = Room::connect(&self.livekit_url, &token, RoomOptions::default())
            .await
            .map_err(|e| format!("Failed to connect STT listener: {}", e))?;

        info!("🎤 STT listener connected to room '{}'", &call_id[..8.min(call_id.len())]);

        let room = Arc::new(room);
        self.listeners.write().await.insert(call_id.to_string(), room.clone());

        // Notify core that listener is ready
        let _ = self.audio_tx.send(EventWithPayload {
            event: continuum_bridge_protocol::BridgeEvent::ListenerReady {
                call_id: call_id.to_string(),
            },
            binary: None,
        });

        let audio_tx = self.audio_tx.clone();
        let media_plane_tx = self.media_tx.clone();
        let call_id_owned = call_id.to_string();

        // Spawn event handler — subscribes to audio tracks and forwards PCM to core
        tokio::spawn(async move {
            while let Some(event) = room_events.recv().await {
                match event {
                    RoomEvent::TrackSubscribed { track, publication, participant } => {
                        let speaker_id = participant.identity().to_string();
                        let speaker_name = participant.name().to_string();
                        let meta = ParticipantMetadata::from_json(&participant.metadata());

                        let is_human = meta
                            .as_ref()
                            .map(|m| m.role == ParticipantRole::Human)
                            .unwrap_or(true);

                        match track {
                            RemoteTrack::Audio(audio_track) => {
                                if !is_human {
                                    info!("🎤 Skipping non-human audio from '{}'", speaker_id);
                                    continue;
                                }

                                let track_sid: String = publication.sid().into();
                                info!("🎤 Subscribed to audio from '{}' ({})", speaker_name, &speaker_id[..8.min(speaker_id.len())]);

                                let tx = audio_tx.clone();
                                let mtx = media_plane_tx.clone();
                                let cid = call_id_owned.clone();
                                let sid = speaker_id.clone();
                                let sname = speaker_name.clone();
                                let tsid = track_sid.clone();

                                // Spawn audio stream forwarder — reads LiveKit audio
                                // and sends raw PCM to core for VAD/STT processing
                                tokio::spawn(async move {
                                    forward_audio_to_core(
                                        audio_track, tx, mtx, cid, sid, sname, tsid,
                                    ).await;
                                });
                            }
                            RemoteTrack::Video(video_track) => {
                                let is_visible = meta.as_ref().map(|m| {
                                    m.role == ParticipantRole::Human || m.role == ParticipantRole::AiPersona
                                }).unwrap_or(true);
                                if !is_visible {
                                    continue;
                                }

                                info!("👁 Subscribed to video from '{}' ({})", speaker_name, &speaker_id[..8.min(speaker_id.len())]);

                                let tx = audio_tx.clone();
                                let mtx = media_plane_tx.clone();
                                let cid = call_id_owned.clone();
                                let sid = speaker_id.clone();
                                let sname = speaker_name.clone();

                                tokio::spawn(async move {
                                    forward_video_to_core(video_track, tx, mtx, cid, sid, sname)
                                        .await;
                                });
                            }
                        }
                    }
                    RoomEvent::ParticipantConnected(p) => {
                        let meta = ParticipantMetadata::from_json(&p.metadata());
                        let is_visible = meta.as_ref().map(|m| {
                            m.role == ParticipantRole::Human || m.role == ParticipantRole::AiPersona
                        }).unwrap_or(true);
                        if is_visible {
                            let _ = audio_tx.send(EventWithPayload {
                                event: continuum_bridge_protocol::BridgeEvent::ParticipantJoined {
                                    call_id: call_id_owned.clone(),
                                    identity: p.identity().to_string(),
                                    name: p.name().to_string(),
                                },
                                binary: None,
                            });
                        }
                    }
                    RoomEvent::ParticipantDisconnected(p) => {
                        let _ = audio_tx.send(EventWithPayload {
                            event: continuum_bridge_protocol::BridgeEvent::ParticipantLeft {
                                call_id: call_id_owned.clone(),
                                identity: p.identity().to_string(),
                            },
                            binary: None,
                        });
                    }
                    RoomEvent::Disconnected { reason } => {
                        info!("🎤 STT listener disconnected: {:?}", reason);
                        let _ = audio_tx.send(EventWithPayload {
                            event: continuum_bridge_protocol::BridgeEvent::RoomDisconnected {
                                call_id: call_id_owned.clone(),
                                reason: format!("{:?}", reason),
                            },
                            binary: None,
                        });
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    /// Stop STT listener for a call.
    pub async fn stop_listener(&self, call_id: &str) {
        if let Some(room) = self.listeners.write().await.remove(call_id) {
            info!("🎤 Stopping STT listener for {}", &call_id[..8.min(call_id.len())]);
            let _ = room.close().await;
        }
    }
}

// =============================================================================
// Audio stream forwarder — LiveKit → core (raw PCM)
// =============================================================================

/// Read audio frames from a LiveKit audio track and forward as raw PCM to core.
/// Core handles VAD/STT — bridge just transports bytes.
async fn forward_audio_to_core(
    audio_track: RemoteAudioTrack,
    tx: mpsc::UnboundedSender<EventWithPayload>,
    media_tx: mpsc::UnboundedSender<(continuum_bridge_protocol::MediaKind, u16, Vec<u8>)>,
    call_id: String,
    speaker_id: String,
    speaker_name: String,
    track_sid: String,
) {
    use livekit::webrtc::audio_stream::native::NativeAudioStream;
    use futures_util::StreamExt;

    let mut stream = NativeAudioStream::new(
        audio_track.rtc_track(),
        SAMPLE_RATE as i32,
        1, // mono
    );

    // BIND ONCE, THEN STREAM (binary plane, 2026-09-01): the old shape sent a
    // JSON `AudioFrame` event — four identity strings, a serialize here and a
    // DOUBLE parse in core — 50×/sec/speaker. Identity now binds once; every
    // frame after is [magic][kind][channel] + raw PCM.
    let channel = NEXT_IN_CHANNEL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let _ = tx.send(EventWithPayload {
        event: continuum_bridge_protocol::BridgeEvent::MediaChannelOpened {
            channel,
            kind: continuum_bridge_protocol::MediaKind::AudioPcm,
            call_id,
            speaker_id,
            speaker_name: speaker_name.clone(),
            track_sid,
        },
        binary: None,
    });

    let mut frame_count: u64 = 0;
    while let Some(frame) = stream.next().await {
        frame_count += 1;
        let samples: &[i16] = frame.data.as_ref();

        if frame_count == 1 {
            info!(
                "🎤 First audio frame from '{}': {} samples, sr={} → binary ch {}",
                speaker_name, samples.len(), frame.sample_rate, channel
            );
        }

        // Raw PCM to core (LE bytes) — core does VAD/STT.
        let mut pcm_bytes: Vec<u8> = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            pcm_bytes.extend_from_slice(&s.to_le_bytes());
        }

        if media_tx
            .send((
                continuum_bridge_protocol::MediaKind::AudioPcm,
                channel,
                pcm_bytes,
            ))
            .is_err()
        {
            warn!("🎤 Media channel closed for '{}'", speaker_name);
            break;
        }
    }

    info!("🎤 Audio stream ended for '{}'", speaker_name);
}

// =============================================================================
// Video stream forwarder — LiveKit → core (JPEG snapshots)
// =============================================================================

/// Capture video frames from a LiveKit participant, convert to JPEG,
/// and forward to core for vision processing.
/// Rate-limited to 1 fps — AI vision doesn't need every frame.
#[allow(clippy::too_many_arguments)]
async fn forward_video_to_core(
    video_track: RemoteVideoTrack,
    tx: mpsc::UnboundedSender<EventWithPayload>,
    media_tx: mpsc::UnboundedSender<(continuum_bridge_protocol::MediaKind, u16, Vec<u8>)>,
    call_id: String,
    speaker_id: String,
    speaker_name: String,
) {
    use livekit::webrtc::video_stream::native::NativeVideoStream;
    use futures_util::StreamExt;

    let video_stream = NativeVideoStream::new(video_track.rtc_track());
    let mut pinned_stream = std::pin::pin!(video_stream);

    // BIND ONCE, THEN STREAM (binary plane — same concept as audio): identity
    // rides one MediaChannelOpened; every sampled JPEG after is
    // [magic][kind][channel][w][h] + bytes. Vision cognition samples at 1fps —
    // 30fps HD into a mind that perceives once a second is pure waste.
    let channel = NEXT_IN_CHANNEL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let _ = tx.send(EventWithPayload {
        event: continuum_bridge_protocol::BridgeEvent::MediaChannelOpened {
            channel,
            kind: continuum_bridge_protocol::MediaKind::VideoJpeg,
            call_id: call_id.clone(),
            speaker_id: speaker_id.clone(),
            speaker_name: speaker_name.clone(),
            track_sid: String::new(),
        },
        binary: None,
    });

    let min_interval = std::time::Duration::from_secs(1); // 1 fps
    let mut last_capture = std::time::Instant::now() - min_interval;
    let mut frame_count: u64 = 0;

    loop {
        let frame = match pinned_stream.next().await {
            Some(f) => f,
            None => break,
        };

        frame_count += 1;

        // Rate limit
        if last_capture.elapsed() < min_interval {
            continue;
        }

        let width = frame.buffer.width();
        let height = frame.buffer.height();
        if width == 0 || height == 0 {
            continue;
        }

        // Convert I420 → RGBA on blocking thread
        let mtx_clone = media_tx.clone();

        let i420_data = frame.buffer.to_i420();
        let (data_y, data_u, data_v) = i420_data.data();
        let strides = i420_data.strides();

        // Copy the plane data for spawn_blocking
        let y_plane = data_y.to_vec();
        let u_plane = data_u.to_vec();
        let v_plane = data_v.to_vec();
        let stride_y = strides.0 as usize;
        let stride_u = strides.1 as usize;
        let stride_v = strides.2 as usize;

        tokio::task::spawn_blocking(move || {
            let w = width as usize;
            let h = height as usize;

            // I420 → RGBA
            let mut rgba = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let y_val = y_plane[y * stride_y + x] as f32;
                    let u_val = u_plane[(y / 2) * stride_u + (x / 2)] as f32 - 128.0;
                    let v_val = v_plane[(y / 2) * stride_v + (x / 2)] as f32 - 128.0;

                    let r = (y_val + 1.402 * v_val).clamp(0.0, 255.0) as u8;
                    let g = (y_val - 0.344 * u_val - 0.714 * v_val).clamp(0.0, 255.0) as u8;
                    let b = (y_val + 1.772 * u_val).clamp(0.0, 255.0) as u8;

                    let i = (y * w + x) * 4;
                    rgba[i] = r;
                    rgba[i + 1] = g;
                    rgba[i + 2] = b;
                    rgba[i + 3] = 255;
                }
            }

            // RGBA → RGB → JPEG (JPEG doesn't support alpha channel)
            let img: image::RgbaImage = image::ImageBuffer::from_raw(
                width, height, rgba,
            ).unwrap();
            let rgb_img = image::DynamicImage::ImageRgba8(img).to_rgb8();
            let mut jpeg_buf = std::io::Cursor::new(Vec::new());
            rgb_img.write_to(&mut jpeg_buf, image::ImageFormat::Jpeg).ok();
            let jpeg = jpeg_buf.into_inner();

            if jpeg.is_empty() {
                return;
            }

            // Binary plane: [w][h] + jpeg on the bound channel — no JSON,
            // no identity strings, no core-side double parse.
            let mut payload = Vec::new();
            continuum_bridge_protocol::encode_video_payload_into(
                &mut payload,
                width as u16,
                height as u16,
                &jpeg,
            );
            let _ = mtx_clone.send((
                continuum_bridge_protocol::MediaKind::VideoJpeg,
                channel,
                payload,
            ));
        });

        last_capture = std::time::Instant::now();

        if frame_count == 1 {
            info!("👁 First video frame from '{}': {}x{}", speaker_name, width, height);
        }
    }

    info!("👁 Video stream ended for '{}'", speaker_name);
}

// =============================================================================
// Helpers
// =============================================================================

fn generate_token(
    identity: &str,
    name: &str,
    room: &str,
    can_publish: bool,
    metadata: &ParticipantMetadata,
) -> Result<String, String> {
    let api_key = std::env::var("LIVEKIT_API_KEY").unwrap_or_else(|_| DEV_API_KEY.to_string());
    let api_secret = std::env::var("LIVEKIT_API_SECRET").unwrap_or_else(|_| DEV_API_SECRET.to_string());
    AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity(identity)
        .with_name(name)
        .with_metadata(&metadata.to_json())
        .with_grants(VideoGrants {
            room_join: true,
            room: room.to_string(),
            can_publish,
            can_subscribe: true,
            can_publish_data: true,
            ..Default::default()
        })
        .to_jwt()
        .map_err(|e| format!("Failed to generate token: {}", e))
}

/// Convert RGBA to I420 into a LiveKit I420Buffer (matches frame_publisher.rs).
fn rgba_to_i420_into(rgba: &[u8], buffer: &mut I420Buffer, width: u32, height: u32) {
    let w = width as usize;
    let h = height as usize;
    let (data_y, data_u, data_v) = buffer.data_mut();

    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let r = rgba[i] as i32;
            let g = rgba[i + 1] as i32;
            let b = rgba[i + 2] as i32;

            data_y[y * w + x] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;

            if y % 2 == 0 && x % 2 == 0 {
                let uv_idx = (y / 2) * (w / 2) + (x / 2);
                data_u[uv_idx] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
                data_v[uv_idx] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            }
        }
    }
}
