/**
 * GeminiLiveAdapter - Audio-native adapter for Google's Gemini Live API
 *
 * Gemini 2.5 Flash Native Audio supports:
 * - Direct audio input (no STT needed)
 * - Direct audio output (no TTS needed)
 * - 30 HD voices in 24 languages
 * - Free tier available
 *
 * @see https://ai.google.dev/api/live
 */

import WebSocket from 'ws';
import type {
  IAudioNativeAdapter,
  AudioNativeSessionConfig,
} from '../../shared/AudioNativeTypes';
import { DEFAULT_AUDIO_NATIVE_CONFIG } from '../../shared/AudioNativeTypes';

const GOOGLE_API_KEY = process.env.GOOGLE_API_KEY;

// Gemini Live API WebSocket endpoint
const GEMINI_LIVE_ENDPOINT = 'wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent';

/**
 * Gemini Live API adapter for audio-native conversations
 */
export class GeminiLiveAdapter implements IAudioNativeAdapter {
  readonly providerId = 'google';
  readonly modelId = 'gemini-2.5-flash-native-audio-preview';

  private ws: WebSocket | null = null;
  private sessionConfig: AudioNativeSessionConfig | null = null;
  private apiKey: string | undefined;
  private eventId = 0;

  // Callbacks
  private audioCallback?: (samples: Int16Array) => void;
  private transcriptCallback?: (text: string, isFinal: boolean) => void;
  private speechCallback?: (started: boolean) => void;
  private errorCallback?: (error: Error) => void;

  constructor(private customApiKey?: string) {
    this.apiKey = customApiKey || GOOGLE_API_KEY;
  }

  private nextEventId(): string {
    return `evt_${++this.eventId}`;
  }

  /**
   * Connect to Gemini Live API
   */
  async connect(config: Partial<AudioNativeSessionConfig> = {}): Promise<void> {
    if (!this.apiKey) {
      throw new Error('GOOGLE_API_KEY environment variable required');
    }

    this.sessionConfig = { ...DEFAULT_AUDIO_NATIVE_CONFIG, ...config };

    // Add API key as query parameter
    const endpoint = `${GEMINI_LIVE_ENDPOINT}?key=${this.apiKey}`;

    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(endpoint);

      this.ws.on('open', () => {
        console.log(`🔊 Gemini Live: Connected`);

        // Send setup message with model and config
        this.sendSetup(config);
        resolve();
      });

      this.ws.on('message', (data: WebSocket.Data) => {
        this.handleMessage(data);
      });

      this.ws.on('error', (error) => {
        console.error('🔊 Gemini Live: WebSocket error:', error);
        this.emitError(error);
        reject(error);
      });

      this.ws.on('close', (code, reason) => {
        console.log(`🔊 Gemini Live: Disconnected (${code}: ${reason})`);
        this.ws = null;
      });
    });
  }

  /**
   * Send setup message to configure the session
   */
  private sendSetup(config: Partial<AudioNativeSessionConfig>): void {
    // Gemini Live setup message format
    const setupMessage = {
      setup: {
        model: `models/${this.modelId}`,
        generationConfig: {
          responseModalities: ['AUDIO'],
          speechConfig: {
            voiceConfig: {
              prebuiltVoiceConfig: {
                voiceName: config.voice || 'Aoede', // Default Gemini voice
              },
            },
          },
        },
        systemInstruction: {
          parts: [{ text: config.instructions || 'You are a helpful assistant.' }],
        },
      },
    };

    this.ws?.send(JSON.stringify(setupMessage));
  }

  /**
   * Handle incoming WebSocket messages
   */
  private handleMessage(data: WebSocket.Data): void {
    try {
      const message = JSON.parse(data.toString());

      // Setup complete
      if (message.setupComplete) {
        console.log('🔊 Gemini Live: Session configured');
        return;
      }

      // Server content (audio, text, etc.)
      if (message.serverContent) {
        const content = message.serverContent;

        // Model turn with parts
        if (content.modelTurn?.parts) {
          for (const part of content.modelTurn.parts) {
            // Audio data
            if (part.inlineData?.mimeType?.startsWith('audio/')) {
              const audioData = Buffer.from(part.inlineData.data, 'base64');
              // Convert to Int16Array (assuming PCM16)
              const samples = new Int16Array(audioData.buffer, audioData.byteOffset, audioData.length / 2);
              this.audioCallback?.(samples);
            }

            // Text response
            if (part.text) {
              this.transcriptCallback?.(part.text, false);
            }
          }
        }

        // Turn complete
        if (content.turnComplete) {
          // Final transcript if available
          if (content.outputTranscription?.text) {
            this.transcriptCallback?.(content.outputTranscription.text, true);
          }
        }

        // Input transcription (what user said)
        if (content.inputTranscription?.text) {
          // Could emit this for display purposes
          console.log(`🔊 Gemini Live: User said: ${content.inputTranscription.text}`);
        }
      }

      // Tool calls (if we implement tools later)
      if (message.toolCall) {
        console.log('🔊 Gemini Live: Tool call received:', message.toolCall);
      }

    } catch (error) {
      console.error('🔊 Gemini Live: Failed to parse message:', error);
    }
  }

  /**
   * Disconnect from the API
   */
  async disconnect(): Promise<void> {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  /**
   * Send audio samples to the API
   */
  sendAudio(samples: Int16Array): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return;
    }

    // Convert Int16Array to base64
    const buffer = Buffer.from(samples.buffer, samples.byteOffset, samples.byteLength);
    const base64Audio = buffer.toString('base64');

    // Gemini realtime input format
    const message = {
      realtimeInput: {
        mediaChunks: [{
          mimeType: 'audio/pcm;rate=16000',
          data: base64Audio,
        }],
      },
    };

    this.ws.send(JSON.stringify(message));
  }

  /**
   * Cancel the current response
   */
  cancelResponse(): void {
    // Send interrupt/cancel message if supported
    // Gemini may use a different mechanism
    console.log('🔊 Gemini Live: Cancel not yet implemented');
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }

  // Callback registration
  onAudioOutput(callback: (samples: Int16Array) => void): void {
    this.audioCallback = callback;
  }

  onTranscript(callback: (text: string, isFinal: boolean) => void): void {
    this.transcriptCallback = callback;
  }

  onSpeechDetected(callback: (started: boolean) => void): void {
    this.speechCallback = callback;
  }

  onError(callback: (error: Error) => void): void {
    this.errorCallback = callback;
  }

  private emitError(error: Error | WebSocket.ErrorEvent): void {
    const err = error instanceof Error ? error : new Error(String(error));
    this.errorCallback?.(err);
  }
}
