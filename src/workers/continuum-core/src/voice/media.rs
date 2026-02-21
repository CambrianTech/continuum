//! Media Abstraction Layer
//!
//! Defines track/session/room primitives that unify audio and video.
//! The call_server currently operates on raw audio — this module provides
//! the types to evolve toward a unified media pipeline where audio and
//! video are interchangeable track types within a session.
//!
//! Design principle: "Think VIDEO not audio" — the architecture should
//! handle both media types with the same structural primitives.
//!
//! Migration path (from current audio-only call_server):
//! 1. Wrap ParticipantStream fields into AudioTrack
//! 2. Wrap participant into MediaSession with one audio track
//! 3. Add VideoTrack variant (initially unused)
//! 4. When video arrives: add video track to MediaSession, compositor to MediaRoom

use crate::audio_constants::{AUDIO_FRAME_SIZE, AUDIO_SAMPLE_RATE};
use crate::voice::handle::Handle;
use std::collections::HashMap;

// ============================================================================
// Track Types
// ============================================================================

/// Track identifier — unique within a session
pub type TrackId = u32;

/// What kind of media this track carries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackKind {
    Audio,
    Video,
}

/// Track direction — controls whether the participant sends, receives, or both
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackDirection {
    /// Full duplex — participant both sends and receives this track
    SendRecv,
    /// Send only — participant produces media but doesn't consume
    SendOnly,
    /// Receive only — participant consumes media but doesn't produce
    RecvOnly,
}

/// Configuration for an audio track
#[derive(Debug, Clone)]
pub struct AudioTrackConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub frame_size: usize,
    pub frame_duration_ms: u64,
}

impl Default for AudioTrackConfig {
    fn default() -> Self {
        Self {
            sample_rate: AUDIO_SAMPLE_RATE,
            channels: 1,
            frame_size: AUDIO_FRAME_SIZE,
            frame_duration_ms: 20,
        }
    }
}

/// Video codec identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    /// VP8 (WebRTC standard, widely supported)
    VP8,
    /// VP9 (better compression, WebRTC optional)
    VP9,
    /// H.264/AVC (hardware-accelerated on most devices)
    H264,
    /// AV1 (best compression, emerging support)
    AV1,
    /// Raw frames (for local processing pipelines)
    Raw,
}

/// Configuration for a video track
#[derive(Debug, Clone)]
pub struct VideoTrackConfig {
    pub width: u32,
    pub height: u32,
    pub frame_rate: u32,
    pub codec: VideoCodec,
}

impl Default for VideoTrackConfig {
    fn default() -> Self {
        Self {
            width: 640,
            height: 480,
            frame_rate: 30,
            codec: VideoCodec::VP8,
        }
    }
}

/// Track configuration — either audio or video
#[derive(Debug, Clone)]
pub enum TrackConfig {
    Audio(AudioTrackConfig),
    Video(VideoTrackConfig),
}

impl TrackConfig {
    pub fn kind(&self) -> TrackKind {
        match self {
            Self::Audio(_) => TrackKind::Audio,
            Self::Video(_) => TrackKind::Video,
        }
    }
}

/// A single media track within a session.
///
/// Current state: Only Audio is implemented (wraps the existing
/// ParticipantStream audio pipeline). Video is defined but unused.
///
/// Each track has a direction (sendrecv/sendonly/recvonly) and
/// configuration specific to its media type.
#[derive(Debug)]
pub struct MediaTrack {
    pub id: TrackId,
    pub config: TrackConfig,
    pub direction: TrackDirection,
    pub muted: bool,
    pub label: String,
}

impl MediaTrack {
    /// Create a default audio track (mono, 16kHz, 20ms frames, full duplex)
    pub fn default_audio(id: TrackId) -> Self {
        Self {
            id,
            config: TrackConfig::Audio(AudioTrackConfig::default()),
            direction: TrackDirection::SendRecv,
            muted: false,
            label: "audio".into(),
        }
    }

    /// Create a default video track (640x480, 30fps, VP8, full duplex)
    pub fn default_video(id: TrackId) -> Self {
        Self {
            id,
            config: TrackConfig::Video(VideoTrackConfig::default()),
            direction: TrackDirection::SendRecv,
            muted: false,
            label: "video".into(),
        }
    }

    /// Create a receive-only audio track (for AI participants that only listen)
    pub fn listen_only_audio(id: TrackId) -> Self {
        Self {
            id,
            config: TrackConfig::Audio(AudioTrackConfig::default()),
            direction: TrackDirection::RecvOnly,
            muted: false,
            label: "audio-listen".into(),
        }
    }

    pub fn kind(&self) -> TrackKind {
        self.config.kind()
    }

    pub fn is_audio(&self) -> bool {
        self.kind() == TrackKind::Audio
    }

    pub fn is_video(&self) -> bool {
        self.kind() == TrackKind::Video
    }
}

// ============================================================================
// Session — a participant's collection of tracks
// ============================================================================

/// A participant's media session — all tracks for one user in one room.
///
/// Current audio-only call_server has `ParticipantStream` per participant.
/// MediaSession generalizes this: a participant has N tracks, each track
/// carries audio or video with its own buffer and configuration.
///
/// For the current audio pipeline:
///   MediaSession { tracks: [AudioTrack(id=0)] }
///
/// For a video huddle:
///   MediaSession { tracks: [AudioTrack(id=0), VideoTrack(id=1)] }
///
/// For screen sharing:
///   MediaSession { tracks: [AudioTrack(id=0), VideoTrack(id=1, label="camera"), VideoTrack(id=2, label="screen")] }
pub struct MediaSession {
    pub handle: Handle,
    pub user_id: String,
    pub display_name: String,
    pub is_ai: bool,
    tracks: HashMap<TrackId, MediaTrack>,
    next_track_id: TrackId,
}

impl MediaSession {
    /// Create a new session with no tracks
    pub fn new(handle: Handle, user_id: String, display_name: String, is_ai: bool) -> Self {
        Self {
            handle,
            user_id,
            display_name,
            is_ai,
            tracks: HashMap::new(),
            next_track_id: 0,
        }
    }

    /// Create a session with a default audio track (common case)
    pub fn with_audio(handle: Handle, user_id: String, display_name: String, is_ai: bool) -> Self {
        let mut session = Self::new(handle, user_id, display_name, is_ai);
        session.add_track(MediaTrack::default_audio(session.next_track_id));
        session
    }

    /// Create a session with audio + video tracks (huddle mode)
    pub fn with_audio_video(handle: Handle, user_id: String, display_name: String, is_ai: bool) -> Self {
        let mut session = Self::new(handle, user_id, display_name, is_ai);
        session.add_track(MediaTrack::default_audio(0));
        session.add_track(MediaTrack::default_video(1));
        session
    }

    /// Add a track to this session
    pub fn add_track(&mut self, track: MediaTrack) -> TrackId {
        let id = track.id;
        self.tracks.insert(id, track);
        if id >= self.next_track_id {
            self.next_track_id = id + 1;
        }
        id
    }

    /// Remove a track
    pub fn remove_track(&mut self, id: TrackId) -> Option<MediaTrack> {
        self.tracks.remove(&id)
    }

    /// Get a track by ID
    pub fn track(&self, id: TrackId) -> Option<&MediaTrack> {
        self.tracks.get(&id)
    }

    /// Get a mutable track by ID
    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut MediaTrack> {
        self.tracks.get_mut(&id)
    }

    /// Get all audio tracks
    pub fn audio_tracks(&self) -> impl Iterator<Item = &MediaTrack> {
        self.tracks.values().filter(|t| t.is_audio())
    }

    /// Get all video tracks
    pub fn video_tracks(&self) -> impl Iterator<Item = &MediaTrack> {
        self.tracks.values().filter(|t| t.is_video())
    }

    /// Number of tracks
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    /// Does this session have any video tracks?
    pub fn has_video(&self) -> bool {
        self.tracks.values().any(|t| t.is_video())
    }

    /// Mute all tracks of a given kind
    pub fn mute_kind(&mut self, kind: TrackKind, muted: bool) {
        for track in self.tracks.values_mut() {
            if track.kind() == kind {
                track.muted = muted;
            }
        }
    }
}

// ============================================================================
// Room — the call/huddle with all participants
// ============================================================================

/// A media room (call/huddle) — manages all participant sessions.
///
/// Current call_server has `Call` with `AudioMixer` and participants.
/// MediaRoom generalizes this: a room has N sessions, each with N tracks.
/// The room is responsible for mixing audio and compositing video.
///
/// Audio mixing: mix-minus (each participant hears everyone except themselves)
/// Video compositing: grid layout, speaker view, etc. (future)
pub struct MediaRoom {
    pub room_id: String,
    sessions: HashMap<Handle, MediaSession>,
}

impl MediaRoom {
    pub fn new(room_id: String) -> Self {
        Self {
            room_id,
            sessions: HashMap::new(),
        }
    }

    /// Add a participant session
    pub fn add_session(&mut self, session: MediaSession) {
        self.sessions.insert(session.handle, session);
    }

    /// Remove a participant session
    pub fn remove_session(&mut self, handle: &Handle) -> Option<MediaSession> {
        self.sessions.remove(handle)
    }

    /// Get a session
    pub fn session(&self, handle: &Handle) -> Option<&MediaSession> {
        self.sessions.get(handle)
    }

    /// Get a mutable session
    pub fn session_mut(&mut self, handle: &Handle) -> Option<&mut MediaSession> {
        self.sessions.get_mut(handle)
    }

    /// All sessions
    pub fn sessions(&self) -> impl Iterator<Item = &MediaSession> {
        self.sessions.values()
    }

    /// Number of participants
    pub fn participant_count(&self) -> usize {
        self.sessions.len()
    }

    /// Does any participant have video enabled?
    pub fn has_video(&self) -> bool {
        self.sessions.values().any(|s| s.has_video())
    }

    /// Get all user IDs in this room
    pub fn user_ids(&self) -> Vec<&str> {
        self.sessions.values().map(|s| s.user_id.as_str()).collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_handle() -> Handle {
        Handle::new()
    }

    #[test]
    fn test_audio_track_defaults() {
        let track = MediaTrack::default_audio(0);
        assert_eq!(track.kind(), TrackKind::Audio);
        assert!(!track.muted);
        assert_eq!(track.direction, TrackDirection::SendRecv);

        if let TrackConfig::Audio(config) = &track.config {
            assert_eq!(config.sample_rate, AUDIO_SAMPLE_RATE);
            assert_eq!(config.channels, 1);
        } else {
            panic!("Expected audio config");
        }
    }

    #[test]
    fn test_video_track_defaults() {
        let track = MediaTrack::default_video(0);
        assert_eq!(track.kind(), TrackKind::Video);
        assert!(track.is_video());
        assert!(!track.is_audio());

        if let TrackConfig::Video(config) = &track.config {
            assert_eq!(config.width, 640);
            assert_eq!(config.height, 480);
            assert_eq!(config.frame_rate, 30);
            assert_eq!(config.codec, VideoCodec::VP8);
        } else {
            panic!("Expected video config");
        }
    }

    #[test]
    fn test_session_audio_only() {
        let handle = test_handle();
        let session = MediaSession::with_audio(handle, "user1".into(), "Alice".into(), false);

        assert_eq!(session.track_count(), 1);
        assert!(!session.has_video());
        assert_eq!(session.audio_tracks().count(), 1);
        assert_eq!(session.video_tracks().count(), 0);
    }

    #[test]
    fn test_session_audio_video() {
        let handle = test_handle();
        let session = MediaSession::with_audio_video(handle, "user1".into(), "Bob".into(), false);

        assert_eq!(session.track_count(), 2);
        assert!(session.has_video());
        assert_eq!(session.audio_tracks().count(), 1);
        assert_eq!(session.video_tracks().count(), 1);
    }

    #[test]
    fn test_session_add_screen_share() {
        let handle = test_handle();
        let mut session = MediaSession::with_audio_video(handle, "user1".into(), "Carol".into(), false);

        // Add screen share as a second video track
        let mut screen = MediaTrack::default_video(2);
        screen.label = "screen".into();
        screen.direction = TrackDirection::SendOnly;
        session.add_track(screen);

        assert_eq!(session.track_count(), 3);
        assert_eq!(session.video_tracks().count(), 2);
    }

    #[test]
    fn test_session_mute_audio() {
        let handle = test_handle();
        let mut session = MediaSession::with_audio_video(handle, "user1".into(), "Dan".into(), false);

        session.mute_kind(TrackKind::Audio, true);

        // Audio muted, video not
        for track in session.audio_tracks() {
            assert!(track.muted);
        }
        for track in session.video_tracks() {
            assert!(!track.muted);
        }
    }

    #[test]
    fn test_room_basics() {
        let mut room = MediaRoom::new("room1".into());

        let h1 = test_handle();
        let h2 = test_handle();

        room.add_session(MediaSession::with_audio(h1, "u1".into(), "Alice".into(), false));
        room.add_session(MediaSession::with_audio_video(h2, "u2".into(), "Bob".into(), false));

        assert_eq!(room.participant_count(), 2);
        assert!(room.has_video()); // Bob has video

        let ids = room.user_ids();
        assert!(ids.contains(&"u1"));
        assert!(ids.contains(&"u2"));
    }

    #[test]
    fn test_room_remove_session() {
        let mut room = MediaRoom::new("room1".into());
        let h1 = test_handle();

        room.add_session(MediaSession::with_audio(h1, "u1".into(), "Alice".into(), false));
        assert_eq!(room.participant_count(), 1);

        let removed = room.remove_session(&h1);
        assert!(removed.is_some());
        assert_eq!(room.participant_count(), 0);
    }

    #[test]
    fn test_ai_session() {
        let handle = test_handle();
        let session = MediaSession::with_audio(handle, "ai1".into(), "Gemini".into(), true);

        assert!(session.is_ai);
        assert_eq!(session.track_count(), 1);
    }

    #[test]
    fn test_listen_only_track() {
        let track = MediaTrack::listen_only_audio(0);
        assert_eq!(track.direction, TrackDirection::RecvOnly);
        assert!(track.is_audio());
    }
}
