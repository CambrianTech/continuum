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

import { ReactiveWidget, html, css, reactive, type TemplateResult } from '../shared/ReactiveWidget';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { COMMANDS } from '../../shared/generated-command-constants';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

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

  // Local user state
  @reactive() private micEnabled: boolean = false;
  @reactive() private cameraEnabled: boolean = false;
  @reactive() private screenShareEnabled: boolean = false;

  // Room association
  @reactive() private roomId: string = '';

  // Media streams
  private localStream: MediaStream | null = null;
  private audioContext: AudioContext | null = null;

  // Event subscriptions
  private unsubscribers: Array<() => void> = [];

  static override styles = css`
    :host {
      display: block;
      height: 100%;
      background: var(--surface-primary, #1a1a2e);
    }

    .live-container {
      display: flex;
      flex-direction: column;
      height: 100%;
      padding: var(--spacing-md, 12px);
    }

    .participant-grid {
      flex: 1;
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
      gap: var(--spacing-md, 12px);
      padding: var(--spacing-md, 12px);
      overflow-y: auto;
    }

    .participant-tile {
      aspect-ratio: 4/3;
      background: var(--surface-secondary, #252540);
      border-radius: var(--radius-md, 8px);
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      position: relative;
      overflow: hidden;
    }

    .participant-tile.speaking {
      box-shadow: 0 0 0 3px var(--accent-color, #00c8ff);
    }

    .participant-avatar {
      width: 64px;
      height: 64px;
      border-radius: 50%;
      background: var(--surface-tertiary, #333);
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 1.5rem;
    }

    .participant-video {
      width: 100%;
      height: 100%;
      object-fit: cover;
    }

    .participant-name {
      position: absolute;
      bottom: 8px;
      left: 8px;
      background: rgba(0, 0, 0, 0.6);
      padding: 4px 8px;
      border-radius: 4px;
      font-size: 0.75rem;
      color: var(--text-primary, #fff);
    }

    .participant-indicators {
      position: absolute;
      top: 8px;
      right: 8px;
      display: flex;
      gap: 4px;
    }

    .indicator {
      width: 24px;
      height: 24px;
      border-radius: 50%;
      background: rgba(0, 0, 0, 0.6);
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 0.75rem;
    }

    .indicator.muted {
      color: var(--error-color, #ff4444);
    }

    .controls {
      display: flex;
      justify-content: center;
      gap: var(--spacing-md, 12px);
      padding: var(--spacing-md, 12px);
      background: var(--surface-secondary, #252540);
      border-radius: var(--radius-md, 8px);
    }

    .control-btn {
      width: 48px;
      height: 48px;
      border-radius: 50%;
      border: none;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 1.25rem;
      transition: all 0.15s ease;
    }

    .control-btn.active {
      background: var(--accent-color, #00c8ff);
      color: var(--background-color, #1a1a2e);
    }

    .control-btn.inactive {
      background: var(--surface-tertiary, #333);
      color: var(--text-secondary, #aaa);
    }

    .control-btn.leave {
      background: var(--error-color, #ff4444);
      color: white;
    }

    .control-btn:hover {
      transform: scale(1.1);
    }

    .join-prompt {
      flex: 1;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: var(--spacing-lg, 24px);
    }

    .join-btn {
      padding: 16px 32px;
      border-radius: var(--radius-md, 8px);
      border: none;
      background: var(--accent-color, #00c8ff);
      color: var(--background-color, #1a1a2e);
      font-size: 1rem;
      font-weight: bold;
      cursor: pointer;
      transition: all 0.15s ease;
    }

    .join-btn:hover {
      transform: scale(1.05);
    }

    .empty-state {
      color: var(--text-secondary, #aaa);
      text-align: center;
    }
  `;

  override connectedCallback(): void {
    super.connectedCallback();
    // Get roomId from attribute
    this.roomId = this.getAttribute('roomId') || this.getAttribute('room-id') || '';
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    this.cleanup();
  }

  private cleanup(): void {
    // Unsubscribe from events
    this.unsubscribers.forEach(unsub => unsub());
    this.unsubscribers = [];

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

  private async handleJoin(): Promise<void> {
    if (!this.roomId) {
      console.error('LiveWidget: No roomId specified');
      return;
    }

    try {
      // Join the live session
      const result = await Commands.execute<any, any>(COMMANDS.COLLABORATION_LIVE_JOIN, {
        roomId: this.roomId
      });

      if (result.success && result.sessionId) {
        this.sessionId = result.sessionId;
        this.isJoined = true;
        this.participants = result.participants || [];

        // Subscribe to session events
        this.subscribeToEvents();

        // Request mic access immediately on join
        try {
          this.localStream = await navigator.mediaDevices.getUserMedia({ audio: true });
          this.micEnabled = true;
          console.log('LiveWidget: Mic access granted');
        } catch (micError) {
          console.warn('LiveWidget: Mic access denied or unavailable:', micError);
          // Still joined, just without mic
        }
      }
    } catch (error) {
      console.error('LiveWidget: Failed to join:', error);
    }
  }

  private async handleLeave(): Promise<void> {
    if (!this.sessionId) return;

    try {
      await Commands.execute<any, any>(COMMANDS.COLLABORATION_LIVE_LEAVE, {
        sessionId: this.sessionId
      });
    } catch (error) {
      console.error('LiveWidget: Failed to leave:', error);
    }

    this.isJoined = false;
    this.sessionId = null;
    this.participants = [];
    this.cleanup();
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

    // Audio stream (mixed, excludes self)
    this.unsubscribers.push(
      Events.subscribe(`live:audio:${this.sessionId}`, (data: any) => {
        this.playAudioChunk(data.audio, data.sampleRate);
      })
    );
  }

  private async toggleMic(): Promise<void> {
    this.micEnabled = !this.micEnabled;

    if (this.micEnabled) {
      try {
        this.localStream = await navigator.mediaDevices.getUserMedia({ audio: true });
        // TODO: Stream audio to server
      } catch (error) {
        console.error('LiveWidget: Failed to get mic:', error);
        this.micEnabled = false;
      }
    } else {
      if (this.localStream) {
        this.localStream.getAudioTracks().forEach(track => track.stop());
      }
    }

    // Notify server
    if (this.sessionId) {
      await Commands.execute<any, any>('live/mic', {
        sessionId: this.sessionId,
        enabled: this.micEnabled
      });
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

  private playAudioChunk(base64Audio: string, sampleRate: number): void {
    // Decode and play audio chunk
    // TODO: Implement audio playback queue
  }

  protected override render(): TemplateResult {
    if (!this.isJoined) {
      return html`
        <div class="live-container">
          <div class="join-prompt">
            <div class="empty-state">
              <p>Join the live session to talk with others</p>
            </div>
            <button class="join-btn" @click=${this.handleJoin}>
              Join Live
            </button>
          </div>
        </div>
      `;
    }

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
