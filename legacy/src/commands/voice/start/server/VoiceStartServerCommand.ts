/**
 * Voice Start Command - Server Implementation
 *
 * Start voice chat session for real-time audio communication with AI
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { VoiceStartParams, VoiceStartResult } from '../shared/VoiceStartTypes';
import { createVoiceStartResultFromParams } from '../shared/VoiceStartTypes';
import { VoiceSessionManager } from '../../shared/VoiceSessionManager';
import { resolveRoomIdentifier } from '@system/routing/RoutingService';
import { getVoiceWebSocketServer } from '@system/voice/server';
import { v4 as uuidv4 } from 'uuid';

// Voice WebSocket server port
const VOICE_WS_PORT = 3001;

export class VoiceStartServerCommand extends CommandBase<VoiceStartParams, VoiceStartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/start', context, subpath, commander);
  }

  async execute(params: VoiceStartParams): Promise<VoiceStartResult> {
    console.log('🎤 SERVER: Starting voice session', params);

    // Ensure voice WebSocket server is running
    const voiceServer = getVoiceWebSocketServer(VOICE_WS_PORT);
    if (voiceServer.connectionCount === 0) {
      // Server might not be started yet - start it
      try {
        await voiceServer.start();
      } catch (error) {
        // Server might already be running, that's OK
        if (!(error instanceof Error) || !error.message.includes('EADDRINUSE')) {
          console.warn('Voice server start warning:', error);
        }
      }
    }

    // Resolve room
    const roomName = params.room || 'general';
    let roomId: string;

    const resolved = await resolveRoomIdentifier(roomName);
    if (resolved) {
      roomId = resolved.id;
    } else {
      // Default to general room if resolution fails
      roomId = 'general';
      console.warn(`Failed to resolve room "${roomName}", using default`);
    }

    // Generate session handle
    const handle = uuidv4();

    // Create voice session
    const session = VoiceSessionManager.createSession({
      handle,
      roomId,
      userId: params.sessionId || 'anonymous',
      model: params.model,
      voice: params.voice,
    });

    // Build WebSocket URL
    const wsProtocol = 'ws:'; // Use wss: in production
    const wsHost = `localhost:${VOICE_WS_PORT}`;
    const wsUrl = `${wsProtocol}//${wsHost}?handle=${handle}&room=${roomId}`;

    console.log(`🎤 Voice session started: ${handle.substring(0, 8)}... in room ${roomId}`);
    console.log(`🎤 Connect to: ${wsUrl}`);

    return createVoiceStartResultFromParams(params, {
      success: true,
      handle,
      wsUrl,
      roomId,
    });
  }
}
