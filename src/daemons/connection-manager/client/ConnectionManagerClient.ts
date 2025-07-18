/**
 * Connection Manager Client - Thin client for connection lifecycle operations
 * Follows the symmetric daemon pattern with WebSocket transport
 */

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
  createConnectionMessage
} from '../shared/ConnectionMessageTypes';

/**
 * WebSocket-based transport for client-server communication
 */
interface WebSocketTransport {
  send(message: ConnectionMessage): Promise<any>;
  isConnected(): boolean;
}

export class ConnectionManagerClient {
  public readonly context: ContinuumContext;
  private webSocketTransport: WebSocketTransport | null = null;

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
   * Register a new connection
   */
  async registerConnection(request: ConnectionRegisterRequest): Promise<{ connectionInfo: ConnectionInfo }> {
    if (!this.webSocketTransport?.isConnected()) {
      throw new Error('WebSocket transport not available');
    }

    const message = createConnectionMessage('register', request);
    const response = await this.webSocketTransport.send(message.data);
    
    if (!response.success) {
      throw new Error(`Connection registration failed: ${response.error}`);
    }

    return response.data;
  }

  /**
   * Unregister a connection
   */
  async unregisterConnection(request: ConnectionUnregisterRequest): Promise<{ connectionId: string }> {
    if (!this.webSocketTransport?.isConnected()) {
      throw new Error('WebSocket transport not available');
    }

    const message = createConnectionMessage('unregister', request);
    const response = await this.webSocketTransport.send(message.data);
    
    if (!response.success) {
      throw new Error(`Connection unregistration failed: ${response.error}`);
    }

    return response.data;
  }

  /**
   * Map connection to session
   */
  async mapConnectionToSession(request: ConnectionMapSessionRequest): Promise<{ connectionId: string; sessionId: string }> {
    if (!this.webSocketTransport?.isConnected()) {
      throw new Error('WebSocket transport not available');
    }

    const message = createConnectionMessage('map_session', request);
    const response = await this.webSocketTransport.send(message.data);
    
    if (!response.success) {
      throw new Error(`Connection session mapping failed: ${response.error}`);
    }

    return response.data;
  }

  /**
   * Unmap connection from session
   */
  async unmapConnectionFromSession(request: ConnectionUnmapSessionRequest): Promise<{ connectionId: string; sessionId: string }> {
    if (!this.webSocketTransport?.isConnected()) {
      throw new Error('WebSocket transport not available');
    }

    const message = createConnectionMessage('unmap_session', request);
    const response = await this.webSocketTransport.send(message.data);
    
    if (!response.success) {
      throw new Error(`Connection session unmapping failed: ${response.error}`);
    }

    return response.data;
  }

  /**
   * List connections
   */
  async listConnections(request: ConnectionListRequest): Promise<{ connections: ConnectionInfo[] }> {
    if (!this.webSocketTransport?.isConnected()) {
      throw new Error('WebSocket transport not available');
    }

    const message = createConnectionMessage('list', request);
    const response = await this.webSocketTransport.send(message.data);
    
    if (!response.success) {
      throw new Error(`Connection listing failed: ${response.error}`);
    }

    return response.data;
  }

  /**
   * Send message to connection
   */
  async sendToConnection(request: SendToConnectionRequest): Promise<{ connectionId: string; queued: boolean }> {
    if (!this.webSocketTransport?.isConnected()) {
      throw new Error('WebSocket transport not available');
    }

    const message = createConnectionMessage('send_to_connection', request);
    const response = await this.webSocketTransport.send(message.data);
    
    if (!response.success) {
      throw new Error(`Send to connection failed: ${response.error}`);
    }

    return response.data;
  }

  /**
   * Check connection status
   */
  async checkConnection(request: CheckConnectionRequest): Promise<{ exists: boolean; isActive: boolean; sessionId?: string; lastActivity: Date }> {
    if (!this.webSocketTransport?.isConnected()) {
      throw new Error('WebSocket transport not available');
    }

    const message = createConnectionMessage('check_connection', request);
    const response = await this.webSocketTransport.send(message.data);
    
    if (!response.success) {
      throw new Error(`Connection check failed: ${response.error}`);
    }

    return response.data;
  }
}