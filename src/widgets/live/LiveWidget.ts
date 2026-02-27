/**
 * LiveWidget - Real-time collaboration orchestrator (audio, video, screen share)
 *
 * Like Slack huddles / Discord voice channels / Zoom calls.
 * Can be attached to any activity/room as a modality layer.
 *
 * Architecture:
 * - Orchestrator: owns session lifecycle, AudioStreamClient, state persistence
 * - Delegates rendering to sub-components:
 *   - <live-participant-tile> for each participant (owns video container in shadow DOM)
 *   - <live-controls> for media buttons (fires events back up)
 *   - <live-captions> for transcription overlay (manages its own fade timers)
 */

import { ReactiveWidget, html, reactive, unsafeCSS, type TemplateResult, type CSSResultGroup } from '../shared/ReactiveWidget';
import { repeat } from 'lit/directives/repeat.js';
import { ref, createRef } from 'lit/directives/ref.js';
import { styles as LIVE_STYLES } from './public/live-widget.styles';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { COMMANDS } from '../../shared/generated-command-constants';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { LiveJoinParams, LiveJoinResult } from '../../commands/collaboration/live/join/shared/LiveJoinTypes';
import type { LiveLeaveParams, LiveLeaveResult } from '../../commands/collaboration/live/leave/shared/LiveLeaveTypes';
import type { UserStateEntity } from '../../system/data/entities/UserStateEntity';
import type { CallEntity } from '../../system/data/entities/CallEntity';
import { AudioStreamClient, type TranscriptionResult } from './AudioStreamClient';
import { ContentService } from '../../system/state/ContentService';

import { DataUpdate } from '../../commands/data/update/shared/DataUpdateTypes';
import { DataList } from '../../commands/data/list/shared/DataListTypes';
import { CollaborationLiveTranscription } from '../../commands/collaboration/live/transcription/shared/CollaborationLiveTranscriptionTypes';

// Import sub-components (registers custom elements)
import './LiveParticipantTile';
import './LiveControls';
import './LiveCaptions';
import type { LiveControls } from './LiveControls';
import type { LiveCaptions } from './LiveCaptions';

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
  @reactive() private isPreview: boolean = false;
  @reactive() private previewStream: MediaStream | null = null;

  // Local user state
  @reactive() private micEnabled: boolean = true;
  @reactive() private speakerEnabled: boolean = true;
  @reactive() private speakerVolume: number = 1.0;
  @reactive() private cameraEnabled: boolean = false;
  @reactive() private screenShareEnabled: boolean = false;
  @reactive() private micPermissionGranted: boolean = false;
  @reactive() private captionsEnabled: boolean = true;

  // Entity association
  @reactive() private entityId: string = '';

  // Spotlight mode
  @reactive() private spotlightUserId: string | null = null;
  /** True when spotlight was set by user click (manual pin). Auto-spotlight won't override. */
  private _spotlightPinned: boolean = false;

  // Set of participant userIds with active video
  @reactive() private activeVideoUsers: Set<string> = new Set();

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

  // Typed refs to child components (Lit ref directive — no querySelector)
  private _captionsRef = createRef<LiveCaptions>();
  private _controlsRef = createRef<LiveControls>();

  // Speaking state timeouts per user
  private speakingTimeouts: Map<UUID, ReturnType<typeof setTimeout>> = new Map();

  // Saved state for visibility changes
  private _visibilitySavedMic: boolean | null = null;
  private _visibilitySavedSpeaker: boolean | null = null;

  // Keyboard listener for Escape key
  private _escapeHandler = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && this.spotlightUserId) {
      this.spotlightUserId = null;
      this._spotlightPinned = false;
    }
  };

  // Saved state for tab deactivation
  private _deactivateSavedMic: boolean | null = null;
  private _deactivateSavedSpeaker: boolean | null = null;

  // Tile resolution tracking — collected from tile-resized events, batched to data channel
  private _tileResolutions: Map<string, { width: number; height: number }> = new Map();
  private _tileResDebounce: ReturnType<typeof setTimeout> | null = null;

  // Reentrancy guard
  private _applyingMicState = false;

  // State loading tracking
  private stateLoadedPromise: Promise<void> | null = null;

  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(LIVE_STYLES)
  ] as CSSResultGroup;

  // ========================================
  // Lifecycle
  // ========================================

  override connectedCallback(): void {
    super.connectedCallback();

    document.addEventListener('keydown', this._escapeHandler);

    this.stateLoadedPromise = this.loadUserContext().then(() => {
      this.loadCallState();
      console.log(`LiveWidget: State loaded (mic=${this.micEnabled}, speaker=${this.speakerEnabled})`);
      this.requestUpdate();
    }).catch(err => {
      console.error('LiveWidget: Failed to load user context:', err);
    });

    this.visibilityObserver = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (this.isJoined) {
          if (!entry.isIntersecting && this._visibilitySavedMic === null) {
            this._visibilitySavedMic = this.micEnabled;
            this._visibilitySavedSpeaker = this.speakerEnabled;
            this.micEnabled = false;
            this.speakerEnabled = false;
          } else if (entry.isIntersecting && this._visibilitySavedMic !== null) {
            this.micEnabled = this._visibilitySavedMic;
            this.speakerEnabled = this._visibilitySavedSpeaker ?? true;
            this._visibilitySavedMic = null;
            this._visibilitySavedSpeaker = null;
          }
        }
      }
    }, { threshold: 0.1 });

    this.visibilityObserver.observe(this);
  }

  protected override updated(changedProperties: Map<string, unknown>): void {
    super.updated(changedProperties);

    // Auto-sync mic state
    if (changedProperties.has('micEnabled') && this.audioClient && !this._applyingMicState) {
      this._applyingMicState = true;
      this.applyMicState().finally(() => { this._applyingMicState = false; });
    }

    // Auto-sync speaker state
    if ((changedProperties.has('speakerEnabled') || changedProperties.has('speakerVolume')) && this.audioClient) {
      this.applySpeakerState();
    }
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    document.removeEventListener('keydown', this._escapeHandler);
    this.cleanup();
  }

  // ========================================
  // Activation / Deactivation (MainWidget interface)
  // ========================================

  onActivate(entityId?: string, metadata?: unknown): void {
    if (entityId) {
      const cleanEntityId = entityId.startsWith('live-') ? entityId.slice(5) : entityId;
      this.entityId = cleanEntityId;

      const meta = metadata as { room?: { id?: string; displayName?: string }; session?: { id: string } } | undefined;
      if (meta?.room?.id) {
        this.entityId = meta.room.id;
      }

      if (!this.isJoined) {
        console.log('LiveWidget: Auto-joining for entityId:', this.entityId);
        this.handleJoin();
      }
    }

    if (this.isJoined && this._deactivateSavedMic !== null) {
      this.micEnabled = this._deactivateSavedMic;
      this.speakerEnabled = this._deactivateSavedSpeaker ?? true;
      this._deactivateSavedMic = null;
      this._deactivateSavedSpeaker = null;
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
    }
  }

  setEntityId(entityId: string): void {
    this.entityId = entityId;
  }

  // ========================================
  // State Persistence
  // ========================================

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

  private async saveCallState(): Promise<void> {
    if (!this.userState?.id) {
      console.warn('LiveWidget: Cannot save call state - userState not loaded');
      return;
    }

    const newCallState = {
      micEnabled: this.micEnabled,
      speakerEnabled: this.speakerEnabled,
      speakerVolume: this.speakerVolume,
      cameraEnabled: this.cameraEnabled,
      screenShareEnabled: this.screenShareEnabled,
      currentCallId: this.sessionId || undefined
    };

    try {
      const result = await DataUpdate.execute<UserStateEntity>({
        collection: 'user_states',
        id: this.userState.id as UUID,
        data: { callState: newCallState },
        dbHandle: 'default'
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

  // ========================================
  // Session Lifecycle
  // ========================================

  private async handleJoin(): Promise<void> {
    if (!this.entityId) {
      console.error('LiveWidget: No entityId specified');
      return;
    }

    if (this.stateLoadedPromise) {
      await this.stateLoadedPromise;
    }

    if (!this.currentUser?.id) {
      console.warn('LiveWidget: No user identity — retrying state load...');
      try { await this.loadUserContext(); } catch (_) { /* ignore */ }
    }
    if (!this.currentUser?.id) {
      console.error('LiveWidget: Cannot join — no user identity after retry');
      return;
    }

    if (this.micEnabled && !this.micPermissionGranted) {
      try {
        this.previewStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
        this.micPermissionGranted = true;
        console.log('LiveWidget: Mic permission granted');
      } catch (error) {
        console.warn('LiveWidget: Mic permission denied:', error);
        this.micEnabled = false;
      }
    }

    this.isPreview = false;

    try {
      const userId = this.currentUser?.id;
      console.log('LiveWidget: Joining with entityId:', this.entityId, 'userId:', userId);
      const result = await Commands.execute<LiveJoinParams, LiveJoinResult>(COMMANDS.COLLABORATION_LIVE_JOIN, {
        entityId: this.entityId,
      });

      if (result.success && result.callId) {
        this.sessionId = result.callId;
        this.isJoined = true;

        // Start with just the local user. Remote participants are added
        // incrementally via LiveKit's ParticipantConnected events as agents
        // connect (staggered 2s apart). This prevents the grid from showing
        // a large initial set then fluctuating as connections stabilize.
        const myInfo = result.myParticipant;
        this.participants = [{
          userId: myInfo?.userId || ('self' as UUID),
          displayName: myInfo?.displayName || 'You',
          micEnabled: true,
          cameraEnabled: false,
          screenShareEnabled: false,
          isSpeaking: false
        }];
        console.log('LiveWidget: Starting with self, agents will appear as they connect');
        this.requestUpdate();

        this.subscribeToEvents();

        this.audioClient = new AudioStreamClient({
          onMicLevel: (level) => {
            // Route mic level to the controls component
            const controls = this._controlsRef.value ?? null;
            controls?.setMicLevel(level);
          },
          onParticipantJoined: (userId, displayName) => {
            console.log(`LiveWidget: ${displayName} joined the call (total before: ${this.participants.length})`);
            if (!this.participants.find(p => p.userId === userId)) {
              this.participants = [...this.participants, {
                userId: userId as UUID,
                displayName,
                micEnabled: true,
                cameraEnabled: false,
                screenShareEnabled: false,
                isSpeaking: false,
              }];
              console.log(`LiveWidget: Added ${displayName}, total now: ${this.participants.length}`);
            } else {
              console.log(`LiveWidget: ${displayName} already in list, skipping`);
            }
          },
          onParticipantLeft: (userId) => {
            // DON'T remove from participant grid on LiveKit disconnect.
            // Agents connect with 2s stagger and may have temporary connection
            // issues. Removing them causes the grid to fluctuate wildly
            // (15→10→12→8→15). The server's initial list is the ground truth
            // for who should be in the call. Just clean up media resources.
            console.log(`LiveWidget: ${userId} disconnected (keeping in grid)`);
            this._remoteVideoElements.delete(userId);
            const newSet = new Set(this.activeVideoUsers);
            newSet.delete(userId);
            this.activeVideoUsers = newSet;
          },
          onConnectionChange: (connected) => {
            console.log(`LiveWidget: Audio stream ${connected ? 'connected' : 'disconnected'}`);
            if (connected) {
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
            if (!this.sessionId) return;

            const resolvedName = this.participants.find(p => p.userId === transcription.userId)?.displayName
              || transcription.displayName;

            // Show caption immediately
            const captions = this._captionsRef.value ?? null;
            captions?.setCaption(resolvedName, transcription.text);

            this.setSpeaking(transcription.userId as UUID, true);

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
          },
        });

        try {
          const myUserId = result.myParticipant?.userId || 'unknown';
          const myDisplayName = result.myParticipant?.displayName || 'Unknown User';

          await this.audioClient.join(result.callId, myUserId, myDisplayName, result.livekitUrl, result.livekitToken);
          console.log('LiveWidget: Connected to audio stream');

          // Merge participants already in the LiveKit room into our grid.
          // ParticipantConnected events handle incremental arrivals, but for
          // participants already present when we connect, we enumerate them here.
          const existing = this.audioClient.getConnectedParticipants();
          if (existing.length > 0) {
            const currentIds = new Set(this.participants.map(p => p.userId));
            const newParticipants = [...this.participants];
            for (const ep of existing) {
              if (!currentIds.has(ep.userId as UUID)) {
                newParticipants.push({
                  userId: ep.userId as UUID,
                  displayName: ep.displayName,
                  micEnabled: true,
                  cameraEnabled: false,
                  screenShareEnabled: false,
                  isSpeaking: false,
                });
              }
            }
            this.participants = newParticipants;
            console.log(`LiveWidget: Added ${existing.length} existing participants (total: ${this.participants.length})`);
          }

          await this.applyMicState();
          this.applySpeakerState();
          console.log(`LiveWidget: State applied from saved (mic=${this.micEnabled}, speaker=${this.speakerEnabled}, volume=${this.speakerVolume})`);
        } catch (audioError) {
          console.warn('LiveWidget: Audio stream failed:', audioError);
        }
      }
    } catch (error) {
      console.error('LiveWidget: Failed to join:', error);
    }
  }

  private async handleLeave(): Promise<void> {
    if (!this.sessionId) return;

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

    window.history.back();
  }

  // ========================================
  // Event Subscriptions
  // ========================================

  private subscribeToEvents(): void {
    if (!this.sessionId) return;

    // NOTE: Participant join/leave is handled ONLY by LiveKit SDK events
    // (onParticipantJoined/onParticipantLeft callbacks in AudioStreamClient).
    // Server-side `live:joined`/`live:left` events are NOT subscribed here
    // because they race with LiveKit SDK events and caused duplicate/phantom
    // participants (the grid would fluctuate: 16→12→4→8).
    // LiveKit SDK is the ground truth for who is actually connected.

    this.unsubscribers.push(
      Events.subscribe(`live:speaking:${this.sessionId}`, (data: any) => {
        this.participants = this.participants.map(p => ({
          ...p,
          isSpeaking: data.speakingUserIds?.includes(p.userId) || false
        }));
      })
    );

    this.unsubscribers.push(
      Events.subscribe('voice:ai:speech', (data: {
        sessionId: string;
        speakerId: string;
        speakerName: string;
        text: string;
        audioDurationMs?: number;
        timestamp: number;
      }) => {
        if (data.sessionId === this.sessionId) {
          const durationMs = data.audioDurationMs || 5000;
          console.log(`LiveWidget: AI speech duration update: ${data.speakerName} (${durationMs}ms)`);

          this.setSpeakingWithDuration(data.speakerId as UUID, durationMs);

          const captions = this._captionsRef.value ?? null;
          if (captions) {
            if (captions.hasCaption(data.speakerName)) {
              captions.extendCaption(data.speakerName, durationMs + 500);
            } else {
              captions.setCaption(data.speakerName, data.text, durationMs + 500);
            }
          }
        }
      })
    );
  }

  // ========================================
  // Audio State Management
  // ========================================

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
    this.audioClient.setMuted(!this.micEnabled);
  }

  private async toggleMic(): Promise<void> {
    this.micEnabled = !this.micEnabled;
    await this.applyMicState();
    await this.saveCallState();
  }

  private applySpeakerState(): void {
    if (!this.audioClient) return;
    this.audioClient.setSpeakerMuted(!this.speakerEnabled);
    this.audioClient.setSpeakerVolume(this.speakerVolume);
  }

  private async toggleSpeaker(): Promise<void> {
    this.speakerEnabled = !this.speakerEnabled;
    this.requestUpdate();
    this.applySpeakerState();
    await this.saveCallState();
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
        const myUserId = this.currentUser?.id || '';
        this._remoteVideoElements.set(myUserId, videoElement);
        this.activeVideoUsers = new Set([...this.activeVideoUsers, myUserId]);
        console.log('LiveWidget: Local camera preview enabled');
      } else if (!this.cameraEnabled) {
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
        stream.getVideoTracks()[0].onended = () => {
          this.screenShareEnabled = false;
        };
      } catch (error) {
        console.error('LiveWidget: Failed to share screen:', error);
        this.screenShareEnabled = false;
      }
    }
  }

  private toggleCaptions(): void {
    this.captionsEnabled = !this.captionsEnabled;
    if (!this.captionsEnabled) {
      const captions = this._captionsRef.value ?? null;
      captions?.clearAll();
    }
  }

  // ========================================
  // Speaking State
  // ========================================

  private setSpeaking(userId: UUID, isSpeaking: boolean): void {
    const existingTimeout = this.speakingTimeouts.get(userId);
    if (existingTimeout) {
      clearTimeout(existingTimeout);
      this.speakingTimeouts.delete(userId);
    }

    this.participants = this.participants.map(p => ({
      ...p,
      isSpeaking: p.userId === userId ? isSpeaking : p.isSpeaking
    }));

    // Auto-spotlight: when someone starts speaking, spotlight them.
    // When they stop, return to grid. Manual pin overrides auto-spotlight.
    if (!this._spotlightPinned) {
      if (isSpeaking && userId !== this.currentUser?.id) {
        this.spotlightUserId = userId;
      } else if (!isSpeaking && this.spotlightUserId === userId) {
        // Only return to grid if no one else is speaking
        const anyoneSpeaking = this.participants.some(p =>
          p.userId !== userId && p.isSpeaking
        );
        if (!anyoneSpeaking) {
          this.spotlightUserId = null;
        }
      }
    }

    if (isSpeaking) {
      const timeout = setTimeout(() => {
        this.setSpeaking(userId, false);
      }, 2000);
      this.speakingTimeouts.set(userId, timeout);
    }
  }

  private setSpeakingWithDuration(userId: UUID, durationMs: number): void {
    const existingTimeout = this.speakingTimeouts.get(userId);
    if (existingTimeout) {
      clearTimeout(existingTimeout);
      this.speakingTimeouts.delete(userId);
    }

    this.participants = this.participants.map(p => ({
      ...p,
      isSpeaking: p.userId === userId ? true : p.isSpeaking
    }));

    // Auto-spotlight while speaking (unless user manually pinned someone)
    if (!this._spotlightPinned && userId !== this.currentUser?.id) {
      this.spotlightUserId = userId;
    }

    this.requestUpdate();

    const timeout = setTimeout(() => {
      this.participants = this.participants.map(p => ({
        ...p,
        isSpeaking: p.userId === userId ? false : p.isSpeaking
      }));
      this.speakingTimeouts.delete(userId);

      // Return to grid when done speaking (if not pinned and no one else is speaking)
      if (!this._spotlightPinned && this.spotlightUserId === userId) {
        const anyoneSpeaking = this.participants.some(p => p.isSpeaking);
        if (!anyoneSpeaking) {
          this.spotlightUserId = null;
        }
      }

      this.requestUpdate();
    }, durationMs + 500);
    this.speakingTimeouts.set(userId, timeout);
  }

  /**
   * Get the video element for a participant (if any).
   * Used in templates to pass video element as a Lit property.
   */
  private _videoFor(userId: string): HTMLVideoElement | null {
    return this._remoteVideoElements.get(userId) ?? null;
  }

  // ========================================
  // Participant Profile
  // ========================================

  private openParticipantProfile(participant: Participant): void {
    const entityId = participant.userId;
    const title = participant.displayName || 'User Profile';

    if (this.currentUser?.id) {
      ContentService.setUserId(this.currentUser.id as UUID);
    }

    ContentService.open('profile', entityId, {
      title,
      metadata: { user: { id: entityId, displayName: title } }
    });
  }

  // ========================================
  // Event Handlers from Sub-Components
  // ========================================

  private _onParticipantClick(e: CustomEvent): void {
    const userId = e.detail.userId;
    if (this.spotlightUserId === userId) {
      this.spotlightUserId = null;
      this._spotlightPinned = false;
    } else {
      this.spotlightUserId = userId;
      this._spotlightPinned = true; // Manual click pins the spotlight
    }
  }

  private _onExitSpotlight(): void {
    this.spotlightUserId = null;
    this._spotlightPinned = false;
  }

  private _onTileResized(e: CustomEvent): void {
    const { userId, width, height } = e.detail;
    this._tileResolutions.set(userId, { width, height });

    // Debounce 1s — batch all tile resize events into a single data channel publish
    if (this._tileResDebounce) clearTimeout(this._tileResDebounce);
    this._tileResDebounce = setTimeout(() => {
      if (this.audioClient && this._tileResolutions.size > 0) {
        this.audioClient.sendTileResolutions(this._tileResolutions);
      }
    }, 1000);
  }

  // ========================================
  // Grid Helpers
  // ========================================

  private _gridDataCount(): string {
    const count = this.participants.length;
    if (count <= 25) return count.toString();
    return 'many';
  }

  // ========================================
  // Cleanup
  // ========================================

  private cleanup(): void {
    if (this.audioClient) {
      this.audioClient.leave();
      this.audioClient = null;
    }

    this.speakingTimeouts.forEach(timeout => clearTimeout(timeout));
    this.speakingTimeouts.clear();

    this._remoteVideoElements.clear();
    this.activeVideoUsers = new Set();
    this._tileResolutions.clear();
    if (this._tileResDebounce) {
      clearTimeout(this._tileResDebounce);
      this._tileResDebounce = null;
    }

    this.unsubscribers.forEach(unsub => unsub());
    this.unsubscribers = [];

    if (this.visibilityObserver) {
      this.visibilityObserver.disconnect();
      this.visibilityObserver = null;
    }

    if (this.previewStream) {
      this.previewStream.getTracks().forEach(track => track.stop());
      this.previewStream = null;
    }

    if (this.localStream) {
      this.localStream.getTracks().forEach(track => track.stop());
      this.localStream = null;
    }

    if (this.audioContext) {
      this.audioContext.close();
      this.audioContext = null;
    }
  }

  // ========================================
  // Rendering (Orchestrator)
  // ========================================

  protected override render(): TemplateResult {
    if (this.isJoined) {
      const presenter = this.spotlightUserId
        ? this.participants.find(p => p.userId === this.spotlightUserId)
        : this.participants.find(p => p.screenShareEnabled);

      if (presenter) {
        return this._renderSpotlightView(presenter);
      }

      return this._renderGridView();
    }

    // Not joined
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

  private _renderGridView(): TemplateResult {
    return html`
      <div class="live-container"
        @toggle-mic=${() => this.toggleMic()}
        @toggle-speaker=${() => this.toggleSpeaker()}
        @toggle-camera=${() => this.toggleCamera()}
        @toggle-screenshare=${() => this.toggleScreenShare()}
        @toggle-captions=${() => this.toggleCaptions()}
        @leave=${() => this.handleLeave()}
        @participant-click=${(e: CustomEvent) => this._onParticipantClick(e)}
        @exit-spotlight=${() => this._onExitSpotlight()}
        @tile-resized=${(e: CustomEvent) => this._onTileResized(e)}
      >
        <div class="content-area">
          <div class="participant-grid" data-count="${this._gridDataCount()}">
            ${this.participants.length === 0
              ? html`<div class="empty-state">Waiting for others to join...</div>`
              : repeat(this.participants, p => p.userId, (p, i) => html`
                  <live-participant-tile
                    data-user-id="${p.userId}"
                    data-color="${(i % 7) + 1}"
                    .userId=${p.userId}
                    .displayName=${p.displayName}
                    .isSpeaking=${p.isSpeaking}
                    .videoElement=${this._videoFor(p.userId)}
                    .isMuted=${!p.micEnabled}
                  ></live-participant-tile>
                `)
            }
          </div>
          <live-captions ${ref(this._captionsRef)} .visible=${this.captionsEnabled}></live-captions>
        </div>
        <live-controls ${ref(this._controlsRef)}
          .micEnabled=${this.micEnabled}
          .speakerEnabled=${this.speakerEnabled}
          .cameraEnabled=${this.cameraEnabled}
          .screenShareEnabled=${this.screenShareEnabled}
          .captionsEnabled=${this.captionsEnabled}
        ></live-controls>
      </div>
    `;
  }

  private _renderSpotlightView(presenter: Participant): TemplateResult {
    const otherParticipants = this.participants.filter(p => p.userId !== presenter.userId);

    return html`
      <div class="live-container spotlight-mode"
        @toggle-mic=${() => this.toggleMic()}
        @toggle-speaker=${() => this.toggleSpeaker()}
        @toggle-camera=${() => this.toggleCamera()}
        @toggle-screenshare=${() => this.toggleScreenShare()}
        @toggle-captions=${() => this.toggleCaptions()}
        @leave=${() => this.handleLeave()}
        @participant-click=${(e: CustomEvent) => this._onParticipantClick(e)}
        @exit-spotlight=${() => this._onExitSpotlight()}
        @tile-resized=${(e: CustomEvent) => this._onTileResized(e)}
      >
        <!-- Main presenter area with caption overlay -->
        <div class="content-area">
          <div class="spotlight-main" @click=${() => { this.spotlightUserId = null; }}>
            <live-participant-tile
              class="spotlight-presenter"
              data-user-id="${presenter.userId}"
              .userId=${presenter.userId}
              .displayName=${presenter.displayName}
              .isSpeaking=${presenter.isSpeaking}
              .videoElement=${this._videoFor(presenter.userId)}
              .isPresenter=${true}
              .isSpotlighted=${!!this.spotlightUserId}
              .isScreenSharing=${presenter.screenShareEnabled}
              .isMuted=${!presenter.micEnabled}
              @click=${(e: Event) => e.stopPropagation()}
            ></live-participant-tile>
          </div>
          <live-captions ${ref(this._captionsRef)} .visible=${this.captionsEnabled}></live-captions>
        </div>

        <!-- Other participants in strip -->
        ${otherParticipants.length > 0 ? html`
          <div class="participant-strip">
            ${repeat(otherParticipants, p => p.userId, (p, i) => html`
              <div class="strip-tile">
                <live-participant-tile
                  data-user-id="${p.userId}"
                  data-color="${(i % 7) + 1}"
                  .userId=${p.userId}
                  .displayName=${p.displayName}
                  .isSpeaking=${p.isSpeaking}
                  .videoElement=${this._videoFor(p.userId)}
                  .isMuted=${!p.micEnabled}
                ></live-participant-tile>
              </div>
            `)}
          </div>
        ` : ''}

        <live-controls ${ref(this._controlsRef)}
          .micEnabled=${this.micEnabled}
          .speakerEnabled=${this.speakerEnabled}
          .cameraEnabled=${this.cameraEnabled}
          .screenShareEnabled=${this.screenShareEnabled}
          .captionsEnabled=${this.captionsEnabled}
        ></live-controls>
      </div>
    `;
  }

  private async loadExistingParticipants(): Promise<void> {
    if (!this.entityId) return;

    try {
      const result = await DataList.execute<CallEntity>({
        collection: 'calls',
        filter: { roomId: this.entityId, status: 'active' },
        limit: 1,
        dbHandle: 'default'
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
}

// Register the custom element
if (typeof customElements !== 'undefined' && !customElements.get('live-widget')) {
  customElements.define('live-widget', LiveWidget);
}
