/**
 * AudioStreamClient - LiveKit WebRTC client for real-time audio/video streaming
 *
 * Connects to a LiveKit SFU server for:
 * - Joining/leaving calls (WebRTC rooms)
 * - Publishing microphone audio (automatic Opus encoding)
 * - Subscribing to remote audio/video tracks
 * - Receiving live transcriptions
 *
 * Participant classification uses JWT metadata (ParticipantRole enum),
 * NOT identity string prefixes. Role is set at token generation time
 * and read via participant.metadata on the SDK.
 */

import {
  Room,
  RoomEvent,
  Track,
  ConnectionState,
  DisconnectReason,
  type RemoteTrack,
  type RemoteTrackPublication,
  type RemoteParticipant,
  type Participant,
  type TrackPublication,
  type TranscriptionSegment,
} from 'livekit-client';

import {
  type ParticipantMetadata,
  ParticipantRole,
  parseParticipantMetadata,
  isVisibleParticipant,
} from '../../shared/LiveKitTypes';

/** Transcription result from STT pipeline */
export interface TranscriptionResult {
  userId: string;
  displayName: string;
  text: string;
  confidence: number;
  language: string;
}

/** Avatar state update (received via LiveKit data messages) */
export interface AvatarUpdateEvent {
  persona_id: string;
  speaking: boolean;
  listening: boolean;
  emotion: string;
  viseme: number;
  viseme_weight: number;
  head_rotation: [number, number, number];
  gaze_target: [number, number];
}

interface AudioStreamClientOptions {
  /** Callback when participant joins */
  onParticipantJoined?: (userId: string, displayName: string) => void;
  /** Callback when participant leaves */
  onParticipantLeft?: (userId: string) => void;
  /** Callback for connection status changes */
  onConnectionChange?: (connected: boolean) => void;
  /** Callback for microphone audio level (0.0 to 1.0, called ~30x/sec) */
  onMicLevel?: (level: number) => void;
  /** Callback when speech is transcribed (LiveKit native transcription) */
  onTranscription?: (result: TranscriptionResult) => void;
  /** Callback when a remote video track is subscribed (returns attached <video> element) */
  onVideoTrackAdded?: (participantId: string, element: HTMLVideoElement) => void;
  /** Callback when a remote video track is unsubscribed */
  onVideoTrackRemoved?: (participantId: string) => void;
  /** Callback when an avatar state update arrives (via data channel) */
  onAvatarUpdate?: (update: AvatarUpdateEvent) => void;
}

export class AudioStreamClient {
  private room: Room | null = null;
  private options: AudioStreamClientOptions;

  // Mic level monitoring (~30fps polling of LiveKit's audioLevel)
  private micLevelInterval: ReturnType<typeof setInterval> | null = null;

  // Speaker (output) state — applied to all remote audio elements
  private speakerMuted = false;
  private speakerVolume = 1.0;
  private remoteAudioElements: Map<string, HTMLAudioElement> = new Map();

  // Hidden audio container (remote audio tracks auto-play here)
  private audioContainer: HTMLDivElement | null = null;

  constructor(options: AudioStreamClientOptions = {}) {
    this.options = options;
  }

  /**
   * Connect to a LiveKit room and join the call.
   *
   * @param callId - Call/room identifier (used for logging)
   * @param userId - Local user's identity (matches token identity)
   * @param displayName - Local user's display name
   * @param livekitUrl - LiveKit server WebSocket URL (ws://host:port)
   * @param token - LiveKit JWT access token
   */
  async join(
    callId: string,
    userId: string,
    displayName: string,
    livekitUrl: string,
    token: string,
  ): Promise<void> {
    this.room = new Room({
      adaptiveStream: true,
      dynacast: true,
    });

    this.setupEventHandlers();

    // Hidden container for remote audio playback elements
    this.audioContainer = document.createElement('div');
    this.audioContainer.style.display = 'none';
    this.audioContainer.setAttribute('data-livekit-audio', callId);
    document.body.appendChild(this.audioContainer);

    await this.room.connect(livekitUrl, token);
    console.log(`AudioStreamClient: Connected to LiveKit room (call=${callId}, identity=${userId})`);
  }

  /**
   * Leave the current call and release all resources
   */
  leave(): void {
    this.cleanup();
  }

  /**
   * Start publishing microphone audio.
   * LiveKit handles getUserMedia, Opus encoding, and WebRTC transport.
   */
  async startMicrophone(): Promise<void> {
    if (!this.room) throw new Error('Not connected to LiveKit room');

    await this.room.localParticipant.setMicrophoneEnabled(true);
    this.startMicLevelMonitoring();

    console.log('AudioStreamClient: Microphone enabled (LiveKit handles encoding/transport)');
  }

  /**
   * Stop publishing microphone audio
   */
  stopMicrophone(): void {
    if (!this.room) return;

    this.room.localParticipant.setMicrophoneEnabled(false);
    this.stopMicLevelMonitoring();

    console.log('AudioStreamClient: Microphone disabled');
  }

  /**
   * Set mic mute status (published/unpublished to other participants)
   */
  setMuted(muted: boolean): void {
    if (!this.room) return;
    this.room.localParticipant.setMicrophoneEnabled(!muted);
    if (muted) {
      this.stopMicLevelMonitoring();
    } else {
      this.startMicLevelMonitoring();
    }
  }

  /**
   * Enable/disable camera via LiveKit.
   * Returns the local <video> element for preview when enabling.
   */
  async setCameraEnabled(enabled: boolean): Promise<HTMLVideoElement | null> {
    if (!this.room) return null;

    await this.room.localParticipant.setCameraEnabled(enabled);

    if (enabled) {
      // Get the local video track and create a preview element
      const publication = this.room.localParticipant.getTrackPublication(Track.Source.Camera);
      if (publication?.track) {
        const element = publication.track.attach() as HTMLVideoElement;
        element.style.width = '100%';
        element.style.height = '100%';
        element.style.objectFit = 'cover';
        element.style.transform = 'scaleX(-1)'; // Mirror for self-view
        console.log('AudioStreamClient: Camera enabled with local preview');
        return element;
      }
    }
    console.log(`AudioStreamClient: Camera ${enabled ? 'enabled' : 'disabled'}`);
    return null;
  }

  /**
   * Set speaker muted (your output - what you hear)
   */
  setSpeakerMuted(muted: boolean): void {
    this.speakerMuted = muted;
    this.applyVolumeToRemoteAudio();
  }

  /**
   * Set speaker volume (0.0 to 1.0)
   */
  setSpeakerVolume(volume: number): void {
    this.speakerVolume = Math.max(0, Math.min(1, volume));
    this.applyVolumeToRemoteAudio();
  }

  /**
   * Check if connected to LiveKit room
   */
  get isConnected(): boolean {
    return this.room?.state === ConnectionState.Connected;
  }

  // --- Event handlers ---

  /**
   * Resolve a participant's role from their JWT metadata.
   * Every participant should have metadata set at token generation time.
   */
  private getParticipantRole(participant: RemoteParticipant): ParticipantMetadata | null {
    return parseParticipantMetadata(participant.metadata);
  }

  private setupEventHandlers(): void {
    if (!this.room) return;

    this.room.on(RoomEvent.Connected, () => {
      console.log('AudioStreamClient: Room connected');
      this.options.onConnectionChange?.(true);
    });

    this.room.on(RoomEvent.Disconnected, (reason?: DisconnectReason) => {
      console.log('AudioStreamClient: Room disconnected, reason:', reason);
      this.options.onConnectionChange?.(false);
    });

    // Remote participant joined — only notify UI for visible participants
    this.room.on(RoomEvent.ParticipantConnected, (participant: RemoteParticipant) => {
      const meta = this.getParticipantRole(participant);
      if (!isVisibleParticipant(meta)) return;
      console.log(`AudioStreamClient: ${participant.name || participant.identity} joined (role=${meta?.role ?? 'unknown'})`);
      this.options.onParticipantJoined?.(participant.identity, participant.name || participant.identity);
    });

    // Remote participant left — only notify UI for visible participants
    this.room.on(RoomEvent.ParticipantDisconnected, (participant: RemoteParticipant) => {
      const meta = this.getParticipantRole(participant);
      if (!isVisibleParticipant(meta)) return;
      console.log(`AudioStreamClient: ${participant.identity} left`);
      this.options.onParticipantLeft?.(participant.identity);
    });

    // Remote track subscribed — audio or video from another participant
    this.room.on(RoomEvent.TrackSubscribed, (
      track: RemoteTrack,
      publication: RemoteTrackPublication,
      participant: RemoteParticipant,
    ) => {
      const meta = this.getParticipantRole(participant);

      // Skip tracks from invisible system participants (STT listeners, ambient audio)
      if (!isVisibleParticipant(meta)) return;

      // Identity IS the userId — no prefix stripping needed.
      // AI agents use their persona userId directly; role is in metadata.
      const userId = participant.identity;

      if (track.kind === Track.Kind.Audio) {
        // Attach audio element for playback (hidden container, auto-plays)
        const element = track.attach() as HTMLAudioElement;
        element.volume = this.speakerMuted ? 0 : this.speakerVolume;
        this.audioContainer?.appendChild(element);
        this.remoteAudioElements.set(userId, element);
        console.log(`AudioStreamClient: Audio track subscribed from ${userId} (role=${meta?.role ?? 'unknown'})`);
      }

      if (track.kind === Track.Kind.Video) {
        // Video: create attached <video> element and notify UI
        const element = track.attach() as HTMLVideoElement;
        element.style.width = '100%';
        element.style.height = '100%';
        element.style.objectFit = 'cover';
        this.options.onVideoTrackAdded?.(userId, element);
        console.log(`AudioStreamClient: Video track subscribed from ${userId}`);
      }
    });

    // Remote track unsubscribed
    this.room.on(RoomEvent.TrackUnsubscribed, (
      track: RemoteTrack,
      publication: RemoteTrackPublication,
      participant: RemoteParticipant,
    ) => {
      // Detach and remove all DOM elements for this track
      track.detach().forEach(el => el.remove());

      const userId = participant.identity;

      if (track.kind === Track.Kind.Audio) {
        this.remoteAudioElements.delete(userId);
      }

      if (track.kind === Track.Kind.Video) {
        this.options.onVideoTrackRemoved?.(userId);
      }

      console.log(`AudioStreamClient: Track unsubscribed from ${userId} (${track.kind})`);
    });

    // Transcriptions (LiveKit native transcription API)
    this.room.on(RoomEvent.TranscriptionReceived, (
      segments: TranscriptionSegment[],
      participant?: Participant,
      publication?: TrackPublication,
    ) => {
      if (!participant) return;

      for (const segment of segments) {
        if (!segment.final) continue; // Only report final transcriptions

        console.log(`AudioStreamClient: Transcription from ${participant.name}: "${segment.text.slice(0, 50)}..."`);
        this.options.onTranscription?.({
          userId: participant.identity,
          displayName: participant.name || participant.identity,
          text: segment.text,
          confidence: 1.0, // LiveKit doesn't provide confidence scores
          language: segment.language || 'en',
        });
      }
    });

    // Data messages (human STT transcriptions, avatar state updates, etc.)
    // Human STT uses data channel (STT listener → room).
    // AI speech uses native transcription API (arrives via TranscriptionReceived above).
    this.room.on(RoomEvent.DataReceived, (
      payload: Uint8Array,
      participant?: RemoteParticipant,
      kind?: unknown,
      topic?: string,
    ) => {
      try {
        const text = new TextDecoder().decode(payload);
        const data = JSON.parse(text);

        if (topic === 'transcription') {
          // Human STT transcription from server-side STT listener
          console.log(`AudioStreamClient: STT transcription: ${data.speaker_name}: "${data.text?.slice(0, 50)}..."`);
          this.options.onTranscription?.({
            userId: data.speaker_id || '',
            displayName: data.speaker_name || data.speaker_id || 'Unknown',
            text: data.text || '',
            confidence: 1.0,
            language: data.language || 'en',
          });
        } else if (topic === 'avatar_state') {
          this.options.onAvatarUpdate?.(data as AvatarUpdateEvent);
        }
      } catch (e) {
        console.error('AudioStreamClient: Failed to parse data message:', e);
      }
    });
  }

  // --- Mic level monitoring ---

  /**
   * Poll local participant's audioLevel at ~30fps for visual feedback.
   * LiveKit computes this from the published audio track automatically.
   */
  private startMicLevelMonitoring(): void {
    this.stopMicLevelMonitoring();

    this.micLevelInterval = setInterval(() => {
      if (this.room?.localParticipant) {
        const level = this.room.localParticipant.audioLevel;
        this.options.onMicLevel?.(level);
      }
    }, 33); // ~30fps
  }

  private stopMicLevelMonitoring(): void {
    if (this.micLevelInterval) {
      clearInterval(this.micLevelInterval);
      this.micLevelInterval = null;
    }
  }

  // --- Speaker volume ---

  /**
   * Apply current volume/mute to all remote audio elements
   */
  private applyVolumeToRemoteAudio(): void {
    const effectiveVolume = this.speakerMuted ? 0 : this.speakerVolume;
    for (const element of this.remoteAudioElements.values()) {
      element.volume = effectiveVolume;
    }
  }

  // --- Cleanup ---

  private cleanup(): void {
    this.stopMicLevelMonitoring();

    // Disconnect from LiveKit room (stops all tracks, closes connection)
    if (this.room) {
      this.room.disconnect();
      this.room = null;
    }

    // Remove remote audio elements from DOM
    for (const element of this.remoteAudioElements.values()) {
      element.remove();
    }
    this.remoteAudioElements.clear();

    // Remove hidden audio container
    if (this.audioContainer) {
      this.audioContainer.remove();
      this.audioContainer = null;
    }
  }
}
