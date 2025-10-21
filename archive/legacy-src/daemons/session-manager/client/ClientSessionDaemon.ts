/**
 * Client Session Daemon - Browser-side session management
 * Symmetric architecture with server daemon, lightweight client-side session tracking
 */

import { ContinuumContext, UUID } from '../../../types/shared/core/ContinuumTypes';
import { 
  SessionInfo, 
  SessionRequest, 
  SessionResponse,
  SessionEventType,
  SessionEvents
} from '../shared/SessionManagerTypes';

/**
 * WebSocket transport for client-server communication
 */
interface WebSocketTransport {
  send(message: any): Promise<void>;
  isConnected(): boolean;
}

export class ClientSessionDaemon {
  public readonly context: ContinuumContext;
  private webSocketTransport: WebSocketTransport | null = null;
  private currentSession: SessionInfo | null = null;
  private eventListeners = new Map<SessionEventType, Set<(event: any) => void>>();

  constructor(context: ContinuumContext) {
    this.context = context;
  }

  /**
   * Set WebSocket transport for server communication
   */
  setWebSocketTransport(transport: WebSocketTransport): void {
    this.webSocketTransport = transport;
  }

  /**
   * Connect to session - delegates to server daemon
   */
  async connect(request: SessionRequest): Promise<SessionResponse> {
    if (!this.webSocketTransport || !this.webSocketTransport.isConnected()) {
      throw new Error('WebSocket transport not available');
    }

    try {
      const response = await this.sendToServer('session.connect', request);
      
      if (response.success) {
        // Store current session info locally
        this.currentSession = {
          id: response.data.sessionId,
          type: request.type,
          owner: request.owner,
          created: new Date(),
          lastActive: new Date(),
          isActive: true,
          paths: response.data.paths,
          processes: {},
          autoCleanup: true,
          cleanupAfterMs: 2 * 60 * 60 * 1000
        };

        // Emit session event
        this.emitEvent(SessionEvents.joined(response.data.sessionId, { source: request.source }));
        
        return response.data;
      } else {
        throw new Error(response.error || 'Failed to connect to session');
      }
    } catch (error) {
      throw new Error(`Session connection failed: ${error}`);
    }
  }

  /**
   * Get current session info
   */
  getCurrentSession(): SessionInfo | null {
    return this.currentSession;
  }

  /**
   * Check if currently connected to a session
   */
  isConnected(): boolean {
    return this.currentSession !== null && this.currentSession.isActive;
  }

  /**
   * Get session by ID - delegates to server
   */
  async getSession(sessionId: UUID): Promise<SessionInfo | null> {
    if (!this.webSocketTransport || !this.webSocketTransport.isConnected()) {
      return null;
    }

    try {
      const response = await this.sendToServer('session.get', { sessionId });
      return response.success ? response.data.session : null;
    } catch (error) {
      console.error('Failed to get session:', error);
      return null;
    }
  }

  /**
   * List sessions - delegates to server
   */
  async listSessions(filter: any = {}): Promise<SessionInfo[]> {
    if (!this.webSocketTransport || !this.webSocketTransport.isConnected()) {
      return [];
    }

    try {
      const response = await this.sendToServer('session.list', { filter });
      return response.success ? response.data.sessions : [];
    } catch (error) {
      console.error('Failed to list sessions:', error);
      return [];
    }
  }

  /**
   * Stop session - delegates to server
   */
  async stopSession(sessionId: UUID, preserveArtifacts: boolean = true): Promise<boolean> {
    if (!this.webSocketTransport || !this.webSocketTransport.isConnected()) {
      return false;
    }

    try {
      const response = await this.sendToServer('session.stop', { sessionId, preserveArtifacts });
      
      if (response.success && this.currentSession?.id === sessionId) {
        this.currentSession.isActive = false;
        this.emitEvent(SessionEvents.stopped(sessionId, { preserveArtifacts }));
      }
      
      return response.success;
    } catch (error) {
      console.error('Failed to stop session:', error);
      return false;
    }
  }

  /**
   * Fork session - delegates to server
   */
  async forkSession(fromSessionId: UUID, config: any): Promise<SessionInfo | null> {
    if (!this.webSocketTransport || !this.webSocketTransport.isConnected()) {
      return null;
    }

    try {
      const response = await this.sendToServer('session.fork', { fromSessionId, config });
      
      if (response.success) {
        this.emitEvent(SessionEvents.forked(response.data.session.id, fromSessionId));
        return response.data.session;
      }
      
      return null;
    } catch (error) {
      console.error('Failed to fork session:', error);
      return null;
    }
  }

  /**
   * Update session activity
   */
  updateActivity(): void {
    if (this.currentSession) {
      this.currentSession.lastActive = new Date();
    }
  }

  /**
   * Add event listener
   */
  addEventListener(type: SessionEventType, listener: (event: any) => void): void {
    if (!this.eventListeners.has(type)) {
      this.eventListeners.set(type, new Set());
    }
    this.eventListeners.get(type)!.add(listener);
  }

  /**
   * Remove event listener
   */
  removeEventListener(type: SessionEventType, listener: (event: any) => void): void {
    const listeners = this.eventListeners.get(type);
    if (listeners) {
      listeners.delete(listener);
    }
  }

  /**
   * Send message to server daemon
   */
  private async sendToServer(type: string, payload: any): Promise<any> {
    if (!this.webSocketTransport || !this.webSocketTransport.isConnected()) {
      throw new Error('WebSocket transport not available');
    }

    return new Promise((_resolve, reject) => {
      const messageId = crypto.randomUUID();
      const message = {
        type,
        payload,
        messageId,
        timestamp: Date.now()
      };

      // TODO: Set up proper response handling with WebSocket integration

      // Send message
      this.webSocketTransport!.send(message)
        .then(() => {
          // TODO: Set up proper response handling with timeout
          // For now, we'll need to integrate with the WebSocket response system
          setTimeout(() => {
            reject(new Error('Request timeout'));
          }, 5000);
        })
        .catch(reject);
    });
  }

  /**
   * Emit event to listeners
   */
  private emitEvent(event: ReturnType<typeof SessionEvents.joined>): void {
    const listeners = this.eventListeners.get(event.type);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(event);
        } catch (error) {
          console.error('Session event listener error:', error);
        }
      });
    }
  }

  /**
   * Clean up
   */
  cleanup(): void {
    if (this.currentSession) {
      this.currentSession.isActive = false;
    }
    this.eventListeners.clear();
    this.webSocketTransport = null;
  }
}