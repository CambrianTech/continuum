/**
 * WebSocket Manager - Pure WebSocket connection handling
 * No business logic, just connection management
 */

import { WebSocketServer, WebSocket } from 'ws';
import { Server as HttpServer } from 'http';

export interface WebSocketConnection {
  id: string;
  ws: WebSocket;
  metadata: Record<string, any>;
  connectedAt: Date;
}

export class WebSocketManager {
  private connections = new Map<string, WebSocketConnection>();
  private server: WebSocketServer | null = null;
  
  // Event callbacks
  public onMessage?: (connectionId: string, data: any) => void;
  public onConnection?: (connectionId: string, connection: WebSocketConnection) => void;
  public onDisconnection?: (connectionId: string) => void;

  async start(httpServer: HttpServer): Promise<void> {
    this.server = new WebSocketServer({ server: httpServer });
    
    this.server.on('connection', (ws, req) => {
      const connectionId = this.generateConnectionId();
      const userAgent = req.headers['user-agent'] || '';
      
      // ONE TAB POLICY: If this is a browser connection and we already have browser connections, close the old ones
      if (userAgent.includes('Mozilla') || userAgent.includes('Chrome') || userAgent.includes('Safari')) {
        const existingBrowserConnections = Array.from(this.connections.entries())
          .filter(([_, conn]) => {
            const ua = conn.metadata.userAgent || '';
            return ua.includes('Mozilla') || ua.includes('Chrome') || ua.includes('Safari');
          });
        
        if (existingBrowserConnections.length > 0) {
          console.log(`⚠️ ONE TAB POLICY: Closing ${existingBrowserConnections.length} existing browser tabs`);
          
          // Close all existing browser connections
          for (const [oldId, oldConn] of existingBrowserConnections) {
            console.log(`❌ Closing duplicate tab: ${oldId}`);
            
            // Send message to close the tab
            if (oldConn.ws.readyState === WebSocket.OPEN) {
              oldConn.ws.send(JSON.stringify({
                type: 'system_command',
                command: 'close_tab',
                reason: 'ONE_TAB_POLICY',
                message: 'New tab connected - closing this duplicate tab'
              }));
              
              // Close the WebSocket
              setTimeout(() => oldConn.ws.close(), 100);
            }
          }
        }
      }
      
      const connection: WebSocketConnection = {
        id: connectionId,
        ws,
        metadata: {
          userAgent: req.headers['user-agent'],
          remoteAddress: req.socket.remoteAddress,
          url: req.url
        },
        connectedAt: new Date()
      };

      this.connections.set(connectionId, connection);
      console.log(`🔌 WebSocket connected: ${connectionId} (${this.connections.size} total)`);

      // Notify connection callback
      if (this.onConnection) {
        this.onConnection(connectionId, connection);
      }

      ws.on('message', (data) => {
        this.handleMessage(connectionId, data);
      });

      ws.on('close', () => {
        this.connections.delete(connectionId);
        console.log(`🔌 WebSocket disconnected: ${connectionId} (${this.connections.size} remaining)`);
        
        // Notify disconnection callback
        if (this.onDisconnection) {
          this.onDisconnection(connectionId);
        }
      });

      ws.on('error', (error) => {
        console.error(`🔌 WebSocket error for ${connectionId}:`, error);
        this.connections.delete(connectionId);
      });

      // Send connection confirmation
      this.sendToConnection(connectionId, {
        type: 'connection_confirmed',
        data: { clientId: connectionId }
      });
    });

    console.log(`🌐 WebSocket server attached to HTTP server`);
  }

  async stop(): Promise<void> {
    if (this.server) {
      this.server.close();
      this.connections.clear();
      console.log('🛑 WebSocket server stopped');
    }
  }

  sendToConnection(connectionId: string, message: any): boolean {
    const connection = this.connections.get(connectionId);
    if (connection && connection.ws.readyState === WebSocket.OPEN) {
      connection.ws.send(JSON.stringify(message));
      return true;
    }
    return false;
  }

  broadcast(message: any, exclude?: string): void {
    for (const [id, connection] of this.connections) {
      if (id !== exclude && connection.ws.readyState === WebSocket.OPEN) {
        connection.ws.send(JSON.stringify(message));
      }
    }
  }

  getConnections(): WebSocketConnection[] {
    return Array.from(this.connections.values());
  }

  getConnectionCount(): number {
    return this.connections.size;
  }
  
  getAllConnectionIds(): string[] {
    return Array.from(this.connections.keys());
  }
  
  getConnection(connectionId: string): WebSocketConnection | undefined {
    return this.connections.get(connectionId);
  }
  
  closeConnection(connectionId: string): void {
    const connection = this.connections.get(connectionId);
    if (connection && connection.ws.readyState === WebSocket.OPEN) {
      connection.ws.close();
    }
    this.connections.delete(connectionId);
  }

  private generateConnectionId(): string {
    return `ws_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  private handleMessage(connectionId: string, data: any): void {
    // Emit event for daemon to handle - pure separation
    this.onMessage?.(connectionId, data);
  }

  // Event handler set by daemon is already defined at class level
}