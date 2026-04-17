/**
 * Voice Chat Widget
 *
 * Provides real-time voice communication with AI via LiveKit WebRTC.
 * Uses AudioStreamClient for transport — LiveKit handles encoding,
 * VAD, transcription, and media routing through its SFU.
 */

import { Events } from '@system/core/shared/Events';
import type { VoiceStartResult } from '@commands/voice/start/shared/VoiceStartTypes';
import { VoiceStart } from '../../commands/voice/start/shared/VoiceStartTypes';
import { VoiceStop } from '../../commands/voice/stop/shared/VoiceStopTypes';
import { AudioStreamClient, type TranscriptionResult } from '../live/AudioStreamClient';

export interface VoiceState {
  isConnected: boolean;
  isListening: boolean;
  isSpeaking: boolean;
  isAISpeaking: boolean;
  audioLevel: number;
  transcription: string;
  error: string | null;
}

export class VoiceChatWidget {
  public roomId: string;
  public handle: string = '';

  private voiceState: VoiceState = {
    isConnected: false,
    isListening: false,
    isSpeaking: false,
    isAISpeaking: false,
    audioLevel: 0,
    transcription: '',
    error: null
  };

  private audioClient: AudioStreamClient | null = null;
  private element: HTMLElement | null = null;
  private onStateChange?: (state: VoiceState) => void;

  // Track active speakers to derive isSpeaking / isAISpeaking
  private activeSpeakers: Set<string> = new Set();
  private localUserId: string = '';

  constructor(options?: { roomId?: string; onStateChange?: (state: VoiceState) => void }) {
    this.roomId = options?.roomId ?? 'general';
    this.onStateChange = options?.onStateChange;
  }

  get state(): VoiceState {
    return { ...this.voiceState };
  }

  private updateState(updates: Partial<VoiceState>): void {
    this.voiceState = { ...this.voiceState, ...updates };
    this.onStateChange?.(this.voiceState);
  }

  /**
   * Start voice chat — requests session from server, connects to LiveKit
   */
  async start(): Promise<void> {
    try {
      // Get LiveKit credentials from voice/start command
      const result: VoiceStartResult = await VoiceStart.execute({
        room: this.roomId,
      });

      if (!result.success) {
        throw new Error(result.error?.message || 'Failed to start voice session');
      }

      this.handle = result.handle;
      this.localUserId = result.roomId; // sessionId used as identity in JWT

      // Create AudioStreamClient wired to our state
      this.audioClient = new AudioStreamClient({
        onConnectionChange: (connected) => {
          this.updateState({ isConnected: connected, error: connected ? null : 'Disconnected' });
        },
        onMicLevel: (level) => {
          this.updateState({ audioLevel: level });
        },
        onTranscription: (tx: TranscriptionResult) => {
          this.updateState({ transcription: tx.text });
          Events.emit('voice:transcription', {
            roomId: this.roomId,
            text: tx.text,
            userId: tx.userId,
            isFinal: true,
          });
        },
        onActiveSpeakersChanged: (speakerIds: string[]) => {
          this.activeSpeakers = new Set(speakerIds);
          const isSpeaking = this.activeSpeakers.has(this.localUserId);
          const isAISpeaking = speakerIds.some(id => id !== this.localUserId);

          this.updateState({ isSpeaking, isAISpeaking });

          if (isSpeaking) {
            Events.emit('voice:speaking:start', { roomId: this.roomId });
          } else {
            Events.emit('voice:speaking:end', { roomId: this.roomId });
          }
          if (isAISpeaking) {
            Events.emit('voice:ai:speaking:start', { roomId: this.roomId });
          } else {
            Events.emit('voice:ai:speaking:end', { roomId: this.roomId });
          }
        },
      });

      // Join LiveKit room
      await this.audioClient.join(
        result.roomId,
        this.localUserId,
        'Voice User',
        result.livekitUrl,
        result.livekitToken,
      );

      // Start publishing mic audio
      await this.audioClient.startMicrophone();

      this.updateState({ isListening: true, isConnected: true, error: null });
      Events.emit('voice:start', { roomId: this.roomId, handle: this.handle });

      console.log(`🎤 Voice session started: ${this.handle.substring(0, 8)}... in room ${result.roomId}`);

    } catch (error) {
      console.error('Failed to start voice:', error);
      this.updateState({
        error: error instanceof Error ? error.message : 'Failed to start voice'
      });
    }
  }

  /**
   * Stop voice chat
   */
  async stop(): Promise<void> {
    this.updateState({ isListening: false });

    // Disconnect from LiveKit
    if (this.audioClient) {
      this.audioClient.stopMicrophone();
      this.audioClient.leave();
      this.audioClient = null;
    }

    // End server-side session
    if (this.handle) {
      try {
        await VoiceStop.execute({ handle: this.handle });
      } catch (error) {
        console.warn('Failed to stop voice session:', error);
      }
      this.handle = '';
    }

    this.activeSpeakers.clear();
    this.updateState({ isConnected: false, isSpeaking: false, isAISpeaking: false, audioLevel: 0 });
    Events.emit('voice:stop', { roomId: this.roomId });
  }

  /**
   * Toggle voice chat
   */
  async toggle(): Promise<void> {
    if (this.voiceState.isListening) {
      await this.stop();
    } else {
      await this.start();
    }
  }

  /**
   * Interrupt AI (mute remote audio briefly — barge-in)
   */
  interrupt(): void {
    if (this.audioClient) {
      this.audioClient.setSpeakerMuted(true);
      // Unmute after a short window so the user can speak
      setTimeout(() => this.audioClient?.setSpeakerMuted(false), 500);
    }
  }

  /**
   * Clean up resources
   */
  destroy(): void {
    this.updateState({ isListening: false });

    if (this.audioClient) {
      this.audioClient.leave();
      this.audioClient = null;
    }

    this.activeSpeakers.clear();
  }
}

export default VoiceChatWidget;
