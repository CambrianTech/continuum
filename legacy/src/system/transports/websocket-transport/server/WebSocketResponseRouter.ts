/**
 * WebSocket Response Router - Client-specific response routing for WebSocket clients
 * 
 * Solves the critical infrastructure bug: Commands execute but responses aren't sent back to WebSocket clients.
 * Maps correlation IDs to specific WebSocket connections for targeted response delivery.
 */

import type { JTAGMessage, JTAGResponseMessage } from '../../../core/types/JTAGTypes';
import type { WebSocket as WSWebSocket } from 'ws';
import { JTAGMessageTypes } from '../../../core/types/JTAGTypes';

/** Internal socket properties exposed by ws library but not in public types */
interface WSWebSocketInternal {
  _socket?: { remoteAddress?: string; remotePort?: number };
}

// Verbose logging helper for server
const verbose = () => process.env.JTAG_VERBOSE === '1';

export interface WebSocketClientConnection {
  socket: WSWebSocket;
  clientId: string;
  connectedAt: number;
  lastActivity: number;
}

export class WebSocketResponseRouter {
  private correlationToClient = new Map<string, WebSocketClientConnection>();
  private clientIdToConnection = new Map<string, WebSocketClientConnection>();

  /**
   * Register a new WebSocket client connection
   */
  registerClient(socket: WSWebSocket, clientId: string): void {
    const connection: WebSocketClientConnection = {
      socket,
      clientId,
      connectedAt: Date.now(),
      lastActivity: Date.now()
    };

    this.clientIdToConnection.set(clientId, connection);
  }

  /**
   * Associate a correlation ID with a specific client
   * Called when a WebSocket client sends a request
   */
  registerCorrelation(correlationId: string, clientId: string): void {
    const connection = this.clientIdToConnection.get(clientId);
    if (connection) {
      this.correlationToClient.set(correlationId, connection);
      connection.lastActivity = Date.now();
      // console.log(`🔗 WebSocketResponseRouter: Mapped ${correlationId} → ${clientId}`);
    } else {
      console.warn(`⚠️ WebSocketResponseRouter: Client ${clientId} not found for correlation ${correlationId}`);
    }
  }

  /**
   * Send response back to the specific client that made the request
   */
  async sendResponse(message: JTAGMessage): Promise<boolean> {
    if (!JTAGMessageTypes.isResponse(message)) {
      console.warn(`⚠️ WebSocketResponseRouter: Message is not a response:`, message.messageType);
      return false;
    }

    const responseMessage = message as JTAGResponseMessage;
    const correlationId = responseMessage.correlationId;
    if (!correlationId) {
      console.warn(`⚠️ WebSocketResponseRouter: Response has no correlation ID`);
      return false;
    }

    const connection = this.correlationToClient.get(correlationId);
    if (!connection) {
      console.warn(`⚠️ WebSocketResponseRouter: No client found for correlation ${correlationId}`);
      return false;
    }

    try {
      if (connection.socket.readyState === 1) { // WebSocket.OPEN
        const messageData = JSON.stringify(message);
        connection.socket.send(messageData);
        connection.lastActivity = Date.now();

        // console.log(`📤 WebSocketResponseRouter: Sent response ${correlationId} → ${connection.clientId}`);

        // Clean up correlation after successful delivery
        this.correlationToClient.delete(correlationId);
        return true;
      } else {
        console.warn(`⚠️ WebSocketResponseRouter: Client ${connection.clientId} socket not open (state: ${connection.socket.readyState})`);
        this.cleanupClient(connection.clientId);
        return false;
      }
    } catch (error) {
      console.error(`❌ WebSocketResponseRouter: Failed to send response:`, error);
      this.cleanupClient(connection.clientId);
      return false;
    }
  }

  /**
   * Clean up when a client disconnects
   */
  unregisterClient(clientId: string): void {
    const connection = this.clientIdToConnection.get(clientId);
    if (connection) {
      // Clean up all correlations for this client
      for (const [correlationId, clientConnection] of this.correlationToClient.entries()) {
        if (clientConnection.clientId === clientId) {
          this.correlationToClient.delete(correlationId);
        }
      }
      
      this.clientIdToConnection.delete(clientId);
      verbose() && console.log(`🔌 WebSocketResponseRouter: Unregistered client ${clientId}`);
    }
  }

  /**
   * Clean up a specific client (called when socket becomes unusable)
   */
  private cleanupClient(clientId: string): void {
    this.unregisterClient(clientId);
  }

  /**
   * Check if a correlation ID has an associated WebSocket client
   */
  hasCorrelation(correlationId: string): boolean {
    return this.correlationToClient.has(correlationId);
  }

  /**
   * Get statistics for debugging
   */
  getStats() {
    return {
      connectedClients: this.clientIdToConnection.size,
      pendingCorrelations: this.correlationToClient.size,
      clients: Array.from(this.clientIdToConnection.keys()),
      correlations: Array.from(this.correlationToClient.keys())
    };
  }

  /**
   * Extract client ID from a WebSocket client for correlation
   * Uses socket properties to generate a consistent client ID
   */
  static generateClientId(socket: WSWebSocket): string {
    const internal = socket as unknown as WSWebSocketInternal;
    const remoteAddress = internal._socket?.remoteAddress || 'unknown';
    const remotePort = internal._socket?.remotePort || 'unknown';
    return `ws-${remoteAddress}-${remotePort}-${Date.now()}`;
  }
}