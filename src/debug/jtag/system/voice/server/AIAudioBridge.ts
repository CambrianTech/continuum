/**
 * AIAudioBridge - Connects AI personas to the Rust streaming-core call server
 *
 * ARCHITECTURE NOTE: Transcription is handled by Rust call_server, NOT here.
 * Rust does VAD-based speech detection and runs Whisper natively.
 * Transcriptions flow: Rust → browser WebSocket → Events → VoiceOrchestrator
 *
 * This bridge ONLY handles:
 * 1. TTS injection (AI speaking INTO the call)
 * 2. Maintaining WebSocket connections for AI participants
 *
 * Previously this did TypeScript-side transcription (buffer concat, RMS, base64)
 * which was wasteful - all that work is now done efficiently in Rust.
 */

import WebSocket from 'ws';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { getVoiceService } from './VoiceService';
// Note: adapter selection is now handled by VoiceService config (no hardcoded adapter here)
import { Events } from '../../core/shared/Events';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { EVENT_SCOPES } from '../../events/shared/EventSystemConstants';

// CallMessage types matching Rust call_server.rs
interface JoinMessage {
  type: 'Join';
  call_id: string;
  user_id: string;
  display_name: string;
  is_ai: boolean;  // AI participants get server-side audio buffering
}

interface AudioMessage {
  type: 'Audio';
  data: string; // base64 encoded i16 PCM
}

interface LeaveMessage {
  type: 'Leave';
}

// Messages we send/receive over the WebSocket (audio flows as binary frames)
type CallMessage = JoinMessage | AudioMessage | LeaveMessage | { type: string };

interface AIConnection {
  ws: WebSocket;
  callId: string;
  userId: string;
  displayName: string;
  isConnected: boolean;
  reconnectAttempts?: number;
  intentionalDisconnect?: boolean;  // true if explicitly called leaveCall()
  reconnectTimeout?: NodeJS.Timeout;
}

const STREAMING_CORE_URL = process.env.STREAMING_CORE_WS_URL || 'ws://127.0.0.1:50053';

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
          isConnected: false,
        };

        ws.on('open', () => {
          console.log(`🤖 AIAudioBridge: ${displayName} connected to call server`);

          // Send join message - is_ai: true enables server-side audio buffering
          const joinMsg: JoinMessage = {
            type: 'Join',
            call_id: callId,
            user_id: userId,
            display_name: displayName,
            is_ai: true,  // CRITICAL: Server creates ring buffer for AI participants
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

        ws.on('close', (code, reason) => {
          console.log(`🤖 AIAudioBridge: ${displayName} disconnected (code: ${code}, reason: ${reason})`);

          // Check if this was an intentional disconnect (user called leaveCall)
          if (connection.intentionalDisconnect) {
            console.log(`🤖 AIAudioBridge: ${displayName} intentionally left, not reconnecting`);
            this.connections.delete(key);
            return;
          }

          // Unintentional disconnect - attempt reconnection with exponential backoff
          connection.isConnected = false;
          this.scheduleReconnect(key, connection);
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
      // Mark as intentional disconnect to prevent reconnection
      connection.intentionalDisconnect = true;

      // Cancel any pending reconnect
      if (connection.reconnectTimeout) {
        clearTimeout(connection.reconnectTimeout);
        connection.reconnectTimeout = undefined;
      }

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
   * Schedule reconnection with exponential backoff
   */
  private scheduleReconnect(key: string, oldConnection: AIConnection): void {
    const maxRetries = 10;
    const attempts = (oldConnection.reconnectAttempts || 0) + 1;

    if (attempts > maxRetries) {
      console.error(`🤖 AIAudioBridge: ${oldConnection.displayName} exceeded max reconnect attempts (${maxRetries})`);
      this.connections.delete(key);
      return;
    }

    // Exponential backoff: 1s, 2s, 4s, 8s, 16s, 32s, 64s, 128s, 256s, 512s (max ~8.5 minutes)
    const backoffMs = Math.min(1000 * Math.pow(2, attempts - 1), 512000);

    console.log(`🤖 AIAudioBridge: ${oldConnection.displayName} will reconnect in ${backoffMs}ms (attempt ${attempts}/${maxRetries})`);

    oldConnection.reconnectTimeout = setTimeout(() => {
      console.log(`🤖 AIAudioBridge: Reconnecting ${oldConnection.displayName} (attempt ${attempts}/${maxRetries})`);

      // Attempt reconnection
      this.joinCall(oldConnection.callId, oldConnection.userId, oldConnection.displayName)
        .then((success) => {
          if (success) {
            console.log(`🤖 AIAudioBridge: ${oldConnection.displayName} reconnected successfully`);
            // Reset reconnect counter on successful connection
            const connection = this.connections.get(key);
            if (connection) {
              connection.reconnectAttempts = 0;
            }
          } else {
            console.warn(`🤖 AIAudioBridge: ${oldConnection.displayName} reconnect failed`);
            // joinCall handles error, will trigger close event and schedule another reconnect
          }
        })
        .catch((error) => {
          console.error(`🤖 AIAudioBridge: Reconnect error for ${oldConnection.displayName}:`, error);
        });

    }, backoffMs);

    // Store updated connection with incremented attempt count
    oldConnection.reconnectAttempts = attempts;
    this.connections.set(key, oldConnection);
  }

  /**
   * Inject TTS audio into the call (AI speaking)
   * @param voice - Voice identifier passed to Rust TTS adapter. Can be:
   *                - Named voice ("af", "am_adam") → used directly
   *                - Numeric seed ("42") → modulo into adapter's voice list
   *                - Any string (uniqueId, UUID, display name) → hashed to pick voice
   *                If not provided, uses userId so each AI gets a consistent unique voice.
   *                The Rust adapter's resolve_voice() handles all mapping.
   */
  async speak(callId: string, userId: UUID, text: string, voice?: string): Promise<void> {
    const key = `${callId}-${userId}`;
    const connection = this.connections.get(key);

    if (!connection || !connection.isConnected) {
      console.warn(`🤖 AIAudioBridge: Cannot speak - ${userId.slice(0, 8)} not in call (connection exists=${!!connection}, isConnected=${connection?.isConnected})`);

      // CRITICAL: Emit speech event even on failure so VoiceOrchestrator
      // can clear cooldown lock and chain to the next responder.
      // Without this, the voice coordination chain breaks permanently.
      const failedEvent = {
        sessionId: callId,
        speakerId: userId,
        speakerName: 'unknown',
        text,
        audioDurationMs: 0,
        failed: true,
        timestamp: Date.now()
      };

      if (DataDaemon.jtagContext) {
        await Events.emit(DataDaemon.jtagContext, 'voice:ai:speech', failedEvent, { scope: EVENT_SCOPES.GLOBAL });
      } else {
        Events.emit('voice:ai:speech', failedEvent);
      }
      return;
    }

    try {
      // Pass userId as voice identifier — Rust adapter's resolve_voice() handles mapping
      // This ensures each AI always gets a consistent unique voice per adapter
      const voiceId = voice ?? userId;

      // Use VoiceService (handles TTS synthesis)
      const voiceService = getVoiceService();
      const result = await voiceService.synthesizeSpeech({
        text,
        userId,
        voice: voiceId,  // Speaker ID for multi-speaker models
        // adapter comes from VoiceService config (default: kokoro)
      });

      // result.audioSamples is already i16 array ready to send
      const samples = result.audioSamples;
      const audioDurationSec = samples.length / 16000;

      // SERVER-SIDE BUFFERING: Send ALL audio at once
      // Rust server has a 60-second ring buffer per AI participant
      // Server pulls frames at precise 32ms intervals (tokio::time::interval)
      // This eliminates JavaScript timing jitter from the audio pipeline

      console.log(`🤖 AIAudioBridge: ${connection.displayName} sending ${samples.length} samples (${audioDurationSec.toFixed(1)}s) to server buffer`);

      // Send audio in chunks to avoid WebSocket frame size limits
      const chunkSize = 16000 * 5; // 5 seconds per chunk
      for (let offset = 0; offset < samples.length; offset += chunkSize) {
        const chunk = samples.slice(offset, Math.min(offset + chunkSize, samples.length));

        if (connection.ws.readyState === WebSocket.OPEN) {
          const buffer = Buffer.from(chunk.buffer, chunk.byteOffset, chunk.byteLength);
          connection.ws.send(buffer);
        }
      }

      // BROADCAST to browser + other AIs: Emit AFTER TTS synthesis and audio send
      // This syncs caption display with actual audio playback (audio is now in server buffer)
      // Browser LiveWidget subscribes to show AI caption/speaker highlight
      const speechEvent = {
        sessionId: callId,
        speakerId: userId,
        speakerName: connection.displayName,
        text,
        audioDurationMs: Math.round(audioDurationSec * 1000),
        timestamp: Date.now()
      };

      if (DataDaemon.jtagContext) {
        console.log(`🤖 AIAudioBridge: Emitting voice:ai:speech (audioDurationMs=${speechEvent.audioDurationMs})`);
        await Events.emit(
          DataDaemon.jtagContext,
          'voice:ai:speech',
          speechEvent,
          {
            scope: EVENT_SCOPES.GLOBAL  // Broadcast to all environments including browser
          }
        );
      } else {
        // Fallback: emit without context (auto-context mode)
        console.warn(`🤖 AIAudioBridge: DataDaemon.jtagContext is null, emitting voice:ai:speech without context`);
        Events.emit('voice:ai:speech', speechEvent);
      }

      console.log(`🤖 AIAudioBridge: ${connection.displayName} spoke: "${text.slice(0, 50)}..."`);

    } catch (error) {
      console.error(`🤖 AIAudioBridge: TTS/send error:`, error);

      // CRITICAL: Emit speech event on failure so VoiceOrchestrator
      // can clear cooldown lock and chain to the next responder.
      // Without this, TTS timeout (30s) permanently blocks the voice chain.
      const failedEvent = {
        sessionId: callId,
        speakerId: userId,
        speakerName: connection.displayName,
        text,
        audioDurationMs: 0,
        failed: true,
        timestamp: Date.now()
      };

      if (DataDaemon.jtagContext) {
        await Events.emit(DataDaemon.jtagContext, 'voice:ai:speech', failedEvent, { scope: EVENT_SCOPES.GLOBAL });
      } else {
        Events.emit('voice:ai:speech', failedEvent);
      }
    }
  }

  /**
   * Handle incoming messages from call server
   *
   * NOTE: Transcription is now handled by Rust call_server with VAD.
   * This handler only processes control messages, not audio.
   */
  private handleMessage(_key: string, data: WebSocket.Data): void {
    // Could handle Transcription messages here if needed
    // But transcriptions flow: Rust → browser → Events → VoiceOrchestrator
    // So server-side AIs don't need to process them here
    try {
      const msg = JSON.parse(data.toString()) as CallMessage;
      // JSON messages are control/transcription only; audio arrives as binary frames
      void msg;
    } catch {
      // Binary data - ignore
    }
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
