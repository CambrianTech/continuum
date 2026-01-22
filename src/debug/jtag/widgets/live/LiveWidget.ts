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
import { AudioStreamClient } from './AudioStreamClient';

interface Participant {
  userId: UUID;
  displayName: string;
  avatar?: string;
  micEnabled: boolean;
  cameraEnabled: boolean;
  screenShareEnabled: boolean;
  isSpeaking: boolean;
  videoStream?: MediaStream;
}

export class LiveWidget extends ReactiveWidget {
  // Session state
  @reactive() private sessionId: string | null = null;
  @reactive() private participants: Participant[] = [];
  @reactive() private isJoined: boolean = false;
  @reactive() private isPreview: boolean = false;  // Preview mode before joining
  @reactive() private previewStream: MediaStream | null = null;

  // Local user state
  @reactive() private micEnabled: boolean = true;  // Default to on
  @reactive() private cameraEnabled: boolean = false;
  @reactive() private screenShareEnabled: boolean = false;
  @reactive() private micPermissionGranted: boolean = false;

  // Entity association (the room/activity this live session is attached to)
  @reactive() private entityId: string = '';

  // Media streams
  private localStream: MediaStream | null = null;
  private audioContext: AudioContext | null = null;

  // Audio streaming client (WebSocket to Rust call server)
  private audioClient: AudioStreamClient | null = null;

  // Event subscriptions
  private unsubscribers: Array<() => void> = [];

  // Styles imported from SCSS
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(LIVE_STYLES)
  ] as CSSResultGroup;

  override connectedCallback(): void {
    super.connectedCallback();
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

      if (meta?.session?.id) {
        this.sessionId = meta.session.id;
      }

      // Auto-join immediately without preview step
      if (!this.isJoined) {
        console.log('LiveWidget: Auto-joining for entityId:', this.entityId);
        this.handleJoin();
      }
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
    this.cleanup();
  }

  private cleanup(): void {
    // Unsubscribe from events
    this.unsubscribers.forEach(unsub => unsub());
    this.unsubscribers = [];

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
      const result = await Commands.execute<any, any>('data/list', {
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
        callerId: userId  // Pass current user's ID so server knows WHO is joining
      });

      if (result.success && result.sessionId) {
        this.sessionId = result.sessionId;
        this.isJoined = true;
        // Map CallParticipant to local Participant (add isSpeaking state)
        this.participants = (result.participants || []).map(p => ({
          userId: p.userId,
          displayName: p.displayName,
          avatar: p.avatar,
          micEnabled: p.micEnabled,
          cameraEnabled: p.cameraEnabled,
          screenShareEnabled: p.screenShareEnabled,
          isSpeaking: false  // Local UI state, not in CallParticipant
        }));
        this.requestUpdate();  // Force UI update

        // Subscribe to session events
        this.subscribeToEvents();

        // Connect to audio streaming server
        this.audioClient = new AudioStreamClient({
          // Port configured via STREAMING_CORE_WS_PORT env var, default 50053
          serverUrl: `ws://127.0.0.1:${(window as any).__STREAMING_CORE_WS_PORT || 50053}`,
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
          },
          onConnectionChange: (connected) => {
            console.log(`LiveWidget: Audio stream ${connected ? 'connected' : 'disconnected'}`);
          },
        });

        try {
          // Get user info for audio stream from myParticipant
          const myUserId = result.myParticipant?.userId || 'unknown';
          const myDisplayName = result.myParticipant?.displayName || 'Unknown User';

          // Join audio stream (sessionId is guaranteed non-null here)
          await this.audioClient.join(result.sessionId, myUserId, myDisplayName);
          console.log('LiveWidget: Connected to audio stream');

          // Start microphone streaming
          await this.audioClient.startMicrophone();
          this.micEnabled = true;
          console.log('LiveWidget: Mic streaming started');
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

    // Note: Audio streaming is handled directly via WebSocket (AudioStreamClient)
    // rather than through JTAG events for lower latency
  }

  private async toggleMic(): Promise<void> {
    this.micEnabled = !this.micEnabled;

    if (this.audioClient) {
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
      // Notify server of mute status
      this.audioClient.setMuted(!this.micEnabled);
    }
  }

  private async toggleCamera(): Promise<void> {
    this.cameraEnabled = !this.cameraEnabled;

    if (this.cameraEnabled) {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ video: true });
        if (this.localStream) {
          stream.getVideoTracks().forEach(track => this.localStream!.addTrack(track));
        } else {
          this.localStream = stream;
        }
        // TODO: Stream video to server
      } catch (error) {
        console.error('LiveWidget: Failed to get camera:', error);
        this.cameraEnabled = false;
      }
    } else {
      if (this.localStream) {
        this.localStream.getVideoTracks().forEach(track => track.stop());
      }
    }

    // Notify server
    if (this.sessionId) {
      await Commands.execute<any, any>('live/camera', {
        sessionId: this.sessionId,
        enabled: this.cameraEnabled
      });
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

    // Notify server
    if (this.sessionId) {
      await Commands.execute<any, any>('live/share', {
        sessionId: this.sessionId,
        enabled: this.screenShareEnabled
      });
    }
  }

  protected override render(): TemplateResult {
    // Joined: show full call UI
    if (this.isJoined) {
      return html`
        <div class="live-container">
          <div class="participant-grid">
            ${this.participants.length === 0
              ? html`<div class="empty-state">Waiting for others to join...</div>`
              : this.participants.map(p => this.renderParticipant(p))
            }
          </div>
          <div class="controls">
            <button
              class="control-btn ${this.micEnabled ? 'active' : 'inactive'}"
              @click=${this.toggleMic}
              title="${this.micEnabled ? 'Mute' : 'Unmute'}"
            >
              ${this.micEnabled ? '🎤' : '🔇'}
            </button>
            <button
              class="control-btn ${this.cameraEnabled ? 'active' : 'inactive'}"
              @click=${this.toggleCamera}
              title="${this.cameraEnabled ? 'Turn off camera' : 'Turn on camera'}"
            >
              ${this.cameraEnabled ? '📷' : '📷'}
            </button>
            <button
              class="control-btn ${this.screenShareEnabled ? 'active' : 'inactive'}"
              @click=${this.toggleScreenShare}
              title="${this.screenShareEnabled ? 'Stop sharing' : 'Share screen'}"
            >
              🖥️
            </button>
            <button
              class="control-btn leave"
              @click=${this.handleLeave}
              title="Leave"
            >
              📞
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
    return html`
      <div class="participant-tile ${participant.isSpeaking ? 'speaking' : ''}">
        ${participant.cameraEnabled && participant.videoStream
          ? html`<video class="participant-video" autoplay muted></video>`
          : html`
              <div class="participant-avatar">
                ${participant.displayName.charAt(0).toUpperCase()}
              </div>
            `
        }
        <div class="participant-name">${participant.displayName}</div>
        <div class="participant-indicators">
          ${!participant.micEnabled
            ? html`<div class="indicator muted">🔇</div>`
            : ''
          }
        </div>
      </div>
    `;
  }
}

// Register the custom element
if (typeof customElements !== 'undefined' && !customElements.get('live-widget')) {
  customElements.define('live-widget', LiveWidget);
}
