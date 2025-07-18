/**
 * Connection Manager Daemon - Pure WebSocket connection lifecycle management
 * Handles connection-to-session mapping without business logic
 */

import { ProcessBasedDaemon } from '../../base/ProcessBasedDaemon';
import { DaemonResponse, DaemonMessage } from '../../base/DaemonProtocol';
import { DaemonType } from '../../base/DaemonTypes';
import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import { 
  ConnectionMessage, 
  ConnectionRegisterRequest,
  ConnectionUnregisterRequest,
  ConnectionMapSessionRequest,
  ConnectionUnmapSessionRequest,
  ConnectionListRequest,
  SendToConnectionRequest,
  CheckConnectionRequest,
  ConnectionInfo,
  ConnectionEvent
} from '../shared/ConnectionMessageTypes';
import { DAEMON_EVENT_BUS } from '../../base/DaemonEventBus';
import { SystemEventType } from '../../base/EventTypes';

export class ConnectionManagerDaemon extends ProcessBasedDaemon<ConnectionMessage> {
  readonly name = 'connection-manager';
  readonly version = '1.0.0';
  readonly daemonType: DaemonType = 'connection-manager' as DaemonType;

  private connections = new Map<string, ConnectionInfo>();
  private sessionConnections = new Map<string, Set<string>>(); // sessionId -> connectionIds
  private connectionSessions = new Map<string, string>(); // connectionId -> sessionId
  private eventListeners = new Set<(event: ConnectionEvent) => void>();

  constructor(context?: ContinuumContext) {
    super(context, {
      queueSize: 5000,
      batchSize: 20,
      processTimeoutMs: 5000
    });
  }

  protected async processMessage(message: DaemonMessage<ConnectionMessage>): Promise<DaemonResponse> {
    try {
      switch (message.data.type) {
        case 'register':
          return await this.handleRegisterConnection(message.data.payload as ConnectionRegisterRequest);
        
        case 'unregister':
          return await this.handleUnregisterConnection(message.data.payload as ConnectionUnregisterRequest);
        
        case 'map_session':
          return await this.handleMapSession(message.data.payload as ConnectionMapSessionRequest);
        
        case 'unmap_session':
          return await this.handleUnmapSession(message.data.payload as ConnectionUnmapSessionRequest);
        
        case 'list':
          return await this.handleListConnections(message.data.payload as ConnectionListRequest);
        
        case 'send_to_connection':
          return await this.handleSendToConnection(message.data.payload as SendToConnectionRequest);
        
        case 'check_connection':
          return await this.handleCheckConnection(message.data.payload as CheckConnectionRequest);
        
        default:
          return { success: false, error: `Unknown connection message type: ${message.data.type}` };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      this.log(`Error processing connection message: ${errorMessage}`, 'error');
      return { success: false, error: errorMessage };
    }
  }

  private async handleRegisterConnection(request: ConnectionRegisterRequest): Promise<DaemonResponse> {
    const connectionInfo: ConnectionInfo = {
      id: request.connectionId,
      metadata: request.metadata || {
        timestamp: new Date()
      },
      isActive: true,
      lastActivity: new Date()
    };

    this.connections.set(request.connectionId, connectionInfo);
    this.log(`Connection registered: ${request.connectionId}`);

    // Emit connection event
    this.emitConnectionEvent({
      type: 'connected',
      connectionId: request.connectionId,
      timestamp: new Date()
    });

    return { success: true, data: { connectionInfo } };
  }

  private async handleUnregisterConnection(request: ConnectionUnregisterRequest): Promise<DaemonResponse> {
    const connectionInfo = this.connections.get(request.connectionId);
    
    if (!connectionInfo) {
      return { success: false, error: `Connection not found: ${request.connectionId}` };
    }

    // Remove from session mapping if mapped
    const sessionId = connectionInfo.sessionId;
    if (sessionId) {
      this.unmapConnectionFromSession(request.connectionId, sessionId);
    }

    connectionInfo.isActive = false;
    this.connections.delete(request.connectionId);
    this.log(`Connection unregistered: ${request.connectionId}`);

    // Emit disconnection event  
    this.emitConnectionEvent({
      type: 'disconnected',
      connectionId: request.connectionId,
      sessionId: sessionId,
      timestamp: new Date(),
      metadata: { reason: request.reason }
    });

    return { success: true, data: { connectionId: request.connectionId } };
  }

  private async handleMapSession(request: ConnectionMapSessionRequest): Promise<DaemonResponse> {
    const connectionInfo = this.connections.get(request.connectionId);
    
    if (!connectionInfo) {
      return { success: false, error: `Connection not found: ${request.connectionId}` };
    }

    // Remove from previous session if mapped
    if (connectionInfo.sessionId) {
      this.unmapConnectionFromSession(request.connectionId, connectionInfo.sessionId);
    }

    // Map to new session
    connectionInfo.sessionId = request.sessionId;
    connectionInfo.lastActivity = new Date();
    
    this.connectionSessions.set(request.connectionId, request.sessionId);
    
    if (!this.sessionConnections.has(request.sessionId)) {
      this.sessionConnections.set(request.sessionId, new Set());
    }
    this.sessionConnections.get(request.sessionId)!.add(request.connectionId);

    this.log(`Connection mapped: ${request.connectionId} -> ${request.sessionId}`);

    // Emit session mapping event
    this.emitConnectionEvent({
      type: 'session_mapped',
      connectionId: request.connectionId,
      sessionId: request.sessionId,
      timestamp: new Date()
    });

    return { success: true, data: { connectionId: request.connectionId, sessionId: request.sessionId } };
  }

  private async handleUnmapSession(request: ConnectionUnmapSessionRequest): Promise<DaemonResponse> {
    const connectionInfo = this.connections.get(request.connectionId);
    
    if (!connectionInfo) {
      return { success: false, error: `Connection not found: ${request.connectionId}` };
    }

    const sessionId = connectionInfo.sessionId;
    if (!sessionId) {
      return { success: false, error: `Connection not mapped to any session: ${request.connectionId}` };
    }

    this.unmapConnectionFromSession(request.connectionId, sessionId);
    connectionInfo.sessionId = undefined;
    connectionInfo.lastActivity = new Date();

    this.log(`Connection unmapped: ${request.connectionId} from ${sessionId}`);

    // Emit session unmapping event
    this.emitConnectionEvent({
      type: 'session_unmapped',
      connectionId: request.connectionId,
      sessionId,
      timestamp: new Date()
    });

    return { success: true, data: { connectionId: request.connectionId, sessionId } };
  }

  private async handleListConnections(request: ConnectionListRequest): Promise<DaemonResponse> {
    let filteredConnections: ConnectionInfo[] = [];

    if (request.sessionId) {
      const connectionIds = this.sessionConnections.get(request.sessionId) || new Set();
      filteredConnections = Array.from(connectionIds)
        .map(id => this.connections.get(id)!)
        .filter(conn => conn !== undefined);
    } else {
      filteredConnections = Array.from(this.connections.values());
    }

    if (request.activeOnly) {
      filteredConnections = filteredConnections.filter(conn => conn.isActive);
    }

    return { success: true, data: { connections: filteredConnections } };
  }

  private async handleSendToConnection(request: SendToConnectionRequest): Promise<DaemonResponse> {
    const connectionInfo = this.connections.get(request.connectionId);
    
    if (!connectionInfo) {
      return { success: false, error: `Connection not found: ${request.connectionId}` };
    }

    if (!connectionInfo.isActive) {
      return { success: false, error: `Connection not active: ${request.connectionId}` };
    }

    // Update last activity
    connectionInfo.lastActivity = new Date();

    // For now, this is just a placeholder - actual WebSocket sending would be delegated to WebSocketManager
    this.log(`Message queued for connection: ${request.connectionId}`);

    return { success: true, data: { connectionId: request.connectionId, queued: true } };
  }

  private async handleCheckConnection(request: CheckConnectionRequest): Promise<DaemonResponse> {
    const connectionInfo = this.connections.get(request.connectionId);
    
    if (!connectionInfo) {
      return { success: false, error: `Connection not found: ${request.connectionId}` };
    }

    return { 
      success: true, 
      data: { 
        exists: true, 
        isActive: connectionInfo.isActive,
        sessionId: connectionInfo.sessionId,
        lastActivity: connectionInfo.lastActivity
      } 
    };
  }

  private unmapConnectionFromSession(connectionId: string, sessionId: string): void {
    this.connectionSessions.delete(connectionId);
    
    const sessionConnections = this.sessionConnections.get(sessionId);
    if (sessionConnections) {
      sessionConnections.delete(connectionId);
      
      // Clean up empty session connection sets
      if (sessionConnections.size === 0) {
        this.sessionConnections.delete(sessionId);
      }
    }
  }

  private emitConnectionEvent(event: ConnectionEvent): void {
    // Emit to event bus with simplified payload
    const eventType = event.type === 'connected' 
      ? SystemEventType.WEBSOCKET_CONNECTION_ESTABLISHED
      : SystemEventType.WEBSOCKET_CONNECTION_CLOSED;
    
    DAEMON_EVENT_BUS.emitEvent(eventType, {
      connectionId: event.connectionId,
      timestamp: event.timestamp,
      source: 'connection-manager'
    });
    
    // Emit to local listeners
    this.eventListeners.forEach(listener => {
      try {
        listener(event);
      } catch (error) {
        this.log(`Error in connection event listener: ${error}`, 'error');
      }
    });
  }

  // Public API for other daemons
  getConnection(connectionId: string): ConnectionInfo | undefined {
    return this.connections.get(connectionId);
  }

  getConnectionsForSession(sessionId: string): ConnectionInfo[] {
    const connectionIds = this.sessionConnections.get(sessionId) || new Set();
    return Array.from(connectionIds)
      .map(id => this.connections.get(id)!)
      .filter(conn => conn !== undefined);
  }

  getSessionForConnection(connectionId: string): string | undefined {
    return this.connectionSessions.get(connectionId);
  }

  addConnectionEventListener(listener: (event: ConnectionEvent) => void): void {
    this.eventListeners.add(listener);
  }

  removeConnectionEventListener(listener: (event: ConnectionEvent) => void): void {
    this.eventListeners.delete(listener);
  }

  // Statistics for monitoring
  getStats(): { totalConnections: number; activeSessions: number; activeConnections: number } {
    const activeSessions = this.sessionConnections.size;
    const activeConnections = Array.from(this.connections.values())
      .filter(conn => conn.isActive).length;
    
    return {
      totalConnections: this.connections.size,
      activeSessions,
      activeConnections
    };
  }
}