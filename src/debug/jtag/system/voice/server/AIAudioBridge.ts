/**
 * AIAudioBridge - Connects AI personas to the Rust streaming-core call server
 *
 * When an AI joins a voice call:
 * 1. Opens WebSocket to Rust streaming-core as that AI
 * 2. Receives mixed audio from other participants
 * 3. Sends audio to STT for transcription (AI "hears" the call)
 * 4. When AI responds, TTS audio is injected into the call
 *
 * This bridges the persona system with the audio call system.
 */

import WebSocket from 'ws';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { Commands } from '../../core/shared/Commands';
import type { VoiceTranscribeParams, VoiceTranscribeResult } from '../../../commands/voice/transcribe/shared/VoiceTranscribeTypes';
import type { VoiceSynthesizeParams, VoiceSynthesizeResult } from '../../../commands/voice/synthesize/shared/VoiceSynthesizeTypes';

// CallMessage types matching Rust call_server.rs
interface JoinMessage {
  type: 'Join';
  call_id: string;
  user_id: string;
  display_name: string;
}

interface AudioMessage {
  type: 'Audio';
  data: string; // base64 encoded i16 PCM
}

interface MixedAudioMessage {
  type: 'MixedAudio';
  data: string; // base64 encoded i16 PCM
}

interface LeaveMessage {
  type: 'Leave';
}

type CallMessage = JoinMessage | AudioMessage | MixedAudioMessage | LeaveMessage | { type: string };

interface AIConnection {
  ws: WebSocket;
  callId: string;
  userId: string;
  displayName: string;
  audioBuffer: Int16Array[];
  lastTranscription: number;
  isConnected: boolean;
}

const STREAMING_CORE_URL = process.env.STREAMING_CORE_WS_URL || 'ws://127.0.0.1:50053';
const SAMPLE_RATE = 16000;
const TRANSCRIPTION_INTERVAL_MS = 3000; // Send accumulated audio to STT every 3 seconds

export class AIAudioBridge {
  private static _instance: AIAudioBridge | null = null;
  private connections: Map<string, AIConnection> = new Map(); // keyed by `${callId}-${userId}`

  private constructor() {
    console.log('🤖 AIAudioBridge: Initialized');
  }

  static get instance(): AIAudioBridge {
    if (!AIAudioBridge._instance) {
      AIAudioBridge._instance = new AIAudioBridge();
    }
    return AIAudioBridge._instance;
  }

  /**
   * Connect an AI participant to a voice call
   */
  async joinCall(callId: string, userId: UUID, displayName: string): Promise<boolean> {
    const key = `${callId}-${userId}`;

    if (this.connections.has(key)) {
      console.log(`🤖 AIAudioBridge: ${displayName} already in call ${callId.slice(0, 8)}`);
      return true;
    }

    return new Promise((resolve) => {
      try {
        const ws = new WebSocket(STREAMING_CORE_URL);

        const connection: AIConnection = {
          ws,
          callId,
          userId,
          displayName,
          audioBuffer: [],
          lastTranscription: Date.now(),
          isConnected: false,
        };

        ws.on('open', () => {
          console.log(`🤖 AIAudioBridge: ${displayName} connected to call server`);

          // Send join message
          const joinMsg: JoinMessage = {
            type: 'Join',
            call_id: callId,
            user_id: userId,
            display_name: displayName,
          };
          ws.send(JSON.stringify(joinMsg));
          connection.isConnected = true;
          this.connections.set(key, connection);
          resolve(true);
        });

        ws.on('message', (data: WebSocket.Data) => {
          this.handleMessage(key, data);
        });

        ws.on('error', (error) => {
          console.error(`🤖 AIAudioBridge: WebSocket error for ${displayName}:`, error);
          resolve(false);
        });

        ws.on('close', () => {
          console.log(`🤖 AIAudioBridge: ${displayName} disconnected from call`);
          this.connections.delete(key);
        });

      } catch (error) {
        console.error(`🤖 AIAudioBridge: Failed to connect ${displayName}:`, error);
        resolve(false);
      }
    });
  }

  /**
   * Disconnect AI from a voice call
   */
  leaveCall(callId: string, userId: UUID): void {
    const key = `${callId}-${userId}`;
    const connection = this.connections.get(key);

    if (connection) {
      if (connection.ws.readyState === WebSocket.OPEN) {
        const leaveMsg: LeaveMessage = { type: 'Leave' };
        connection.ws.send(JSON.stringify(leaveMsg));
        connection.ws.close();
      }
      this.connections.delete(key);
      console.log(`🤖 AIAudioBridge: ${connection.displayName} left call ${callId.slice(0, 8)}`);
    }
  }

  /**
   * Inject TTS audio into the call (AI speaking)
   */
  async speak(callId: string, userId: UUID, text: string): Promise<void> {
    const key = `${callId}-${userId}`;
    const connection = this.connections.get(key);

    if (!connection || !connection.isConnected) {
      console.warn(`🤖 AIAudioBridge: Cannot speak - ${userId.slice(0, 8)} not in call`);
      return;
    }

    try {
      // Generate TTS audio
      const ttsResult = await Commands.execute<VoiceSynthesizeParams, VoiceSynthesizeResult>(
        'voice/synthesize',
        {
          text,
          voice: 'default',
          format: 'pcm16',
        }
      );

      if (!ttsResult.success || !ttsResult.audio) {
        console.warn(`🤖 AIAudioBridge: TTS failed for ${connection.displayName}`);
        return;
      }

      // Send audio in chunks to the call
      const audioData = Buffer.from(ttsResult.audio, 'base64');
      const samples = new Int16Array(audioData.buffer, audioData.byteOffset, audioData.byteLength / 2);

      // Send in ~20ms chunks (320 samples at 16kHz)
      const chunkSize = 320;
      for (let i = 0; i < samples.length; i += chunkSize) {
        const chunk = samples.slice(i, i + chunkSize);
        const base64Chunk = this.int16ToBase64(chunk);

        const audioMsg: AudioMessage = {
          type: 'Audio',
          data: base64Chunk,
        };

        if (connection.ws.readyState === WebSocket.OPEN) {
          connection.ws.send(JSON.stringify(audioMsg));
        }

        // Small delay between chunks to simulate real-time playback
        await this.sleep(20);
      }

      console.log(`🤖 AIAudioBridge: ${connection.displayName} spoke: "${text.slice(0, 50)}..."`);

    } catch (error) {
      console.error(`🤖 AIAudioBridge: TTS/send error:`, error);
    }
  }

  /**
   * Handle incoming messages from call server
   */
  private handleMessage(key: string, data: WebSocket.Data): void {
    const connection = this.connections.get(key);
    if (!connection) return;

    try {
      const msg = JSON.parse(data.toString()) as CallMessage;

      if (msg.type === 'MixedAudio') {
        // AI receives audio from other participants
        this.handleMixedAudio(connection, (msg as MixedAudioMessage).data);
      }
    } catch (error) {
      // Might be binary data
    }
  }

  /**
   * Handle received mixed audio (AI "hears" other participants)
   */
  private handleMixedAudio(connection: AIConnection, base64Data: string): void {
    // Decode audio
    const audioData = Buffer.from(base64Data, 'base64');
    const samples = new Int16Array(audioData.buffer, audioData.byteOffset, audioData.byteLength / 2);

    // Buffer audio for transcription
    connection.audioBuffer.push(samples);

    // Check if enough time has passed for transcription
    const now = Date.now();
    if (now - connection.lastTranscription >= TRANSCRIPTION_INTERVAL_MS) {
      this.transcribeBufferedAudio(connection);
      connection.lastTranscription = now;
    }
  }

  /**
   * Transcribe buffered audio (AI "understands" what was said)
   */
  private async transcribeBufferedAudio(connection: AIConnection): Promise<void> {
    if (connection.audioBuffer.length === 0) return;

    // Concatenate all buffered audio
    const totalLength = connection.audioBuffer.reduce((sum, arr) => sum + arr.length, 0);
    const concatenated = new Int16Array(totalLength);
    let offset = 0;
    for (const chunk of connection.audioBuffer) {
      concatenated.set(chunk, offset);
      offset += chunk.length;
    }

    // Clear buffer
    connection.audioBuffer = [];

    // Check if audio is mostly silence (skip transcription)
    const rms = this.calculateRMS(concatenated);
    if (rms < 100) {
      // Too quiet - probably just hold tone or silence
      return;
    }

    // Convert to base64 for STT
    const base64Audio = this.int16ToBase64(concatenated);

    try {
      const result = await Commands.execute<VoiceTranscribeParams, VoiceTranscribeResult>(
        'voice/transcribe',
        {
          audio: base64Audio,
          language: 'en',
        }
      );

      if (result.success && result.text && result.text.trim().length > 2) {
        console.log(`🤖 AIAudioBridge: ${connection.displayName} heard: "${result.text}"`);

        // TODO: Route this to VoiceOrchestrator for persona processing
        // For now, just log it
      }
    } catch (error) {
      console.warn(`🤖 AIAudioBridge: STT error:`, error);
    }
  }

  /**
   * Calculate RMS of audio samples
   */
  private calculateRMS(samples: Int16Array): number {
    if (samples.length === 0) return 0;
    const sumSquares = samples.reduce((sum, s) => sum + s * s, 0);
    return Math.sqrt(sumSquares / samples.length);
  }

  /**
   * Convert Int16Array to base64
   */
  private int16ToBase64(samples: Int16Array): string {
    const buffer = Buffer.from(samples.buffer, samples.byteOffset, samples.byteLength);
    return buffer.toString('base64');
  }

  /**
   * Sleep helper
   */
  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Check if AI is in a call
   */
  isInCall(callId: string, userId: UUID): boolean {
    const key = `${callId}-${userId}`;
    const connection = this.connections.get(key);
    return connection?.isConnected ?? false;
  }

  /**
   * Get all AI participants in a call
   */
  getAIParticipants(callId: string): string[] {
    const participants: string[] = [];
    for (const [key, conn] of this.connections) {
      if (key.startsWith(callId) && conn.isConnected) {
        participants.push(conn.displayName);
      }
    }
    return participants;
  }
}

// Singleton accessor
export function getAIAudioBridge(): AIAudioBridge {
  return AIAudioBridge.instance;
}
