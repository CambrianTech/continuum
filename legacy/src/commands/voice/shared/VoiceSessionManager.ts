/**
 * Voice Session Manager
 *
 * Manages active voice chat sessions.
 * Sessions are identified by handle (UUID) and track connection state.
 */

import { Events } from '@system/core/shared/Events';

export interface VoiceSessionConfig {
  handle: string;
  roomId: string;
  userId: string;
  model?: string;
  voice?: string;
}

export interface VoiceSession {
  handle: string;
  roomId: string;
  userId: string;
  model: string;
  voice: string;
  startedAt: number;
  isConnected: boolean;
  isListening: boolean;
  isAISpeaking: boolean;
}

/**
 * Voice Session Manager - Singleton
 *
 * Tracks all active voice sessions server-side.
 */
class VoiceSessionManagerImpl {
  private sessions: Map<string, VoiceSession> = new Map();

  /**
   * Create a new voice session
   */
  createSession(config: VoiceSessionConfig): VoiceSession {
    const session: VoiceSession = {
      handle: config.handle,
      roomId: config.roomId,
      userId: config.userId,
      model: config.model || 'llama3.2:3b',
      voice: config.voice || 'default',
      startedAt: Date.now(),
      isConnected: false,
      isListening: false,
      isAISpeaking: false,
    };

    this.sessions.set(config.handle, session);

    // Emit session created event
    Events.emit('voice:session:created', {
      handle: config.handle,
      roomId: config.roomId,
      userId: config.userId,
    });

    return session;
  }

  /**
   * Get session by handle
   */
  getSession(handle: string): VoiceSession | undefined {
    return this.sessions.get(handle);
  }

  /**
   * Get all sessions for a room
   */
  getSessionsInRoom(roomId: string): VoiceSession[] {
    return Array.from(this.sessions.values()).filter(s => s.roomId === roomId);
  }

  /**
   * Get all sessions for a user
   */
  getSessionsForUser(userId: string): VoiceSession[] {
    return Array.from(this.sessions.values()).filter(s => s.userId === userId);
  }

  /**
   * Mark session as connected (WebSocket opened)
   */
  markConnected(handle: string): void {
    const session = this.sessions.get(handle);
    if (session) {
      session.isConnected = true;
      Events.emit('voice:session:connected', { handle });
    }
  }

  /**
   * Mark session as listening (receiving audio)
   */
  setListening(handle: string, isListening: boolean): void {
    const session = this.sessions.get(handle);
    if (session) {
      session.isListening = isListening;
      Events.emit('voice:session:listening', { handle, isListening });
    }
  }

  /**
   * Mark AI speaking state
   */
  setAISpeaking(handle: string, isSpeaking: boolean): void {
    const session = this.sessions.get(handle);
    if (session) {
      session.isAISpeaking = isSpeaking;
      Events.emit('voice:session:ai-speaking', { handle, isSpeaking });
    }
  }

  /**
   * End and remove a session
   */
  endSession(handle: string): { duration: number } | null {
    const session = this.sessions.get(handle);
    if (!session) {
      return null;
    }

    const duration = Math.floor((Date.now() - session.startedAt) / 1000);

    this.sessions.delete(handle);

    // Emit session ended event
    Events.emit('voice:session:ended', {
      handle,
      roomId: session.roomId,
      userId: session.userId,
      duration,
    });

    return { duration };
  }

  /**
   * Get count of active sessions
   */
  get activeCount(): number {
    return this.sessions.size;
  }

  /**
   * Get all active sessions
   */
  getAllSessions(): VoiceSession[] {
    return Array.from(this.sessions.values());
  }
}

// Export singleton instance
export const VoiceSessionManager = new VoiceSessionManagerImpl();
