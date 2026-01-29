/**
 * Voice Chat Widget
 *
 * Provides real-time voice communication with AI.
 * Uses AudioWorklet for low-latency capture/playback.
 * Streams audio over WebSocket to server.
 */

import { Events } from '@system/core/shared/Events';
import { Commands } from '@system/core/shared/Commands';
import type { VoiceStartParams, VoiceStartResult } from '@commands/voice/start/shared/VoiceStartTypes';
import type { VoiceStopParams, VoiceStopResult } from '@commands/voice/stop/shared/VoiceStopTypes';

import { VoiceStart } from '../../commands/voice/start/shared/VoiceStartTypes';
import { VoiceStop } from '../../commands/voice/stop/shared/VoiceStopTypes';
// Audio configuration
const SAMPLE_RATE = 16000;     // Target sample rate for speech
const CHUNK_DURATION_MS = 20;  // 20ms chunks
const CHUNK_SAMPLES = (SAMPLE_RATE * CHUNK_DURATION_MS) / 1000; // 320 samples

// Voice WebSocket server port (separate from main JTAG WebSocket)
const VOICE_WS_PORT = 3001;

export interface VoiceState {
  isConnected: boolean;
  isListening: boolean;
  isSpeaking: boolean;    // User is speaking
  isAISpeaking: boolean;  // AI is speaking
  audioLevel: number;     // 0-1 audio level
  transcription: string;  // Current transcription
  error: string | null;
}

/**
 * Voice Chat Widget Class
 *
 * Can be instantiated directly or used as a custom element.
 */
export class VoiceChatWidget {
  // Configuration
  public roomId: string = '';
  public handle: string = '';

  // State
  private voiceState: VoiceState = {
    isConnected: false,
    isListening: false,
    isSpeaking: false,
    isAISpeaking: false,
    audioLevel: 0,
    transcription: '',
    error: null
  };

  // Audio context and nodes
  private audioContext: AudioContext | null = null;
  private captureNode: AudioWorkletNode | null = null;
  private playbackNode: AudioWorkletNode | null = null;
  private mediaStream: MediaStream | null = null;

  // WebSocket connection
  private ws: WebSocket | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 3;

  // DOM element (if rendered)
  private element: HTMLElement | null = null;

  // State change callback
  private onStateChange?: (state: VoiceState) => void;

  constructor(options?: { roomId?: string; onStateChange?: (state: VoiceState) => void }) {
    if (options?.roomId) {
      this.roomId = options.roomId;
    }
    if (options?.onStateChange) {
      this.onStateChange = options.onStateChange;
    }
  }

  /**
   * Get current state
   */
  get state(): VoiceState {
    return { ...this.voiceState };
  }

  /**
   * Update state and notify listeners
   */
  private updateState(updates: Partial<VoiceState>): void {
    this.voiceState = { ...this.voiceState, ...updates };
    this.onStateChange?.(this.voiceState);
  }

  /**
   * Initialize audio system
   */
  async initAudio(): Promise<void> {
    try {
      // Create audio context
      this.audioContext = new AudioContext({
        sampleRate: 48000 // Standard rate, we'll downsample in worklet
      });

      // Load AudioWorklet processors
      const baseUrl = this.getWorkletBaseUrl();
      await this.audioContext.audioWorklet.addModule(`${baseUrl}/voice-capture-processor.js`);
      await this.audioContext.audioWorklet.addModule(`${baseUrl}/voice-playback-processor.js`);

      // Get microphone access
      this.mediaStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
          sampleRate: 48000
        }
      });

      // Create source from mic
      const source = this.audioContext.createMediaStreamSource(this.mediaStream);

      // Create capture worklet
      this.captureNode = new AudioWorkletNode(this.audioContext, 'voice-capture-processor');
      this.captureNode.port.postMessage({
        type: 'setSampleRate',
        sampleRate: this.audioContext.sampleRate
      });
      this.captureNode.port.onmessage = this.handleCaptureMessage.bind(this);

      // Connect mic -> capture processor
      source.connect(this.captureNode);

      // Create playback worklet
      this.playbackNode = new AudioWorkletNode(this.audioContext, 'voice-playback-processor');
      this.playbackNode.port.postMessage({
        type: 'setSampleRate',
        sampleRate: this.audioContext.sampleRate
      });
      this.playbackNode.port.onmessage = this.handlePlaybackMessage.bind(this);

      // Connect playback -> speakers
      this.playbackNode.connect(this.audioContext.destination);

      console.log('🎤 Audio system initialized');

    } catch (error) {
      console.error('Failed to initialize audio:', error);
      this.updateState({
        error: error instanceof Error ? error.message : 'Failed to access microphone'
      });
      throw error;
    }
  }

  /**
   * Get base URL for loading AudioWorklet modules
   */
  private getWorkletBaseUrl(): string {
    // Worklet files should be served from widgets/voice-chat/
    return '/widgets/voice-chat';
  }

  /**
   * Handle messages from capture worklet
   */
  private handleCaptureMessage(event: MessageEvent): void {
    const { type, samples, level, isSpeaking } = event.data;

    switch (type) {
      case 'audio':
        // Update level display
        this.updateState({ audioLevel: level });

        // Send to WebSocket if connected and listening
        if (this.ws?.readyState === WebSocket.OPEN && this.voiceState.isListening) {
          this.ws.send(samples);
        }
        break;

      case 'vadStart':
        this.updateState({ isSpeaking: true });
        Events.emit('voice:speaking:start', { roomId: this.roomId });
        break;

      case 'vadEnd':
        this.updateState({ isSpeaking: false });
        Events.emit('voice:speaking:end', { roomId: this.roomId });
        break;
    }
  }

  /**
   * Handle messages from playback worklet
   */
  private handlePlaybackMessage(event: MessageEvent): void {
    const { type } = event.data;

    switch (type) {
      case 'playbackStart':
        this.updateState({ isAISpeaking: true });
        Events.emit('voice:ai:speaking:start', { roomId: this.roomId });
        break;

      case 'playbackStop':
        this.updateState({ isAISpeaking: false });
        Events.emit('voice:ai:speaking:end', { roomId: this.roomId });
        break;

      case 'bufferUnderrun':
        console.warn('Audio buffer underrun');
        break;
    }
  }

  /**
   * Connect to voice WebSocket
   */
  private async connectWebSocket(): Promise<void> {
    return new Promise((resolve, reject) => {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const host = window.location.hostname;
      const wsUrl = `${protocol}//${host}:${VOICE_WS_PORT}?handle=${this.handle}&room=${this.roomId}`;

      console.log('🎤 Connecting to voice WebSocket:', wsUrl);
      this.ws = new WebSocket(wsUrl);
      this.ws.binaryType = 'arraybuffer';

      this.ws.onopen = () => {
        console.log('🔌 Voice WebSocket connected');
        this.updateState({ isConnected: true, error: null });
        this.reconnectAttempts = 0;
        resolve();
      };

      this.ws.onmessage = (event) => {
        if (event.data instanceof ArrayBuffer) {
          // Audio data from server - send to playback
          this.playbackNode?.port.postMessage({
            type: 'audio',
            samples: event.data
          }, [event.data]);
        } else {
          // JSON message (transcription, events, etc.)
          try {
            const message = JSON.parse(event.data);
            this.handleServerMessage(message);
          } catch (e) {
            console.error('Failed to parse server message:', e);
          }
        }
      };

      this.ws.onclose = (event) => {
        console.log('Voice WebSocket closed:', event.code, event.reason);
        this.updateState({ isConnected: false });

        // Attempt reconnect if not intentional close
        if (event.code !== 1000 && this.reconnectAttempts < this.maxReconnectAttempts) {
          this.reconnectAttempts++;
          setTimeout(() => this.connectWebSocket(), 1000 * this.reconnectAttempts);
        }
      };

      this.ws.onerror = (error) => {
        console.error('Voice WebSocket error:', error);
        this.updateState({ error: 'Connection error' });
        reject(error);
      };
    });
  }

  /**
   * Handle JSON messages from server
   */
  private handleServerMessage(message: any): void {
    switch (message.type) {
      case 'transcription':
        this.updateState({ transcription: message.text });
        Events.emit('voice:transcription', {
          roomId: this.roomId,
          text: message.text,
          isFinal: message.isFinal
        });
        break;

      case 'ai_response':
        Events.emit('voice:ai:response', {
          roomId: this.roomId,
          text: message.text
        });
        break;

      case 'error':
        this.updateState({ error: message.message });
        break;
    }
  }

  /**
   * Start voice chat
   */
  async start(): Promise<void> {
    try {
      // Resume audio context if suspended (browser autoplay policy)
      if (this.audioContext?.state === 'suspended') {
        await this.audioContext.resume();
      }

      // Initialize audio if needed
      if (!this.audioContext) {
        await this.initAudio();
      }

      // Start voice session via command to get handle
      if (!this.handle) {
        const result = await VoiceStart.execute({
          room: this.roomId || 'general',
        });

        if (!result.success) {
          throw new Error(result.error?.message || 'Failed to start voice session');
        }

        this.handle = result.handle;
        console.log('🎤 Voice session handle:', this.handle);
      }

      // Connect WebSocket if needed
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        await this.connectWebSocket();
      }

      this.updateState({ isListening: true, error: null });
      Events.emit('voice:start', { roomId: this.roomId, handle: this.handle });

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

    // Clear playback buffer (interrupt AI if speaking)
    this.playbackNode?.port.postMessage({ type: 'clear' });

    // Stop session via command
    if (this.handle) {
      try {
        await VoiceStop.execute({ handle: this.handle });
      } catch (error) {
        console.warn('Failed to stop voice session:', error);
      }
      this.handle = '';
    }

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
   * Interrupt AI (barge-in)
   */
  interrupt(): void {
    // Clear playback buffer
    this.playbackNode?.port.postMessage({ type: 'clear' });

    // Notify server
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: 'interrupt' }));
    }
  }

  /**
   * Clean up resources
   */
  destroy(): void {
    // Stop listening
    this.updateState({ isListening: false });

    // Close WebSocket
    if (this.ws) {
      this.ws.close(1000, 'Widget cleanup');
      this.ws = null;
    }

    // Stop media stream
    if (this.mediaStream) {
      this.mediaStream.getTracks().forEach(track => track.stop());
      this.mediaStream = null;
    }

    // Disconnect audio nodes
    this.captureNode?.disconnect();
    this.playbackNode?.disconnect();
    this.captureNode = null;
    this.playbackNode = null;

    // Close audio context
    if (this.audioContext) {
      this.audioContext.close();
      this.audioContext = null;
    }
  }
}

// Export for direct use
export default VoiceChatWidget;
