/**
 * Session Manager Client - Simple interface for session management
 * Follows the same pattern as LoggerClient for consistency
 */

import type { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import type { SessionRequest, SessionResponse, SessionInfo } from '../shared/SessionManagerTypes';
import { ClientSessionDaemon } from './ClientSessionDaemon';

export class SessionManagerClient {
  private static instance: SessionManagerClient;
  private clientSessionDaemon: ClientSessionDaemon | null = null;

  private constructor() {}

  static getInstance(): SessionManagerClient {
    if (!SessionManagerClient.instance) {
      SessionManagerClient.instance = new SessionManagerClient();
    }
    return SessionManagerClient.instance;
  }

  /**
   * Initialize daemon with context
   */
  initialize(context: ContinuumContext): void {
    if (this.clientSessionDaemon) {
      return; // Already initialized
    }
    
    this.clientSessionDaemon = new ClientSessionDaemon(context);
  }

  /**
   * Set WebSocket transport for server communication
   */
  setWebSocketTransport(transport: any): void {
    if (!this.clientSessionDaemon) {
      throw new Error('SessionManagerClient not initialized');
    }
    this.clientSessionDaemon.setWebSocketTransport(transport);
  }

  /**
   * Connect to session
   */
  async connect(request: SessionRequest): Promise<SessionResponse> {
    if (!this.clientSessionDaemon) {
      throw new Error('SessionManagerClient not initialized');
    }
    
    return await this.clientSessionDaemon.connect(request);
  }

  /**
   * Get current session
   */
  getCurrentSession(): SessionInfo | null {
    if (!this.clientSessionDaemon) {
      return null;
    }
    
    return this.clientSessionDaemon.getCurrentSession();
  }

  /**
   * Check if connected to session
   */
  isConnected(): boolean {
    if (!this.clientSessionDaemon) {
      return false;
    }
    
    return this.clientSessionDaemon.isConnected();
  }

  /**
   * Get session by ID
   */
  async getSession(sessionId: string): Promise<SessionInfo | null> {
    if (!this.clientSessionDaemon) {
      return null;
    }
    
    return await this.clientSessionDaemon.getSession(sessionId);
  }

  /**
   * List sessions
   */
  async listSessions(filter: any = {}): Promise<SessionInfo[]> {
    if (!this.clientSessionDaemon) {
      return [];
    }
    
    return await this.clientSessionDaemon.listSessions(filter);
  }

  /**
   * Stop session
   */
  async stopSession(sessionId: string, preserveArtifacts: boolean = true): Promise<boolean> {
    if (!this.clientSessionDaemon) {
      return false;
    }
    
    return await this.clientSessionDaemon.stopSession(sessionId, preserveArtifacts);
  }

  /**
   * Fork session
   */
  async forkSession(fromSessionId: string, config: any): Promise<SessionInfo | null> {
    if (!this.clientSessionDaemon) {
      return null;
    }
    
    return await this.clientSessionDaemon.forkSession(fromSessionId, config);
  }

  /**
   * Update activity
   */
  updateActivity(): void {
    if (this.clientSessionDaemon) {
      this.clientSessionDaemon.updateActivity();
    }
  }

  /**
   * Add event listener
   */
  addEventListener(type: string, listener: (event: any) => void): void {
    if (this.clientSessionDaemon) {
      this.clientSessionDaemon.addEventListener(type as any, listener);
    }
  }

  /**
   * Remove event listener
   */
  removeEventListener(type: string, listener: (event: any) => void): void {
    if (this.clientSessionDaemon) {
      this.clientSessionDaemon.removeEventListener(type as any, listener);
    }
  }

  /**
   * Check if daemon is initialized
   */
  isInitialized(): boolean {
    return this.clientSessionDaemon !== null;
  }

  /**
   * Clean up
   */
  destroy(): void {
    if (this.clientSessionDaemon) {
      this.clientSessionDaemon.cleanup();
      this.clientSessionDaemon = null;
    }
  }
}

// Export singleton instance
export const sessionManagerClient = SessionManagerClient.getInstance();