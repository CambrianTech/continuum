/**
 * LiveWidget - Real-time collaboration (audio, video, screen share)
 *
 * Like Slack huddles / Discord voice channels / Zoom calls.
 * Can be attached to any activity/room as a modality layer.
 *
 * Architecture:
 * - Browser captures mic/camera via getUserMedia
 * - Streams to server via events (handle-based)
 * - Server mixes audio (each participant gets everyone except self)
 * - Server relays video (SFU pattern)
 * - Widget renders participant grid + controls
 */

import { ReactiveWidget, html, reactive, unsafeCSS, type TemplateResult, type CSSResultGroup } from '../shared/ReactiveWidget';
import { styles as LIVE_STYLES } from './public/live-widget.styles';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { COMMANDS } from '../../shared/generated-command-constants';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { LiveJoinParams, LiveJoinResult } from '../../commands/collaboration/live/join/shared/LiveJoinTypes';
import type { LiveLeaveParams, LiveLeaveResult } from '../../commands/collaboration/live/leave/shared/LiveLeaveTypes';
import type { CollaborationLiveTranscriptionParams, CollaborationLiveTranscriptionResult } from '../../commands/collaboration/live/transcription/shared/CollaborationLiveTranscriptionTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../commands/data/update/shared/DataUpdateTypes';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { UserStateEntity } from '../../system/data/entities/UserStateEntity';
import type { CallEntity } from '../../system/data/entities/CallEntity';
import { AudioStreamClient, type TranscriptionResult } from './AudioStreamClient';
import { ContentService } from '../../system/state/ContentService';

import { DataUpdate } from '../../commands/data/update/shared/DataUpdateTypes';
import { DataList } from '../../commands/data/list/shared/DataListTypes';
import { CollaborationLiveTranscription } from '../../commands/collaboration/live/transcription/shared/CollaborationLiveTranscriptionTypes';
interface Participant {
  userId: UUID;
  displayName: string;
  avatar?: string;
  micEnabled: boolean;
  cameraEnabled: boolean;
  screenShareEnabled: boolean;
  isSpeaking: boolean;
}

export class LiveWidget extends ReactiveWidget {
  // Session state
  @reactive() private sessionId: string | null = null;
  @reactive() private participants: Participant[] = [];
  @reactive() private isJoined: boolean = false;
  @reactive() private isPreview: boolean = false;  // Preview mode before joining
  @reactive() private previewStream: MediaStream | null = null;

  // Local user state
  @reactive() private micEnabled: boolean = true;  // Default to on - YOUR microphone input
  @reactive() private speakerEnabled: boolean = true;  // Default to on - audio OUTPUT
  @reactive() private speakerVolume: number = 1.0;  // 0.0 to 1.0
  @reactive() private micLevel: number = 0;  // 0.0 to 1.0 - current mic input level for visual feedback
  @reactive() private cameraEnabled: boolean = false;
  @reactive() private screenShareEnabled: boolean = false;
  @reactive() private micPermissionGranted: boolean = false;
  @reactive() private captionsEnabled: boolean = true;  // Show live transcription captions
  // Support multiple simultaneous speakers - Map keyed by speakerId
  @reactive() private activeCaptions: Map<string, { speakerName: string; text: string; timestamp: number }> = new Map();

  // Entity association (the room/activity this live session is attached to)
  @reactive() private entityId: string = '';

  // Media streams
  private localStream: MediaStream | null = null;
  private audioContext: AudioContext | null = null;

  // Visibility observer for auto-mute
  private visibilityObserver: IntersectionObserver | null = null;

  // Audio streaming client (WebSocket to Rust call server)
  private audioClient: AudioStreamClient | null = null;

  // Event subscriptions
  private unsubscribers: Array<() => void> = [];

  // Remote video elements from LiveKit — keyed by participant identity (userId)
  private _remoteVideoElements: Map<string, HTMLVideoElement> = new Map();

  // Set of participant userIds with active video (triggers re-render when changed)
  @reactive() private activeVideoUsers: Set<string> = new Set();

  // Spotlight mode — focus on one participant (click to maximize)
  @reactive() private spotlightUserId: string | null = null;

  // Caption fade timeouts per speaker (supports multiple simultaneous speakers)
  private captionFadeTimeouts: Map<string, ReturnType<typeof setTimeout>> = new Map();

  // Speaking state timeouts per user (clear after 2s of no speech)
  private speakingTimeouts: Map<UUID, ReturnType<typeof setTimeout>> = new Map();

  // Saved state for visibility changes (IntersectionObserver auto-mute)
  private _visibilitySavedMic: boolean | null = null;
  private _visibilitySavedSpeaker: boolean | null = null;

  // Keyboard listener for Escape key (spotlight exit)
  private _escapeHandler = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && this.spotlightUserId) {
      this.spotlightUserId = null;
    }
  };

  // Saved state for tab deactivation (onDeactivate/onActivate)
  private _deactivateSavedMic: boolean | null = null;
  private _deactivateSavedSpeaker: boolean | null = null;

  // Reentrancy guard — prevents updated() from re-triggering applyMicState()
  // when applyMicState() itself sets micEnabled = false on error
  private _applyingMicState = false;

  // State loading tracking - ensures state is loaded before using it
  private stateLoadedPromise: Promise<void> | null = null;

  // Styles imported from SCSS
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(LIVE_STYLES)
  ] as CSSResultGroup;

  override connectedCallback(): void {
    super.connectedCallback();

    // Listen for Escape key to exit spotlight mode
    document.addEventListener('keydown', this._escapeHandler);

    // Wait for userState to load before trying to read call state
    // loadUserContext is already called by super.connectedCallback()
    // Store promise so handleJoin() can wait for it
    this.stateLoadedPromise = this.loadUserContext().then(() => {
      this.loadCallState();
      console.log(`LiveWidget: State loaded (mic=${this.micEnabled}, speaker=${this.speakerEnabled})`);
      this.requestUpdate(); // Force re-render with loaded state
    }).catch(err => {
      console.error('LiveWidget: Failed to load user context:', err);
    });

    // IntersectionObserver for auto-mute when widget becomes hidden
    // Uses _visibilitySaved* (separate from _deactivateSaved* to avoid conflicts)
    this.visibilityObserver = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (this.isJoined) {
          if (!entry.isIntersecting && this._visibilitySavedMic === null) {
            // Widget went out of view — save state and mute
            this._visibilitySavedMic = this.micEnabled;
            this._visibilitySavedSpeaker = this.speakerEnabled;
            this.micEnabled = false;
            this.speakerEnabled = false;
            // updated() hook handles applyMicState/applySpeakerState reactively
          } else if (entry.isIntersecting && this._visibilitySavedMic !== null) {
            // Widget came back into view — restore saved state
            this.micEnabled = this._visibilitySavedMic;
            this.speakerEnabled = this._visibilitySavedSpeaker ?? true;
            this._visibilitySavedMic = null;
            this._visibilitySavedSpeaker = null;
            // updated() hook handles applyMicState/applySpeakerState reactively
          }
        }
      }
    }, { threshold: 0.1 });

    this.visibilityObserver.observe(this);
  }


  /**
   * Reactive state sync — LitElement lifecycle hook.
   * Whenever micEnabled or speakerEnabled changes (from ANY source),
   * automatically propagate to the audio client.
   * This is the SINGLE mechanism for state → audio sync.
   */
  protected override updated(changedProperties: Map<string, unknown>): void {
    super.updated(changedProperties);

    // Attach LiveKit video elements to their container divs after DOM updates
    if (changedProperties.has('activeVideoUsers') || changedProperties.has('participants')) {
      this.attachVideoElements();
    }

    // Auto-sync mic state when micEnabled changes
    if (changedProperties.has('micEnabled') && this.audioClient && !this._applyingMicState) {
      this._applyingMicState = true;
      this.applyMicState().finally(() => { this._applyingMicState = false; });
    }

    // Auto-sync speaker state
    if ((changedProperties.has('speakerEnabled') || changedProperties.has('speakerVolume')) && this.audioClient) {
      this.applySpeakerState();
    }
  }

  /**
   * Load call state from UserStateEntity
   */
  private loadCallState(): void {
    const callState = this.userState?.callState;
    if (callState) {
      this.micEnabled = callState.micEnabled ?? true;
      this.speakerEnabled = callState.speakerEnabled ?? true;
      this.speakerVolume = callState.speakerVolume ?? 1.0;
      this.cameraEnabled = callState.cameraEnabled ?? false;
      this.screenShareEnabled = callState.screenShareEnabled ?? false;
    }
  }

  /**
   * Persist call state to UserStateEntity
   */
  private async saveCallState(): Promise<void> {
    if (!this.userState?.id) {
      console.warn('LiveWidget: Cannot save call state - userState not loaded');
      return;
    }

    // Update the entity
    const newCallState = {
      micEnabled: this.micEnabled,
      speakerEnabled: this.speakerEnabled,
      speakerVolume: this.speakerVolume,
      cameraEnabled: this.cameraEnabled,
      screenShareEnabled: this.screenShareEnabled,
      currentCallId: this.sessionId || undefined
    };

    // Persist via data/update
    try {
      const result = await DataUpdate.execute<UserStateEntity>({
        collection: 'user_states',
        id: this.userState.id as UUID,
        data: { callState: newCallState }
      });

      if (!result.success) {
        console.error('LiveWidget: Failed to save call state:', result.error);
      } else {
        console.log('LiveWidget: Call state saved:', newCallState);
      }
    } catch (error) {
      console.error('LiveWidget: Exception saving call state:', error);
    }
  }

  /**
   * Called by MainWidget when this widget is activated with an entityId.
   * For live content type, entityId is the room/activity this live session attaches to.
   * Auto-joins immediately without preview step.
   */
  onActivate(entityId?: string, metadata?: unknown): void {
    if (entityId) {
      // Strip any 'live-' prefix - the actual room ID is the source of truth
      const cleanEntityId = entityId.startsWith('live-') ? entityId.slice(5) : entityId;
      this.entityId = cleanEntityId;

      // Store metadata for later use
      const meta = metadata as { room?: { id?: string; displayName?: string }; session?: { id: string } } | undefined;

      // Prefer room.id from metadata if available (most reliable source)
      if (meta?.room?.id) {
        this.entityId = meta.room.id;
      }

      // Don't use metadata.session.id - it's from tab state and may be stale
      // Session ID should only come from LiveJoinResult (server source of truth)
      // Widget will get fresh sessionId when handleJoin() calls LiveJoin command

      // Auto-join immediately without preview step
      if (!this.isJoined) {
        console.log('LiveWidget: Auto-joining for entityId:', this.entityId);
        this.handleJoin();
      }
    }

    // Restore mic/speaker when reactivated (from onDeactivate save)
    if (this.isJoined && this._deactivateSavedMic !== null) {
      this.micEnabled = this._deactivateSavedMic;
      this.speakerEnabled = this._deactivateSavedSpeaker ?? true;
      this._deactivateSavedMic = null;
      this._deactivateSavedSpeaker = null;
      // updated() hook handles applyMicState/applySpeakerState reactively
    }
  }

  onDeactivate(): void {
    console.log('LiveWidget: onDeactivate', { isJoined: this.isJoined, micEnabled: this.micEnabled });
    if (this.isJoined && this._deactivateSavedMic === null) {
      this._deactivateSavedMic = this.micEnabled;
      this._deactivateSavedSpeaker = this.speakerEnabled;
      this.micEnabled = false;
      this.speakerEnabled = false;
      console.log('LiveWidget: Muting mic/speaker on deactivate');
      // updated() hook handles applyMicState/applySpeakerState reactively
    }
  }

  /**
   * Alternative setter for entityId (MainWidget compatibility)
   */
  setEntityId(entityId: string): void {
    this.entityId = entityId;
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    document.removeEventListener('keydown', this._escapeHandler);
    this.cleanup();
  }

  private cleanup(): void {
    // Stop audio client
    if (this.audioClient) {
      this.audioClient.leave();
      this.audioClient = null;
    }

    // Clear caption timeouts
    this.captionFadeTimeouts.forEach(timeout => clearTimeout(timeout));
    this.captionFadeTimeouts.clear();
    this.activeCaptions.clear();

    // Clear speaking timeouts
    this.speakingTimeouts.forEach(timeout => clearTimeout(timeout));
    this.speakingTimeouts.clear();

    // Clear remote video elements
    this._remoteVideoElements.clear();
    this.activeVideoUsers = new Set();

    // Unsubscribe from events
    this.unsubscribers.forEach(unsub => unsub());
    this.unsubscribers = [];

    // Disconnect visibility observer
    if (this.visibilityObserver) {
      this.visibilityObserver.disconnect();
      this.visibilityObserver = null;
    }

    // Stop preview stream
    if (this.previewStream) {
      this.previewStream.getTracks().forEach(track => track.stop());
      this.previewStream = null;
    }

    // Stop local media
    if (this.localStream) {
      this.localStream.getTracks().forEach(track => track.stop());
      this.localStream = null;
    }

    // Close audio context
    if (this.audioContext) {
      this.audioContext.close();
      this.audioContext = null;
    }
  }

  /**
   * Start preview mode - show preview UI, DON'T request permissions yet
   */
  private async startPreview(): Promise<void> {
    this.isPreview = true;

    // Fetch current participants in this call (if any)
    await this.loadExistingParticipants();

    // Don't request mic permission here - wait until user clicks Join
    console.log('LiveWidget: Preview mode ready');
  }

  /**
   * Load existing participants already in this call
   */
  private async loadExistingParticipants(): Promise<void> {
    if (!this.entityId) return;

    try {
      // Query for active call in this room
      const result = await DataList.execute<CallEntity>({
        collection: 'calls',
        filter: { roomId: this.entityId, status: 'active' },
        limit: 1
      });

      if (result.success && result.items?.length > 0) {
        const call = result.items[0];
        this.participants = (call.participants || []).map((p: any) => ({
          userId: p.userId,
          displayName: p.displayName || 'Unknown',
          micEnabled: p.micEnabled ?? true,
          cameraEnabled: p.cameraEnabled ?? false,
          screenShareEnabled: p.screenShareEnabled ?? false,
          isSpeaking: false
        }));
        console.log('LiveWidget: Found', this.participants.length, 'existing participants');
      }
    } catch (error) {
      console.warn('LiveWidget: Failed to load existing participants:', error);
    }
  }

  /**
   * Toggle mic in preview mode
   */
  private togglePreviewMic(): void {
    this.micEnabled = !this.micEnabled;
    if (this.previewStream) {
      this.previewStream.getAudioTracks().forEach(track => {
        track.enabled = this.micEnabled;
      });
    }
  }

  /**
   * Cancel/close the preview - go back to previous content
   */
  private handleCancel(): void {
    console.log('LiveWidget: Cancelled');
    this.isPreview = false;
    this.cleanup();

    // Navigate back - close this tab
    window.history.back();
  }

  private async handleJoin(): Promise<void> {
    if (!this.entityId) {
      console.error('LiveWidget: No entityId specified');
      return;
    }

    // CRITICAL: Wait for saved state to load before using micEnabled/speakerEnabled
    // This prevents race conditions where we use default values instead of saved state
    if (this.stateLoadedPromise) {
      await this.stateLoadedPromise;
    }

    // Request mic permission NOW (when user clicks Join)
    if (this.micEnabled && !this.micPermissionGranted) {
      try {
        this.previewStream = await navigator.mediaDevices.getUserMedia({
          audio: true,
          video: false
        });
        this.micPermissionGranted = true;
        console.log('LiveWidget: Mic permission granted');
      } catch (error) {
        console.warn('LiveWidget: Mic permission denied:', error);
        this.micEnabled = false;
      }
    }

    // Exit preview mode
    this.isPreview = false;

    try {
      // Join the live session via command (creates/finds room, registers participant)
      const userId = this.currentUser?.id;
      console.log('LiveWidget: Joining with entityId:', this.entityId, 'userId:', userId);
      const result = await Commands.execute<LiveJoinParams, LiveJoinResult>(COMMANDS.COLLABORATION_LIVE_JOIN, {
        entityId: this.entityId,
        // userId is auto-injected by Commands.execute() from jtagClient
      });

      if (result.success && result.callId) {
        this.sessionId = result.callId;
        this.isJoined = true;

        // Use participants from server response (includes all room members for new calls)
        // WebSocket events (ParticipantJoined/Left) will update in real-time
        if (result.participants && result.participants.length > 0) {
          this.participants = result.participants.map((p: any) => ({
            userId: p.userId,
            displayName: p.displayName || 'Unknown',
            micEnabled: p.micEnabled ?? true,
            cameraEnabled: p.cameraEnabled ?? false,
            screenShareEnabled: p.screenShareEnabled ?? false,
            isSpeaking: false
          }));
          console.log('LiveWidget: Loaded', this.participants.length, 'participants from server');
        } else {
          // Fallback to just ourselves if server returned empty
          const myInfo = result.myParticipant;
          this.participants = [{
            userId: myInfo?.userId || ('self' as UUID),
            displayName: myInfo?.displayName || 'You',
            micEnabled: true,
            cameraEnabled: false,
            screenShareEnabled: false,
            isSpeaking: false
          }];
        }
        this.requestUpdate();  // Force UI update

        // Subscribe to session events
        this.subscribeToEvents();

        // Connect to LiveKit audio/video streaming
        this.audioClient = new AudioStreamClient({
          onMicLevel: (level) => {
            // Update mic level indicator directly (bypass reactive rendering for 30fps)
            const indicator = this.shadowRoot?.getElementById('mic-level') as HTMLElement;
            if (indicator) {
              indicator.style.height = `${Math.min(100, level * 300)}%`;
            }
          },
          onParticipantJoined: (userId, displayName) => {
            console.log(`LiveWidget: ${displayName} joined the call`);
            // Add to participants if not already present
            if (!this.participants.find(p => p.userId === userId)) {
              this.participants = [...this.participants, {
                userId: userId as UUID,
                displayName,
                micEnabled: true,
                cameraEnabled: false,
                screenShareEnabled: false,
                isSpeaking: false,
              }];
            }
          },
          onParticipantLeft: (userId) => {
            console.log(`LiveWidget: ${userId} left the call`);
            this.participants = this.participants.filter(p => p.userId !== userId);
            // Clean up video element if any
            this._remoteVideoElements.delete(userId);
            const newSet = new Set(this.activeVideoUsers);
            newSet.delete(userId);
            this.activeVideoUsers = newSet;
          },
          onConnectionChange: (connected) => {
            console.log(`LiveWidget: Audio stream ${connected ? 'connected' : 'disconnected'}`);
            if (connected) {
              // Re-apply mute state after reconnection
              this.audioClient?.setMuted(!this.micEnabled);
            }
          },
          onVideoTrackAdded: (participantId: string, element: HTMLVideoElement) => {
            console.log(`LiveWidget: Video track added for ${participantId}`);
            this._remoteVideoElements.set(participantId, element);
            this.activeVideoUsers = new Set([...this.activeVideoUsers, participantId]);
          },
          onVideoTrackRemoved: (participantId: string) => {
            console.log(`LiveWidget: Video track removed for ${participantId}`);
            this._remoteVideoElements.delete(participantId);
            const newSet = new Set(this.activeVideoUsers);
            newSet.delete(participantId);
            this.activeVideoUsers = newSet;
          },
          onTranscription: async (transcription: TranscriptionResult) => {
            if (!this.sessionId) {
              return;
            }

            try {
              await CollaborationLiveTranscription.execute({
                callSessionId: this.sessionId,
                speakerId: transcription.userId,
                speakerName: transcription.displayName,
                transcript: transcription.text,
                confidence: transcription.confidence,
                language: transcription.language,
                timestamp: Date.now()
              });
            } catch (error) {
              console.error(`Failed to relay transcription:`, error);
            }

            // Update caption display
            this.setCaption(transcription.displayName, transcription.text);

            // Mark user as speaking (auto-clears after 2s)
            this.setSpeaking(transcription.userId as UUID, true);
          },
        });

        try {
          // Get user info for audio stream from myParticipant
          const myUserId = result.myParticipant?.userId || 'unknown';
          const myDisplayName = result.myParticipant?.displayName || 'Unknown User';

          // Join LiveKit room (callId is guaranteed non-null here)
          await this.audioClient.join(result.callId, myUserId, myDisplayName, result.livekitUrl, result.livekitToken);
          console.log('LiveWidget: Connected to audio stream');

          // Apply saved state to audio client (ONE source of truth)
          await this.applyMicState();
          this.applySpeakerState();
          console.log(`LiveWidget: State applied from saved (mic=${this.micEnabled}, speaker=${this.speakerEnabled}, volume=${this.speakerVolume})`);
        } catch (audioError) {
          console.warn('LiveWidget: Audio stream failed:', audioError);
          // Still joined, just without audio
        }
      }
    } catch (error) {
      console.error('LiveWidget: Failed to join:', error);
    }
  }

  private async handleLeave(): Promise<void> {
    if (!this.sessionId) return;

    // Disconnect from audio stream first
    if (this.audioClient) {
      this.audioClient.leave();
      this.audioClient = null;
    }

    try {
      await Commands.execute<LiveLeaveParams, LiveLeaveResult>(COMMANDS.COLLABORATION_LIVE_LEAVE, {
        sessionId: this.sessionId
      });
    } catch (error) {
      console.error('LiveWidget: Failed to leave:', error);
    }

    this.isJoined = false;
    this.sessionId = null;
    this.participants = [];
    this.isPreview = false;
    this.cleanup();
    this.requestUpdate();

    // Navigate back after leaving
    window.history.back();
  }

  private subscribeToEvents(): void {
    if (!this.sessionId) return;

    // Participant joined
    this.unsubscribers.push(
      Events.subscribe(`live:joined:${this.sessionId}`, (data: any) => {
        this.participants = [...this.participants, data.participant];
      })
    );

    // Participant left
    this.unsubscribers.push(
      Events.subscribe(`live:left:${this.sessionId}`, (data: any) => {
        this.participants = this.participants.filter(p => p.userId !== data.userId);
      })
    );

    // Speaking indicator
    this.unsubscribers.push(
      Events.subscribe(`live:speaking:${this.sessionId}`, (data: any) => {
        this.participants = this.participants.map(p => ({
          ...p,
          isSpeaking: data.speakingUserIds?.includes(p.userId) || false
        }));
      })
    );

    // AI speech captions - when an AI speaks via TTS, show it in captions
    // This event is emitted by AIAudioBridge AFTER TTS synthesis, when audio is sent to server
    // audioDurationMs tells us how long the audio will play, so we can time the caption/highlight
    this.unsubscribers.push(
      Events.subscribe('voice:ai:speech', (data: {
        sessionId: string;
        speakerId: string;
        speakerName: string;
        text: string;
        audioDurationMs?: number;
        timestamp: number;
      }) => {
        // Only show captions for this session
        if (data.sessionId === this.sessionId) {
          const durationMs = data.audioDurationMs || 5000;  // Default 5s if not provided
          console.log(`LiveWidget: AI speech caption: ${data.speakerName}: "${data.text.slice(0, 50)}..." (${durationMs}ms)`);

          // Show caption and speaking indicator for the duration of the audio
          this.setCaptionWithDuration(data.speakerName, data.text, durationMs);
          this.setSpeakingWithDuration(data.speakerId as UUID, durationMs);
        }
      })
    );

    // Note: Audio streaming is handled directly via WebSocket (AudioStreamClient)
    // rather than through JTAG events for lower latency
  }

  /**
   * Apply mic state to audio client.
   * Called explicitly (toggleMic, handleJoin) and reactively via updated() hook.
   * The updated() hook ensures this runs whenever micEnabled changes from ANY source.
   */
  private async applyMicState(): Promise<void> {
    if (!this.audioClient) return;

    console.log(`LiveWidget: applyMicState(micEnabled=${this.micEnabled})`);

    if (this.micEnabled) {
      try {
        await this.audioClient.startMicrophone();
      } catch (error) {
        console.error('LiveWidget: Failed to start mic:', error);
        this.micEnabled = false;
      }
    } else {
      this.audioClient.stopMicrophone();
    }
    // Always sync mute status to server (defense in depth)
    this.audioClient.setMuted(!this.micEnabled);
  }

  private async toggleMic(): Promise<void> {
    this.micEnabled = !this.micEnabled;
    // updated() hook reactively calls applyMicState()
    // But we also await it explicitly to ensure mic is stopped before persisting state
    await this.applyMicState();
    await this.saveCallState();
  }

  /**
   * Apply speaker state to audio client (ONE source of truth)
   * Used by: initial load, toggleSpeaker, setSpeakerVolume
   */
  private applySpeakerState(): void {
    if (!this.audioClient) return;

    // Apply mute state
    this.audioClient.setSpeakerMuted(!this.speakerEnabled);

    // Apply volume
    this.audioClient.setSpeakerVolume(this.speakerVolume);
  }

  /**
   * Toggle speaker (audio output) - controls what YOU hear
   * Separate from mic which controls what OTHERS hear
   */
  private async toggleSpeaker(): Promise<void> {
    this.speakerEnabled = !this.speakerEnabled;
    this.requestUpdate();  // Force UI update

    this.applySpeakerState();

    // Persist to UserStateEntity
    await this.saveCallState();
  }

  /**
   * Set speaker volume (0.0 to 1.0)
   */
  private setSpeakerVolume(volume: number): void {
    this.speakerVolume = Math.max(0, Math.min(1, volume));
    this.applySpeakerState();
  }

  private async toggleCamera(): Promise<void> {
    this.cameraEnabled = !this.cameraEnabled;

    if (!this.audioClient) {
      console.error('LiveWidget: No audio client for camera toggle');
      this.cameraEnabled = false;
      return;
    }

    try {
      const videoElement = await this.audioClient.setCameraEnabled(this.cameraEnabled);

      if (this.cameraEnabled && videoElement) {
        // Store local video element for self-preview
        const myUserId = this.currentUser?.id || '';
        this._remoteVideoElements.set(myUserId, videoElement);
        this.activeVideoUsers = new Set([...this.activeVideoUsers, myUserId]);
        console.log('LiveWidget: Local camera preview enabled');
      } else if (!this.cameraEnabled) {
        // Remove self-preview
        const myUserId = this.currentUser?.id || '';
        this._remoteVideoElements.delete(myUserId);
        const newSet = new Set(this.activeVideoUsers);
        newSet.delete(myUserId);
        this.activeVideoUsers = newSet;
        console.log('LiveWidget: Local camera preview disabled');
      }
    } catch (error) {
      console.error('LiveWidget: Failed to toggle camera:', error);
      this.cameraEnabled = false;
    }
  }

  private async toggleScreenShare(): Promise<void> {
    this.screenShareEnabled = !this.screenShareEnabled;

    if (this.screenShareEnabled) {
      try {
        const stream = await navigator.mediaDevices.getDisplayMedia({ video: true });
        // TODO: Stream to server
        stream.getVideoTracks()[0].onended = () => {
          this.screenShareEnabled = false;
        };
      } catch (error) {
        console.error('LiveWidget: Failed to share screen:', error);
        this.screenShareEnabled = false;
      }
    }

    // TODO: Notify server about screen share state change (command doesn't exist yet)
    // When live/share command is implemented, uncomment:
    // if (this.sessionId) {
    //   await Commands.execute<LiveShareParams, LiveShareResult>('live/share', {
    //     sessionId: this.sessionId,
    //     enabled: this.screenShareEnabled
    //   });
    // }
  }

  /**
   * Toggle caption display on/off
   */
  private toggleCaptions(): void {
    this.captionsEnabled = !this.captionsEnabled;
    if (!this.captionsEnabled) {
      this.captionFadeTimeouts.forEach(timeout => clearTimeout(timeout));
      this.captionFadeTimeouts.clear();
      this.activeCaptions.clear();
    }
  }

  /**
   * Set a caption to display (auto-fades after 5 seconds)
   * Uses speakerName as key to support multiple simultaneous speakers
   */
  private setCaption(speakerName: string, text: string): void {
    // Clear existing timeout for this speaker
    const existingTimeout = this.captionFadeTimeouts.get(speakerName);
    if (existingTimeout) {
      clearTimeout(existingTimeout);
    }

    // Set/update caption for this speaker
    this.activeCaptions.set(speakerName, {
      speakerName,
      text,
      timestamp: Date.now()
    });

    // Force re-render
    this.requestUpdate();

    // Auto-fade after 5 seconds of no new transcription from this speaker
    const timeout = setTimeout(() => {
      this.activeCaptions.delete(speakerName);
      this.captionFadeTimeouts.delete(speakerName);
      this.requestUpdate();
    }, 5000);
    this.captionFadeTimeouts.set(speakerName, timeout);
  }

  /**
   * Mark a user as speaking (auto-clears after 2 seconds)
   */
  private setSpeaking(userId: UUID, isSpeaking: boolean): void {
    // Clear existing timeout for this user
    const existingTimeout = this.speakingTimeouts.get(userId);
    if (existingTimeout) {
      clearTimeout(existingTimeout);
      this.speakingTimeouts.delete(userId);
    }

    // Update participant state
    this.participants = this.participants.map(p => ({
      ...p,
      isSpeaking: p.userId === userId ? isSpeaking : p.isSpeaking
    }));

    // If setting to speaking, schedule auto-clear after 2s
    if (isSpeaking) {
      const timeout = setTimeout(() => {
        this.setSpeaking(userId, false);
      }, 2000);
      this.speakingTimeouts.set(userId, timeout);
    }
  }

  /**
   * Set caption with specific duration (for AI speech with known audio length)
   * Supports multiple simultaneous speakers
   */
  private setCaptionWithDuration(speakerName: string, text: string, durationMs: number): void {
    // Clear existing timeout for this speaker
    const existingTimeout = this.captionFadeTimeouts.get(speakerName);
    if (existingTimeout) {
      clearTimeout(existingTimeout);
    }

    // Set/update caption for this speaker
    this.activeCaptions.set(speakerName, {
      speakerName,
      text,
      timestamp: Date.now()
    });

    // Force re-render
    this.requestUpdate();

    // Clear caption after audio duration + small buffer
    const timeout = setTimeout(() => {
      this.activeCaptions.delete(speakerName);
      this.captionFadeTimeouts.delete(speakerName);
      this.requestUpdate();
    }, durationMs + 500);  // Add 500ms buffer
    this.captionFadeTimeouts.set(speakerName, timeout);
  }

  /**
   * Mark a user as speaking for a specific duration (for AI speech with known audio length)
   */
  private setSpeakingWithDuration(userId: UUID, durationMs: number): void {
    // Clear existing timeout for this user
    const existingTimeout = this.speakingTimeouts.get(userId);
    if (existingTimeout) {
      clearTimeout(existingTimeout);
      this.speakingTimeouts.delete(userId);
    }

    // Update participant state - set speaking
    this.participants = this.participants.map(p => ({
      ...p,
      isSpeaking: p.userId === userId ? true : p.isSpeaking
    }));
    this.requestUpdate();

    // Schedule auto-clear after audio duration + buffer
    const timeout = setTimeout(() => {
      this.participants = this.participants.map(p => ({
        ...p,
        isSpeaking: p.userId === userId ? false : p.isSpeaking
      }));
      this.speakingTimeouts.delete(userId);
      this.requestUpdate();
    }, durationMs + 500);  // Add 500ms buffer
    this.speakingTimeouts.set(userId, timeout);
  }

  /**
   * Attach LiveKit video elements to their container divs in the shadow DOM.
   * Called from updated() after DOM re-renders to ensure video elements are
   * placed in the correct participant tile containers.
   */
  private attachVideoElements(): void {
    for (const [userId, element] of this._remoteVideoElements) {
      // Find the video container in this participant's tile
      const container = this.shadowRoot?.querySelector(
        `.participant-tile[data-user-id="${userId}"] .video-container, .presenter-tile[data-user-id="${userId}"] .video-container`
      ) as HTMLElement | null;

      if (container && !container.contains(element)) {
        container.innerHTML = '';
        container.appendChild(element);
      }
    }
  }

  /**
   * Open user profile in a new tab
   */
  private openParticipantProfile(participant: Participant): void {
    const entityId = participant.userId;
    const title = participant.displayName || 'User Profile';

    console.log(`👤 LiveWidget: Opening profile for ${title} (entityId=${entityId})`);

    // Set current user ID for ContentService
    if (this.currentUser?.id) {
      ContentService.setUserId(this.currentUser.id as UUID);
    }

    // Open profile in new tab
    ContentService.open('profile', entityId, {
      title,
      metadata: {
        user: {
          id: entityId,
          displayName: title
        }
      }
    });
  }

  /**
   * Calculate optimal grid layout based on participant count
   * Returns string for CSS data-count attribute
   */
  private getGridDataCount(): string {
    const count = this.participants.length;
    if (count <= 25) return count.toString();
    return 'many';  // 26+ triggers scrollable grid
  }

  protected override render(): TemplateResult {
    // Joined: show full call UI
    if (this.isJoined) {
      // Check if someone is screen sharing OR spotlighted
      const presenter = this.spotlightUserId
        ? this.participants.find(p => p.userId === this.spotlightUserId)
        : this.participants.find(p => p.screenShareEnabled);

      if (presenter) {
        // Spotlight mode: presenter main, others in strip
        return this.renderSpotlightView(presenter);
      }

      // Grid mode: everyone equal
      return html`
        <div class="live-container">
          <div class="participant-grid" data-count="${this.getGridDataCount()}">
            ${this.participants.length === 0
              ? html`<div class="empty-state">Waiting for others to join...</div>`
              : this.participants.map(p => this.renderParticipant(p))
            }
          </div>
          <div class="controls">
            ${this.captionsEnabled && this.activeCaptions.size > 0 ? html`
              <div class="caption-display multi-speaker">
                ${Array.from(this.activeCaptions.values()).map(caption => html`
                  <div class="caption-line">
                    <span class="caption-speaker">${caption.speakerName}:</span>
                    <span class="caption-text">${caption.text}</span>
                  </div>
                `)}
              </div>
            ` : ''}
            <button
              id="mic-btn"
              class="control-btn ${this.micEnabled ? 'active' : 'inactive'}"
              @click=${this.toggleMic}
              title="${this.micEnabled ? 'Mute your mic' : 'Unmute your mic'}"
            >
              ${this.micEnabled ? this.renderMicOnIcon() : this.renderMicOffIcon()}
              ${this.micEnabled ? html`<span class="mic-level-indicator" id="mic-level"></span>` : ''}
            </button>
            <button
              class="control-btn ${this.speakerEnabled ? 'active' : 'inactive'}"
              @click=${this.toggleSpeaker}
              title="${this.speakerEnabled ? 'Mute audio' : 'Unmute audio'}"
            >
              ${this.speakerEnabled ? this.renderSpeakerOnIcon() : this.renderSpeakerOffIcon()}
            </button>
            <button
              class="control-btn ${this.cameraEnabled ? 'active' : 'inactive'}"
              @click=${this.toggleCamera}
              title="${this.cameraEnabled ? 'Turn off camera' : 'Turn on camera'}"
            >
              ${this.cameraEnabled ? this.renderCameraOnIcon() : this.renderCameraOffIcon()}
            </button>
            <button
              class="control-btn ${this.screenShareEnabled ? 'active' : 'inactive'}"
              @click=${this.toggleScreenShare}
              title="${this.screenShareEnabled ? 'Stop sharing' : 'Share screen'}"
            >
              ${this.renderScreenShareIcon()}
            </button>
            <button
              class="control-btn ${this.captionsEnabled ? 'active' : 'inactive'}"
              @click=${this.toggleCaptions}
              title="${this.captionsEnabled ? 'Hide captions' : 'Show captions'}"
            >
              ${this.renderCaptionsIcon()}
            </button>
            <button
              class="control-btn leave"
              @click=${this.handleLeave}
              title="Leave"
            >
              ${this.renderLeaveIcon()}
            </button>
          </div>
        </div>
      `;
    }

    // Not activated yet
    return html`
      <div class="live-container">
        <div class="join-prompt">
          <div class="empty-state">
            <p>Select a conversation to start a call</p>
          </div>
        </div>
      </div>
    `;
  }

  private renderParticipant(participant: Participant): TemplateResult {
    const hasVideo = this.activeVideoUsers.has(participant.userId);

    return html`
      <div
        class="participant-tile ${participant.isSpeaking ? 'speaking' : ''} ${hasVideo ? 'has-video' : ''}"
        data-user-id="${participant.userId}"
        @click=${() => this.handleParticipantClick(participant)}
        title="Click to view ${participant.displayName}'s profile"
      >
        ${hasVideo
          ? html`<div class="video-container"></div>`
          : html`
                <div class="participant-avatar">
                  ${participant.displayName.charAt(0).toUpperCase()}
                </div>
              `
        }
        <div class="participant-name">${participant.displayName}</div>
        <div class="participant-indicators">
          ${/* TODO: Sync mic status from Rust call_server to show accurate muted state */
            '' /* Disabled: micEnabled not synced, shows everyone as muted */
          }
        </div>
      </div>
    `;
  }

  /**
   * Handle click on participant tile — toggle spotlight (maximize/minimize).
   * Click once to maximize, click again to return to grid.
   */
  private handleParticipantClick(participant: Participant): void {
    if (this.spotlightUserId === participant.userId) {
      // Already spotlighted — exit back to grid
      this.spotlightUserId = null;
    } else {
      // Maximize this participant
      this.spotlightUserId = participant.userId;
    }
  }

  // ========================================
  // SVG Icon Renderers (Modern, clean icons)
  // ========================================

  private renderMicOnIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"></path>
        <path d="M19 10v2a7 7 0 0 1-14 0v-2"></path>
        <line x1="12" x2="12" y1="19" y2="22"></line>
      </svg>
    `;
  }

  private renderMicOffIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="2" x2="22" y1="2" y2="22"></line>
        <path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2"></path>
        <path d="M5 10v2a7 7 0 0 0 12 5"></path>
        <path d="M15 9.34V5a3 3 0 0 0-5.68-1.33"></path>
        <path d="M9 9v3a3 3 0 0 0 5.12 2.12"></path>
        <line x1="12" x2="12" y1="19" y2="22"></line>
      </svg>
    `;
  }

  private renderCameraOnIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="m22 8-6 4 6 4V8Z"></path>
        <rect width="14" height="12" x="2" y="6" rx="2" ry="2"></rect>
      </svg>
    `;
  }

  private renderCameraOffIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="2" x2="22" y1="2" y2="22"></line>
        <path d="M7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h12"></path>
        <path d="m22 8-6 4 6 4V8Z"></path>
        <path d="M10.3 7.7A4 4 0 0 1 14 6h2a2 2 0 0 1 2 2v3.34l1 .66"></path>
      </svg>
    `;
  }

  private renderSpeakerOnIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon>
        <path d="M15.54 8.46a5 5 0 0 1 0 7.07"></path>
        <path d="M19.07 4.93a10 10 0 0 1 0 14.14"></path>
      </svg>
    `;
  }

  private renderSpeakerOffIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon>
        <line x1="22" x2="16" y1="9" y2="15"></line>
        <line x1="16" x2="22" y1="9" y2="15"></line>
      </svg>
    `;
  }

  private renderScreenShareIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <rect width="20" height="14" x="2" y="3" rx="2"></rect>
        <line x1="8" x2="16" y1="21" y2="21"></line>
        <line x1="12" x2="12" y1="17" y2="21"></line>
        <path d="m9 10 3-3 3 3"></path>
        <path d="M12 7v7"></path>
      </svg>
    `;
  }

  private renderLeaveIcon(): TemplateResult {
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M10.68 13.31a16 16 0 0 0 3.41 2.6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7 2 2 0 0 1 1.72 2v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.42 19.42 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.63A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91"></path>
        <line x1="22" x2="16" y1="2" y2="8"></line>
        <line x1="16" x2="22" y1="2" y2="8"></line>
      </svg>
    `;
  }

  private renderCaptionsIcon(): TemplateResult {
    // CC (closed captions) icon
    return html`
      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <rect x="2" y="4" width="20" height="16" rx="2"></rect>
        <path d="M7 10h1.5"></path>
        <path d="M7 14h5"></path>
        <path d="M15 10h2"></path>
        <path d="M15 14h2"></path>
      </svg>
    `;
  }

  private renderMutedIndicator(): TemplateResult {
    return html`
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="2" x2="22" y1="2" y2="22"></line>
        <path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2"></path>
        <path d="M5 10v2a7 7 0 0 0 12 5"></path>
        <path d="M15 9.34V5a3 3 0 0 0-5.68-1.33"></path>
        <path d="M9 9v3a3 3 0 0 0 5.12 2.12"></path>
        <line x1="12" x2="12" y1="19" y2="22"></line>
      </svg>
    `;
  }

  /**
   * Render spotlight view - presenter in main area, others in strip at bottom.
   * Used when someone is screen sharing, presenting, or user-spotlighted.
   */
  private renderSpotlightView(presenter: Participant): TemplateResult {
    const otherParticipants = this.participants.filter(p => p.userId !== presenter.userId);
    const presenterHasVideo = this.activeVideoUsers.has(presenter.userId);

    return html`
      <div class="live-container spotlight-mode" @keydown=${this.handleSpotlightKeydown}>
        <!-- Main presenter area -->
        <div class="spotlight-main" @click=${() => { this.spotlightUserId = null; }}>
          <div
            class="presenter-tile ${presenterHasVideo ? 'has-video' : ''}"
            data-user-id="${presenter.userId}"
            @click=${(e: Event) => e.stopPropagation()}
          >
            ${presenter.screenShareEnabled
              ? html`<div class="screen-share-placeholder">${this.renderScreenShareIcon()} ${presenter.displayName} is sharing</div>`
              : presenterHasVideo
                ? html`<div class="video-container spotlight-video"></div>`
                : html`
                      <div class="participant-avatar large">
                        ${presenter.displayName.charAt(0).toUpperCase()}
                      </div>
                    `
            }
            <div class="participant-name">${presenter.displayName}</div>
            ${this.spotlightUserId ? html`
              <button class="exit-spotlight-btn" @click=${() => { this.spotlightUserId = null; }} title="Exit spotlight (Esc)">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="18" y1="6" x2="6" y2="18"></line>
                  <line x1="6" y1="6" x2="18" y2="18"></line>
                </svg>
              </button>
            ` : html`<div class="live-badge">LIVE</div>`}
          </div>
        </div>

        <!-- Other participants in horizontal strip -->
        ${otherParticipants.length > 0 ? html`
          <div class="participant-strip">
            ${otherParticipants.map(p => html`
              <div
                class="strip-tile ${p.isSpeaking ? 'speaking' : ''}"
                @click=${() => { this.spotlightUserId = p.userId; }}
                title="Spotlight ${p.displayName}"
              >
                <div class="strip-avatar">
                  ${p.displayName.charAt(0).toUpperCase()}
                </div>
                ${!p.micEnabled ? html`<div class="strip-muted">${this.renderMutedIndicator()}</div>` : ''}
              </div>
            `)}
          </div>
        ` : ''}

        <!-- Controls -->
        <div class="controls">
          ${this.captionsEnabled && this.activeCaptions.size > 0 ? html`
            <div class="caption-display multi-speaker">
              ${Array.from(this.activeCaptions.values()).map(caption => html`
                <div class="caption-line">
                  <span class="caption-speaker">${caption.speakerName}:</span>
                  <span class="caption-text">${caption.text}</span>
                </div>
              `)}
            </div>
          ` : ''}
          <button
            class="control-btn ${this.micEnabled ? 'active' : 'inactive'}"
            @click=${this.toggleMic}
            title="${this.micEnabled ? 'Mute' : 'Unmute'}"
            style="--mic-level: ${Math.min(1, this.micLevel * 3)}"
          >
            ${this.micEnabled ? this.renderMicOnIcon() : this.renderMicOffIcon()}
            ${this.micEnabled ? html`<span class="mic-level-indicator" style="height: ${Math.min(100, this.micLevel * 300)}%"></span>` : ''}
          </button>
          <button
            class="control-btn ${this.speakerEnabled ? 'active' : 'inactive'}"
            @click=${this.toggleSpeaker}
            title="${this.speakerEnabled ? 'Mute audio' : 'Unmute audio'}"
          >
            ${this.speakerEnabled ? this.renderSpeakerOnIcon() : this.renderSpeakerOffIcon()}
          </button>
          <button
            class="control-btn ${this.cameraEnabled ? 'active' : 'inactive'}"
            @click=${this.toggleCamera}
            title="${this.cameraEnabled ? 'Turn off camera' : 'Turn on camera'}"
          >
            ${this.cameraEnabled ? this.renderCameraOnIcon() : this.renderCameraOffIcon()}
          </button>
          <button
            class="control-btn ${this.screenShareEnabled ? 'active' : 'inactive'}"
            @click=${this.toggleScreenShare}
            title="${this.screenShareEnabled ? 'Stop sharing' : 'Share screen'}"
          >
            ${this.renderScreenShareIcon()}
          </button>
          <button
            class="control-btn ${this.captionsEnabled ? 'active' : 'inactive'}"
            @click=${this.toggleCaptions}
            title="${this.captionsEnabled ? 'Hide captions' : 'Show captions'}"
          >
            ${this.renderCaptionsIcon()}
          </button>
          <button
            class="control-btn leave"
            @click=${this.handleLeave}
            title="Leave"
          >
            ${this.renderLeaveIcon()}
          </button>
        </div>
      </div>
    `;
  }

  /**
   * Handle Escape key to exit spotlight mode
   */
  private handleSpotlightKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape' && this.spotlightUserId) {
      this.spotlightUserId = null;
    }
  }
}

// Register the custom element
if (typeof customElements !== 'undefined' && !customElements.get('live-widget')) {
  customElements.define('live-widget', LiveWidget);
}
