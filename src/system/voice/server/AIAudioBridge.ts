/**
 * AIAudioBridge - Connects AI personas to the Rust streaming-core call server
 *
 * ARCHITECTURE:
 * - Transcription: Rust call_server does VAD + Whisper natively
 * - Speaking: Rust voice/speak-in-call synthesizes + injects audio in one call
 * - WebSocket connections: AI participants join calls for presence/mixing
 *
 * Audio NEVER leaves Rust. Synthesis and injection happen in one IPC call.
 * TypeScript only handles coordination (who speaks when) and event emission.
 */

import WebSocket from 'ws';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../workers/continuum-core/bindings/RustCoreIPC';
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
  private ipcClient: RustCoreIPCClient;

  private constructor() {
    this.ipcClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
    this.ipcClient.connect().catch(err => {
      console.error('🤖 AIAudioBridge: Failed to connect IPC to continuum-core:', err);
    });
    console.log('🤖 AIAudioBridge: Initialized (Rust IPC for synthesis + injection)');
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
   *
   * Audio NEVER leaves Rust. One IPC call does synthesis + injection.
   * TypeScript only coordinates (who speaks when) and emits events for UI.
   *
   * @param voice - Voice identifier passed to Rust TTS adapter. Can be:
   *                - Named voice ("alba", "marius") → used directly
   *                - Any string (uniqueId, UUID) → hashed to pick voice
   *                If not provided, uses userId so each AI gets a consistent voice.
   */
  async speak(callId: string, userId: UUID, text: string, voice?: string): Promise<void> {
    const key = `${callId}-${userId}`;
    const connection = this.connections.get(key);

    if (!connection || !connection.isConnected) {
      console.warn(`🤖 AIAudioBridge: Cannot speak - ${userId.slice(0, 8)} not in call (connection exists=${!!connection}, isConnected=${connection?.isConnected})`);
      await this.emitSpeechEvent(callId, userId, 'unknown', text, 0, true);
      return;
    }

    try {
      // Reconnect IPC if needed (Rust worker may have restarted)
      if (!this.ipcClient.connected) {
        await this.ipcClient.connect();
      }

      const voiceId = voice ?? userId;

      // ONE Rust IPC call: synthesize + inject into call mixer.
      // Audio stays in Rust — no TypeScript intermediation, no round-trips.
      // Explicit 'edge' adapter: <200ms, concurrent (no Mutex), 300+ distinct voices.
      // Pocket-TTS is 23x slower than realtime on CPU — unusable for live calls.
      const result = await this.ipcClient.voiceSpeakInCall(callId, userId, text, voiceId, 'edge');

      const audioDurationMs = result.durationMs;
      console.log(`🤖 AIAudioBridge: ${connection.displayName} spoke ${audioDurationMs}ms via Rust: "${text.slice(0, 50)}..."`);

      await this.emitSpeechEvent(callId, userId, connection.displayName, text, audioDurationMs, false);

    } catch (error) {
      console.error(`🤖 AIAudioBridge: speak failed for ${connection.displayName}:`, error);
      await this.emitSpeechEvent(callId, userId, connection.displayName, text, 0, true);
    }
  }

  /**
   * Emit voice:ai:speech event for VoiceOrchestrator coordination + browser UI.
   * ALWAYS emits on both success and failure — without this, the voice chain breaks.
   */
  private async emitSpeechEvent(
    callId: string,
    userId: UUID,
    displayName: string,
    text: string,
    audioDurationMs: number,
    failed: boolean
  ): Promise<void> {
    const event = {
      sessionId: callId,
      speakerId: userId,
      speakerName: displayName,
      text,
      audioDurationMs,
      failed,
      timestamp: Date.now()
    };

    if (DataDaemon.jtagContext) {
      await Events.emit(DataDaemon.jtagContext, 'voice:ai:speech', event, { scope: EVENT_SCOPES.GLOBAL });
    } else {
      Events.emit('voice:ai:speech', event);
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
