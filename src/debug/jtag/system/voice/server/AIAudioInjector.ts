/**
 * AIAudioInjector - Server-side audio injection for AI voice responses
 *
 * Allows PersonaUsers to push synthesized TTS audio into CallServer
 * as if they were call participants. This enables AI voice responses
 * to be mixed with human audio in real-time.
 *
 * Architecture:
 * 1. PersonaUser generates TTS audio
 * 2. AIAudioInjector connects to CallServer WebSocket (as participant)
 * 3. TTS audio is chunked and pushed via WebSocket
 * 4. CallServer mixer treats AI as regular participant
 * 5. Mixed audio (human + AI) broadcasts to all participants
 */

import WebSocket from 'ws';
import { Events } from '../../core/shared/Events';

interface CallMessage {
  type: string;
  call_id?: string;
  user_id?: string;
  display_name?: string;
}

interface AIAudioInjectorOptions {
  serverUrl?: string;
  sampleRate?: number;
  frameSize?: number;
}

export class AIAudioInjector {
  private ws: WebSocket | null = null;
  private serverUrl: string;
  private sampleRate: number;
  private frameSize: number;

  private callId: string | null = null;
  private userId: string | null = null;
  private displayName: string | null = null;
  private connected = false;

  constructor(options: AIAudioInjectorOptions = {}) {
    this.serverUrl = options.serverUrl || 'ws://127.0.0.1:50053';
    this.sampleRate = options.sampleRate || 16000;
    this.frameSize = options.frameSize || 512;
  }

  /**
   * Connect to CallServer and join as AI participant
   */
  async join(callId: string, userId: string, displayName: string): Promise<void> {
    this.callId = callId;
    this.userId = userId;
    this.displayName = displayName;

    return new Promise((resolve, reject) => {
      try {
        this.ws = new WebSocket(this.serverUrl);

        this.ws.on('open', () => {
          console.log(`🎙️ ${displayName}: Connected to CallServer`);
          this.connected = true;

          // Send join message
          const joinMsg: CallMessage = {
            type: 'Join',
            call_id: callId,
            user_id: userId,
            display_name: displayName,
          };
          this.ws?.send(JSON.stringify(joinMsg));
          resolve();
        });

        this.ws.on('message', (data) => {
          // Handle any messages from server (transcriptions, etc.)
          try {
            const msg = JSON.parse(data.toString());
            if (msg.type === 'Transcription') {
              console.log(`🎙️ ${displayName}: Transcription: "${msg.text}"`);
            }
          } catch (e) {
            // Binary audio data - AIs don't need to receive mixed audio
          }
        });

        this.ws.on('error', (error) => {
          console.error(`🎙️ ${displayName}: WebSocket error:`, error);
          reject(error);
        });

        this.ws.on('close', () => {
          console.log(`🎙️ ${displayName}: Disconnected from CallServer`);
          this.connected = false;
        });
      } catch (error) {
        reject(error);
      }
    });
  }

  /**
   * Inject TTS audio into the call
   * Audio must be Int16Array at 16kHz sample rate
   */
  async injectAudio(audioSamples: Int16Array): Promise<void> {
    if (!this.connected || !this.ws || this.ws.readyState !== WebSocket.OPEN) {
      console.warn(`🎙️ ${this.displayName}: Cannot inject audio - not connected`);
      return;
    }

    const totalSamples = audioSamples.length;
    console.log(
      `🎙️ ${this.displayName}: Injecting ${totalSamples} samples (${(totalSamples / this.sampleRate).toFixed(2)}s)`
    );

    // Chunk audio into frameSize chunks and send as BINARY WebSocket frames
    // Direct bytes transfer - no JSON, no base64 encoding overhead
    for (let offset = 0; offset < totalSamples; offset += this.frameSize) {
      if (this.ws.readyState !== WebSocket.OPEN) break;

      const end = Math.min(offset + this.frameSize, totalSamples);
      const chunk = audioSamples.subarray(offset, end);

      // Convert to Buffer (little-endian Int16) and send directly
      // Rust server receives as Message::Binary and converts with bytes_to_i16()
      const buffer = Buffer.allocUnsafe(chunk.length * 2);
      for (let i = 0; i < chunk.length; i++) {
        buffer.writeInt16LE(chunk[i], i * 2);
      }

      // Send raw binary - no JSON wrapper, no base64 encoding
      this.ws.send(buffer);

      // Pace audio at real-time rate (frameSize samples at sampleRate Hz)
      const frameDurationMs = (this.frameSize / this.sampleRate) * 1000;
      await this.delay(frameDurationMs);
    }

    console.log(`🎙️ ${this.displayName}: Audio injection complete`);
  }

  /**
   * Leave the call and disconnect
   */
  async leave(): Promise<void> {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      const leaveMsg: CallMessage = {
        type: 'Leave',
      };
      this.ws.send(JSON.stringify(leaveMsg));
      this.ws.close();
    }
    this.connected = false;
    this.ws = null;
  }

  private delay(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * Static factory: Create injector and auto-join call
   */
  static async create(
    callId: string,
    userId: string,
    displayName: string,
    options?: AIAudioInjectorOptions
  ): Promise<AIAudioInjector> {
    const injector = new AIAudioInjector(options);
    await injector.join(callId, userId, displayName);
    return injector;
  }

  /**
   * Static helper: Inject audio to a call (auto join/leave)
   */
  static async injectToCall(
    callId: string,
    userId: string,
    displayName: string,
    audioSamples: Int16Array
  ): Promise<void> {
    const injector = await AIAudioInjector.create(callId, userId, displayName);
    try {
      await injector.injectAudio(audioSamples);
    } finally {
      // Wait a bit before leaving to ensure audio finishes
      await injector.delay(100);
      await injector.leave();
    }
  }

  /**
   * Subscribe to voice:audio:${handle} events and inject to call
   * This is the bridge between TTS synthesis and CallServer
   *
   * NOTE: Currently not working because voice:audio events lack callId/sessionId.
   * This needs to be fixed in VoiceSynthesizeServerCommand to include session context.
   */
  static subscribeToTTSEvents(personaId: string, personaName: string): () => void {
    console.log(`🎙️ ${personaName}: Subscribing to TTS audio events (PROTOTYPE - needs callId in events)`);

    // Track active injectors by call ID
    const activeInjectors = new Map<string, AIAudioInjector>();

    // Subscribe to voice:audio:* events (pattern matching)
    // NOTE: Events.subscribe doesn't pass eventName to listener, so we can't extract handle
    // For now, this is a prototype - full implementation needs event naming refactor
    const unsubscribe = Events.subscribe('voice:audio:*', (data: any) => {
      (async () => {
        console.log(`🎙️ ${personaName}: Received TTS audio event`);

        // Decode base64 audio to Int16Array
        const audioBase64 = data.audio;
        if (!audioBase64) {
          console.warn(`🎙️ ${personaName}: No audio in event`);
          return;
        }

        const audioBuffer = Buffer.from(audioBase64, 'base64');
        const audioSamples = new Int16Array(
          audioBuffer.buffer,
          audioBuffer.byteOffset,
          audioBuffer.byteLength / 2
        );

        // Get call ID from context
        // NOTE: callId = voice call ID (not JTAG sessionId)
        // TODO: VoiceSynthesizeServerCommand needs to add callId to events
        const callId = data.callId;
        if (!callId) {
          console.warn(`🎙️ ${personaName}: No callId in TTS event (VoiceSynthesizeServerCommand needs to include voice call ID)`);
          return;
        }

        // Get or create injector for this call
        let injector = activeInjectors.get(callId);
        if (!injector || !injector['connected']) {
          console.log(`🎙️ ${personaName}: Creating new injector for call ${callId}`);
          injector = await AIAudioInjector.create(callId, personaId, personaName);
          activeInjectors.set(callId, injector);
        }

        // Inject audio
        await injector.injectAudio(audioSamples);
      })().catch((error) => {
        console.error(`🎙️ ${personaName}: Audio injection error:`, error);
      });
    });

    return () => {
      unsubscribe();
      // Cleanup all injectors
      for (const injector of activeInjectors.values()) {
        injector.leave().catch(() => {});
      }
      activeInjectors.clear();
    };
  }
}
