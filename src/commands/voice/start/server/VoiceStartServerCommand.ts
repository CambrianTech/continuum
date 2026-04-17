/**
 * Voice Start Command - Server Implementation
 *
 * Starts a voice chat session using LiveKit WebRTC.
 * Returns a LiveKit JWT token + URL for the browser to connect.
 *
 * Migration: previously spun up a legacy WebSocket server on port 3001.
 * Now uses the same LiveKit infrastructure as collaboration/live/join.
 * Port 3001 is no longer needed.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { VoiceStartParams, VoiceStartResult } from '../shared/VoiceStartTypes';
import { createVoiceStartResultFromParams } from '../shared/VoiceStartTypes';
import { VoiceSessionManager } from '../../shared/VoiceSessionManager';
import { resolveRoomIdentifier } from '@system/routing/RoutingService';
import { getSecret } from '@system/secrets/SecretManager';
import { v4 as uuidv4 } from 'uuid';

// LiveKit dev-mode defaults (same as collaboration/live/join)
const LIVEKIT_API_KEY = 'devkey';
const LIVEKIT_API_SECRET = 'secret';

export class VoiceStartServerCommand extends CommandBase<VoiceStartParams, VoiceStartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/start', context, subpath, commander);
  }

  async execute(params: VoiceStartParams): Promise<VoiceStartResult> {
    console.log('🎤 SERVER: Starting voice session via LiveKit', params);

    // Resolve room
    const roomName = params.room || 'general';
    let roomId: string;

    const resolved = await resolveRoomIdentifier(roomName);
    if (resolved) {
      roomId = resolved.id;
    } else {
      roomId = 'general';
      console.warn(`Failed to resolve room "${roomName}", using default`);
    }

    // Generate session handle
    const handle = uuidv4();

    // Create voice session (tracks active sessions for cleanup)
    VoiceSessionManager.createSession({
      handle,
      roomId,
      userId: params.sessionId || 'anonymous',
      model: params.model,
      voice: params.voice,
    });

    // Generate LiveKit JWT token
    const livekitToken = await this.generateLiveKitToken(
      roomId,
      params.sessionId || 'anonymous',
      'Voice User'
    );

    // LiveKit URL for browser connection
    const livekitUrl = getSecret('LIVEKIT_URL') || 'ws://localhost:7880';

    console.log(`🎤 Voice session started: ${handle.substring(0, 8)}... in room ${roomId}`);
    console.log(`🎤 LiveKit URL: ${livekitUrl}`);

    return createVoiceStartResultFromParams(params, {
      success: true,
      handle,
      livekitUrl,
      livekitToken,
      wsUrl: livekitUrl, // backwards compat
      roomId,
    });
  }

  /**
   * Generate a LiveKit JWT access token for a voice participant.
   * Same pattern as LiveJoinServerCommand.generateLiveKitToken.
   */
  private async generateLiveKitToken(
    roomId: string,
    userId: string,
    displayName: string
  ): Promise<string> {
    const { AccessToken } = await import('livekit-server-sdk');

    const apiKey = getSecret('LIVEKIT_API_KEY') || LIVEKIT_API_KEY;
    const apiSecret = getSecret('LIVEKIT_API_SECRET') || LIVEKIT_API_SECRET;
    const token = new AccessToken(apiKey, apiSecret, {
      identity: userId,
      name: displayName,
      metadata: JSON.stringify({ role: 'human' }),
      ttl: '6h',
    });
    token.addGrant({
      room: roomId,
      roomJoin: true,
      canPublish: true,
      canSubscribe: true,
      canPublishData: true,
    });

    return await token.toJwt();
  }
}
