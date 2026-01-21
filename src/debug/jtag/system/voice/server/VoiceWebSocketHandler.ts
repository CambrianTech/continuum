/**
 * Voice WebSocket Handler
 *
 * Handles WebSocket connections for voice chat.
 * Binary audio data flows bidirectionally.
 * JSON messages for control (transcription, interrupts, etc.)
 */

import { WebSocket, WebSocketServer } from 'ws';
import { IncomingMessage } from 'http';
import { URL } from 'url';
import { VoiceSessionManager } from '@commands/voice/shared/VoiceSessionManager';
import { Events } from '@system/core/shared/Events';
import { Commands } from '@system/core/shared/Commands';
import type { VoiceTranscribeParams, VoiceTranscribeResult } from '@commands/voice/transcribe/shared/VoiceTranscribeTypes';
import type { VoiceSynthesizeParams, VoiceSynthesizeResult } from '@commands/voice/synthesize/shared/VoiceSynthesizeTypes';
import type { ChatSendParams, ChatSendResult } from '@commands/collaboration/chat/send/shared/ChatSendTypes';

// Audio configuration
const SAMPLE_RATE = 16000;
const BYTES_PER_SAMPLE = 2; // Int16
const CHUNK_DURATION_MS = 20;
const SAMPLES_PER_CHUNK = (SAMPLE_RATE * CHUNK_DURATION_MS) / 1000; // 320

interface VoiceConnection {
  ws: WebSocket;
  handle: string;
  roomId: string;
  userId: string;
  isListening: boolean;
  audioBuffer: Int16Array[];
}

/**
 * Voice WebSocket Server
 *
 * Runs on a separate port from the main JTAG WebSocket.
 * Handles binary audio streaming for voice chat.
 */
export class VoiceWebSocketServer {
  private server: WebSocketServer | null = null;
  private connections: Map<string, VoiceConnection> = new Map();
  private port: number;

  constructor(port: number = 3001) {
    this.port = port;
  }

  /**
   * Start the voice WebSocket server
   */
  async start(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.server = new WebSocketServer({ port: this.port });

      this.server.on('error', (error: Error) => {
        console.error('🎤 Voice WebSocket server error:', error);
        reject(error);
      });

      this.server.on('listening', () => {
        console.log(`🎤 Voice WebSocket server listening on port ${this.port}`);
        resolve();
      });

      this.server.on('connection', this.handleConnection.bind(this));
    });
  }

  /**
   * Stop the voice WebSocket server
   */
  async stop(): Promise<void> {
    if (!this.server) return;

    // Close all connections
    for (const [handle, conn] of this.connections) {
      conn.ws.close(1000, 'Server shutting down');
      VoiceSessionManager.endSession(handle);
    }
    this.connections.clear();

    return new Promise((resolve) => {
      this.server?.close(() => {
        console.log('🎤 Voice WebSocket server stopped');
        this.server = null;
        resolve();
      });
    });
  }

  /**
   * Handle new WebSocket connection
   */
  private handleConnection(ws: WebSocket, request: IncomingMessage): void {
    // Parse query parameters from URL
    const url = new URL(request.url || '', `http://localhost:${this.port}`);
    const handle = url.searchParams.get('handle');
    const roomId = url.searchParams.get('room') || 'general';

    if (!handle) {
      console.warn('🎤 Voice connection rejected: missing handle');
      ws.close(4000, 'Missing handle parameter');
      return;
    }

    // Verify session exists
    const session = VoiceSessionManager.getSession(handle);
    if (!session) {
      console.warn(`🎤 Voice connection rejected: unknown handle ${handle.substring(0, 8)}`);
      ws.close(4001, 'Unknown session handle');
      return;
    }

    console.log(`🎤 Voice connection established: ${handle.substring(0, 8)}... in room ${roomId}`);

    // Create connection tracking
    const connection: VoiceConnection = {
      ws,
      handle,
      roomId,
      userId: session.userId,
      isListening: false,
      audioBuffer: [],
    };

    this.connections.set(handle, connection);
    VoiceSessionManager.markConnected(handle);

    // Send welcome message
    this.sendJson(ws, {
      type: 'connected',
      handle,
      roomId,
    });

    // Set up event handlers
    ws.on('message', (data, isBinary) => {
      if (isBinary) {
        this.handleAudioData(connection, data as Buffer);
      } else {
        this.handleJsonMessage(connection, data.toString());
      }
    });

    ws.on('close', (code, reason) => {
      console.log(`🎤 Voice connection closed: ${handle.substring(0, 8)}... (${code})`);
      this.connections.delete(handle);
      VoiceSessionManager.endSession(handle);
    });

    ws.on('error', (error) => {
      console.error(`🎤 Voice connection error: ${handle.substring(0, 8)}...`, error);
      this.connections.delete(handle);
      VoiceSessionManager.endSession(handle);
    });
  }

  /**
   * Handle incoming binary audio data
   */
  private handleAudioData(connection: VoiceConnection, data: Buffer): void {
    // Convert Buffer to Int16Array
    const samples = new Int16Array(data.buffer, data.byteOffset, data.length / BYTES_PER_SAMPLE);

    // Buffer audio chunks
    connection.audioBuffer.push(samples);

    // Mark as listening
    if (!connection.isListening) {
      connection.isListening = true;
      VoiceSessionManager.setListening(connection.handle, true);
    }

    // Process buffered audio (accumulate ~500ms before processing)
    if (connection.audioBuffer.length >= 25) { // 25 * 20ms = 500ms
      this.processAudioBuffer(connection);
    }

    // Emit audio level for monitoring
    const level = this.calculateAudioLevel(samples);
    Events.emit('voice:audio:level', {
      handle: connection.handle,
      level,
    });
  }

  /**
   * Process accumulated audio buffer
   *
   * Flow: Audio → STT → Text → Chat Message → PersonaUser → Response → TTS → Audio
   */
  private async processAudioBuffer(connection: VoiceConnection): Promise<void> {
    // Concatenate buffered chunks
    const totalSamples = connection.audioBuffer.reduce((sum, chunk) => sum + chunk.length, 0);
    const combined = new Int16Array(totalSamples);
    let offset = 0;
    for (const chunk of connection.audioBuffer) {
      combined.set(chunk, offset);
      offset += chunk.length;
    }
    connection.audioBuffer = [];

    // Convert Int16Array to base64 for transport
    const audioBuffer = Buffer.from(combined.buffer, combined.byteOffset, combined.byteLength);
    const audioBase64 = audioBuffer.toString('base64');

    try {
      // Step 1: Transcribe audio to text via Rust Whisper
      console.log(`🎤 Transcribing ${totalSamples} samples (${(totalSamples / SAMPLE_RATE * 1000).toFixed(0)}ms)`);

      const transcribeResult = await Commands.execute<VoiceTranscribeParams, VoiceTranscribeResult>(
        'voice/transcribe',
        { audio: audioBase64 }
      );

      if (!transcribeResult.success || !transcribeResult.text.trim()) {
        console.log('🎤 No speech detected or transcription failed');
        return;
      }

      const transcribedText = transcribeResult.text;
      console.log(`🎤 Transcribed: "${transcribedText}"`);

      // Send transcription to client
      this.sendJson(connection.ws, {
        type: 'transcription',
        text: transcribedText,
        isFinal: true,
        language: transcribeResult.language,
        confidence: transcribeResult.confidence,
      });

      // Step 2: Send text as chat message (this triggers PersonaUser to respond)
      // We use the existing chat/send command which creates a message entity
      // PersonaUser will automatically see it and respond
      const chatResult = await Commands.execute<ChatSendParams, ChatSendResult>(
        'collaboration/chat/send',
        {
          room: connection.roomId,
          message: transcribedText,
        }
      );

      if (!chatResult.success) {
        console.error('🎤 Failed to send chat message:', chatResult.message);
        return;
      }

      // Step 3: Wait for AI response (PersonaUser will respond automatically)
      // We subscribe to the room's message events to catch the AI's response
      const aiResponse = await this.waitForAIResponse(connection, chatResult.messageEntity.id);

      if (!aiResponse) {
        console.log('🎤 No AI response received');
        return;
      }

      // Send AI response text to client
      this.sendJson(connection.ws, {
        type: 'ai_response',
        text: aiResponse,
      });

      // Step 4: Synthesize AI response to audio via Rust TTS
      console.log(`🎤 Synthesizing response: "${aiResponse.substring(0, 50)}..."`);

      VoiceSessionManager.setAISpeaking(connection.handle, true);

      const synthesizeResult = await Commands.execute<VoiceSynthesizeParams, VoiceSynthesizeResult>(
        'voice/synthesize',
        {
          text: aiResponse,
          adapter: 'kokoro', // Use best TTS
        }
      );

      if (synthesizeResult.success && synthesizeResult.audio) {
        // Decode base64 audio and send to client
        const audioData = Buffer.from(synthesizeResult.audio, 'base64');
        await this.sendAudioResponse(connection, audioData, synthesizeResult.sampleRate);
      } else {
        console.error('🎤 TTS synthesis failed:', synthesizeResult.error);
        // Fall back to silence
        await this.sendMockAudioResponse(connection);
      }

      VoiceSessionManager.setAISpeaking(connection.handle, false);

    } catch (error) {
      console.error('🎤 Voice processing error:', error);
      this.sendJson(connection.ws, {
        type: 'error',
        message: error instanceof Error ? error.message : 'Voice processing failed',
      });
    }
  }

  /**
   * Wait for AI response after sending chat message
   */
  private async waitForAIResponse(connection: VoiceConnection, originalMessageId: string): Promise<string | null> {
    return new Promise((resolve) => {
      const timeout = setTimeout(() => {
        unsubscribe();
        resolve(null);
      }, 10000); // 10 second timeout

      // Subscribe to room messages
      const unsubscribe = Events.subscribe(`data:chat_messages:created`, (message: any) => {
        // Check if this is an AI response to our message
        if (message.roomId === connection.roomId &&
            message.replyToId === originalMessageId &&
            message.metadata?.source !== 'voice') {
          clearTimeout(timeout);
          unsubscribe();
          resolve(message.content || message.text);
        }
      });

      // Also check for any AI message in the room after ours
      Events.subscribe(`chat:message:${connection.roomId}`, (message: any) => {
        if (message.userId !== connection.userId) {
          clearTimeout(timeout);
          unsubscribe();
          resolve(message.content || message.text);
        }
      });
    });
  }

  /**
   * Send synthesized audio response to client
   */
  private async sendAudioResponse(connection: VoiceConnection, audioData: Buffer, sampleRate: number): Promise<void> {
    // Audio is PCM16, need to chunk into 20ms frames
    const samplesPerChunk = Math.floor(sampleRate * CHUNK_DURATION_MS / 1000);
    const bytesPerChunk = samplesPerChunk * BYTES_PER_SAMPLE;

    for (let offset = 0; offset < audioData.length; offset += bytesPerChunk) {
      if (connection.ws.readyState !== WebSocket.OPEN) break;

      const chunkEnd = Math.min(offset + bytesPerChunk, audioData.length);
      const chunk = audioData.subarray(offset, chunkEnd);
      connection.ws.send(chunk);

      // Pace audio playback
      await this.delay(CHUNK_DURATION_MS);
    }
  }

  /**
   * Send mock audio response (silence)
   * In production, this would be TTS output
   */
  private async sendMockAudioResponse(connection: VoiceConnection): Promise<void> {
    // Send 1 second of silence (50 chunks of 20ms)
    const silentChunk = new Int16Array(SAMPLES_PER_CHUNK);

    for (let i = 0; i < 50; i++) {
      if (connection.ws.readyState === WebSocket.OPEN) {
        connection.ws.send(Buffer.from(silentChunk.buffer));
        await this.delay(CHUNK_DURATION_MS);
      } else {
        break;
      }
    }
  }

  /**
   * Handle incoming JSON message
   */
  private handleJsonMessage(connection: VoiceConnection, data: string): void {
    try {
      const message = JSON.parse(data);

      switch (message.type) {
        case 'interrupt':
          // User wants to interrupt AI
          console.log(`🎤 Interrupt requested: ${connection.handle.substring(0, 8)}`);
          VoiceSessionManager.setAISpeaking(connection.handle, false);
          this.sendJson(connection.ws, { type: 'interrupted' });
          break;

        case 'ping':
          this.sendJson(connection.ws, { type: 'pong' });
          break;

        default:
          console.log(`🎤 Unknown message type: ${message.type}`);
      }
    } catch (error) {
      console.error('🎤 Failed to parse JSON message:', error);
    }
  }

  /**
   * Calculate RMS audio level (0-1)
   */
  private calculateAudioLevel(samples: Int16Array): number {
    let sum = 0;
    for (let i = 0; i < samples.length; i++) {
      const normalized = samples[i] / 32768;
      sum += normalized * normalized;
    }
    return Math.sqrt(sum / samples.length);
  }

  /**
   * Send JSON message to client
   */
  private sendJson(ws: WebSocket, message: object): void {
    if (ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(message));
    }
  }

  /**
   * Utility delay function
   */
  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Broadcast to all connections in a room
   */
  broadcastToRoom(roomId: string, message: object): void {
    const json = JSON.stringify(message);
    for (const conn of this.connections.values()) {
      if (conn.roomId === roomId && conn.ws.readyState === WebSocket.OPEN) {
        conn.ws.send(json);
      }
    }
  }

  /**
   * Send audio to specific connection
   */
  sendAudio(handle: string, samples: Int16Array): void {
    const conn = this.connections.get(handle);
    if (conn && conn.ws.readyState === WebSocket.OPEN) {
      conn.ws.send(Buffer.from(samples.buffer));
    }
  }

  /**
   * Get active connection count
   */
  get connectionCount(): number {
    return this.connections.size;
  }
}

// Singleton instance
let voiceServerInstance: VoiceWebSocketServer | null = null;

/**
 * Get or create the voice WebSocket server instance
 */
export function getVoiceWebSocketServer(port?: number): VoiceWebSocketServer {
  if (!voiceServerInstance) {
    voiceServerInstance = new VoiceWebSocketServer(port);
  }
  return voiceServerInstance;
}

/**
 * Start the voice WebSocket server
 */
export async function startVoiceServer(port: number = 3001): Promise<VoiceWebSocketServer> {
  const server = getVoiceWebSocketServer(port);
  await server.start();
  return server;
}
