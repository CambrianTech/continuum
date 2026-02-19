/**
 * UDP Multicast Transport - Server Implementation
 * 
 * Server-side UDP multicast transport using Node.js dgram sockets.
 * Provides P2P mesh networking capabilities for server JTAG nodes.
 */

import * as dgram from 'dgram';
import * as os from 'os';
import { UDPMulticastTransportBase } from '../shared/UDPMulticastTransportBase';
import type { UDPMulticastConfig } from '../shared/UDPMulticastTypes';

export class UDPMulticastTransportServer extends UDPMulticastTransportBase {
  private multicastSocket?: dgram.Socket;
  private unicastSocket?: dgram.Socket;

  /**
   * Initialize multicast socket for discovery and broadcast messages
   */
  protected async initializeMulticastSocket(): Promise<void> {
    console.log(`📡 UDP Server: Initializing multicast socket ${this.config.multicastAddress}:${this.config.multicastPort}`);
    
    this.multicastSocket = dgram.createSocket({ type: 'udp4', reuseAddr: true });
    
    return new Promise((resolve, reject) => {
      const socket = this.multicastSocket!;
      
      socket.on('error', (error) => {
        console.error('❌ UDP Server: Multicast socket error:', error.message);
        reject(error);
      });
      
      socket.on('message', (messageBuffer, remoteInfo) => {
        this.handleIncomingUDPMessage(messageBuffer, remoteInfo);
      });
      
      socket.bind(this.config.multicastPort, () => {
        try {
          // Join multicast group
          socket.addMembership(this.config.multicastAddress);
          socket.setMulticastTTL(this.config.ttl);
          socket.setMulticastLoopback(true); // Enable loopback for localhost testing (nodes filter their own messages)
          
          console.log(`✅ UDP Server: Multicast socket ready (TTL: ${this.config.ttl})`);
          resolve();
        } catch (error) {
          reject(error);
        }
      });
    });
  }

  /**
   * Initialize unicast socket for direct peer communication
   */
  protected async initializeUnicastSocket(): Promise<void> {
    console.log(`📡 UDP Server: Initializing unicast socket on port ${this.config.unicastPort}`);
    
    this.unicastSocket = dgram.createSocket('udp4');
    
    return new Promise((resolve, reject) => {
      const socket = this.unicastSocket!;
      
      socket.on('error', (error) => {
        console.error('❌ UDP Server: Unicast socket error:', error.message);
        reject(error);
      });
      
      socket.on('message', (messageBuffer, remoteInfo) => {
        this.handleIncomingUDPMessage(messageBuffer, remoteInfo);
      });
      
      socket.bind(this.config.unicastPort, () => {
        console.log(`✅ UDP Server: Unicast socket ready on port ${this.config.unicastPort}`);
        resolve();
      });
    });
  }

  /**
   * Send message via multicast for discovery and broadcast
   */
  protected async sendMulticastMessage(message: Buffer): Promise<void> {
    if (!this.multicastSocket) {
      throw new Error('Multicast socket not initialized');
    }
    
    return new Promise((resolve, reject) => {
      this.multicastSocket!.send(
        message,
        this.config.multicastPort,
        this.config.multicastAddress,
        (error) => {
          if (error) {
            reject(error);
          } else {
            resolve();
          }
        }
      );
    });
  }

  /**
   * Send message via unicast to specific peer
   */
  protected async sendUnicastMessage(targetPort: number, message: Buffer): Promise<void> {
    if (!this.unicastSocket) {
      throw new Error('Unicast socket not initialized');
    }
    
    // For now, send to localhost - in production this would resolve target IP
    const targetAddress = '127.0.0.1';
    
    return new Promise((resolve, reject) => {
      this.unicastSocket!.send(
        message,
        targetPort,
        targetAddress,
        (error) => {
          if (error) {
            reject(error);
          } else {
            resolve();
          }
        }
      );
    });
  }

  /**
   * Clean up sockets and resources
   */
  protected async cleanup(): Promise<void> {
    console.log('🧹 UDP Server: Cleaning up sockets');
    
    if (this.multicastSocket) {
      try {
        this.multicastSocket.dropMembership(this.config.multicastAddress);
        this.multicastSocket.close();
      } catch (error: any) {
        console.warn('⚠️ UDP Server: Multicast socket cleanup warning:', error.message);
      }
      this.multicastSocket = undefined;
    }
    
    if (this.unicastSocket) {
      try {
        this.unicastSocket.close();
      } catch (error: any) {
        console.warn('⚠️ UDP Server: Unicast socket cleanup warning:', error.message);
      }
      this.unicastSocket = undefined;
    }
  }

  /**
   * Get hostname for node metadata
   */
  protected getHostname(): string {
    return os.hostname();
  }

  /**
   * Get platform information for node metadata
   */
  protected getPlatform(): string {
    return `${os.type()} ${os.release()} (${os.arch()})`;
  }

  /**
   * Reconnect to P2P network
   */
  async reconnect(): Promise<void> {
    console.log('🔄 UDP Server: Reconnecting to P2P network...');
    await this.disconnect();
    await this.initialize();
  }
}