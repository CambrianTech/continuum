/**
 * Connection Manager - Modular client connection management
 */

import { EventEmitter } from 'events';
import { WebSocket } from 'ws';
import { Client, ConnectionConfig, ConnectionStats } from '../types';

export class ConnectionManager extends EventEmitter {
  private clients = new Map<string, Client>();
  private heartbeatTimer: NodeJS.Timeout | null = null;
  private readonly config: Required<ConnectionConfig>;

  constructor(config: ConnectionConfig = {}) {
    super();
    this.config = {
      maxClients: config.maxClients ?? 100,
      heartbeatInterval: config.heartbeatInterval ?? 30000,
      clientTimeout: config.clientTimeout ?? 60000,
      enableHeartbeat: config.enableHeartbeat ?? true,
      enableAuth: config.enableAuth ?? false,
      authTimeout: config.authTimeout ?? 10000
    };
  }

  addClient(socket: WebSocket, metadata: Record<string, any> = {}): string {
    if (this.clients.size >= this.config.maxClients) {
      throw new Error(`Maximum client limit reached: ${this.config.maxClients}`);
    }

    const clientId = this.generateClientId();
    const now = new Date();
    
    const client: Client = {
      id: clientId,
      socket,
      connected: true,
      connectTime: now,
      lastActivity: now,
      metadata: { ...metadata }
    };

    this.clients.set(clientId, client);
    this.setupClientHandlers(client);
    
    this.emit('client:connected', client);
    console.log(`📱 Client connected: ${clientId} (${this.clients.size}/${this.config.maxClients})`);
    
    if (this.config.enableHeartbeat && !this.heartbeatTimer) {
      this.startHeartbeat();
    }
    
    return clientId;
  }

  removeClient(clientId: string): boolean {
    const client = this.clients.get(clientId);
    if (!client) {
      return false;
    }

    client.connected = false;
    this.clients.delete(clientId);
    
    this.emit('client:disconnected', client);
    console.log(`📱 Client disconnected: ${clientId} (${this.clients.size}/${this.config.maxClients})`);
    
    if (this.clients.size === 0 && this.heartbeatTimer) {
      this.stopHeartbeat();
    }
    
    return true;
  }

  getClient(clientId: string): Client | undefined {
    return this.clients.get(clientId);
  }

  getAllClients(): Client[] {
    return Array.from(this.clients.values());
  }

  getConnectedCount(): number {
    return this.clients.size;
  }

  sendToClient(clientId: string, message: any): boolean {
    const client = this.clients.get(clientId);
    if (!client || !client.connected || client.socket.readyState !== WebSocket.OPEN) {
      return false;
    }

    try {
      client.socket.send(JSON.stringify(message));
      client.lastActivity = new Date();
      return true;
    } catch (error) {
      console.error(`❌ Failed to send to client ${clientId}:`, error);
      this.removeClient(clientId);
      return false;
    }
  }

  broadcast(message: any, excludeClientId?: string): number {
    let sentCount = 0;
    const messageStr = JSON.stringify(message);
    
    for (const [clientId, client] of this.clients) {
      if (excludeClientId && clientId === excludeClientId) {
        continue;
      }
      
      if (client.connected && client.socket.readyState === WebSocket.OPEN) {
        try {
          client.socket.send(messageStr);
          client.lastActivity = new Date();
          sentCount++;
        } catch (error) {
          console.error(`❌ Broadcast failed to client ${clientId}:`, error);
          this.removeClient(clientId);
        }
      }
    }
    
    return sentCount;
  }

  updateClientMetadata(clientId: string, metadata: Record<string, any>): boolean {
    const client = this.clients.get(clientId);
    if (!client) {
      return false;
    }
    
    client.metadata = { ...client.metadata, ...metadata };
    client.lastActivity = new Date();
    return true;
  }

  getStats(): ConnectionStats {
    const clients = this.getAllClients();
    const now = new Date();
    
    return {
      totalClients: clients.length,
      maxClients: this.config.maxClients,
      averageConnectionTime: clients.length > 0 
        ? clients.reduce((sum, client) => sum + (now.getTime() - client.connectTime.getTime()), 0) / clients.length
        : 0,
      oldestConnection: clients.length > 0 
        ? Math.min(...clients.map(c => c.connectTime.getTime()))
        : null,
      heartbeatEnabled: this.config.enableHeartbeat,
      heartbeatInterval: this.config.heartbeatInterval
    };
  }

  shutdown(): void {
    console.log(`🛑 Shutting down ConnectionManager with ${this.clients.size} clients`);
    
    this.stopHeartbeat();
    
    for (const [_clientId, client] of this.clients) {
      if (client.socket.readyState === WebSocket.OPEN) {
        client.socket.close(1001, 'Server shutdown');
      }
    }
    
    this.clients.clear();
    this.removeAllListeners();
  }

  private setupClientHandlers(client: Client): void {
    client.socket.on('close', (code, reason) => {
      console.log(`🔌 Client ${client.id} disconnected - Code: ${code}, Reason: ${reason || 'No reason'}`);
      this.removeClient(client.id);
    });

    client.socket.on('error', (error) => {
      console.error(`❌ Client ${client.id} WebSocket error:`, error.message);
      this.removeClient(client.id);
    });

    client.socket.on('pong', () => {
      console.log(`💓 Client ${client.id} pong received`);
      client.lastActivity = new Date();
    });
  }

  private startHeartbeat(): void {
    this.heartbeatTimer = setInterval(() => {
      this.performHeartbeat();
    }, this.config.heartbeatInterval);
    
    console.log(`💓 Heartbeat started: ${this.config.heartbeatInterval}ms interval`);
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
      console.log(`💓 Heartbeat stopped`);
    }
  }

  private performHeartbeat(): void {
    const now = new Date();
    const _timeoutThreshold = now.getTime() - this.config.clientTimeout;
    const staleClients: string[] = [];
    
    console.log(`💓 Heartbeat check: ${this.clients.size} clients, timeout threshold: ${this.config.clientTimeout}ms`);
    
    for (const [clientId, client] of this.clients) {
      const timeSinceActivity = now.getTime() - client.lastActivity.getTime();
      
      if (timeSinceActivity > this.config.clientTimeout) {
        console.log(`⏰ Client ${clientId} timed out (${timeSinceActivity}ms since last activity)`);
        staleClients.push(clientId);
        continue;
      }
      
      if (client.socket.readyState === WebSocket.OPEN) {
        try {
          console.log(`💓 Sending ping to client ${clientId} (active for ${timeSinceActivity}ms)`);
          client.socket.ping();
        } catch (error) {
          const errorMessage = error instanceof Error ? error.message : String(error);
          console.error(`❌ Ping failed for client ${clientId}:`, errorMessage);
          staleClients.push(clientId);
        }
      } else {
        console.log(`🔌 Client ${clientId} not ready (state: ${client.socket.readyState})`);
        staleClients.push(clientId);
      }
    }
    
    staleClients.forEach(clientId => {
      console.log(`⏰ Removing stale client: ${clientId}`);
      this.removeClient(clientId);
    });
    
    if (staleClients.length > 0) {
      this.emit('heartbeat:cleanup', { removedCount: staleClients.length });
    }
  }

  private generateClientId(): string {
    return `client_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }
}