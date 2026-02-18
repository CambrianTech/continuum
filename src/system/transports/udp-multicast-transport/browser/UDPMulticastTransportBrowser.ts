/**
 * UDP Multicast Transport - Browser Implementation
 * 
 * Browser-side UDP multicast transport using WebRTC DataChannels and WebSocket fallback.
 * Provides P2P mesh networking capabilities for browser JTAG nodes.
 */

import { UDPMulticastTransportBase } from '../shared/UDPMulticastTransportBase';
import type { UDPMulticastConfig } from '../shared/UDPMulticastTypes';

interface BrowserPeerConnection {
  readonly peerId: string;
  readonly peerConnection: RTCPeerConnection;
  readonly dataChannel?: RTCDataChannel;
  readonly state: 'connecting' | 'connected' | 'disconnected';
}

export class UDPMulticastTransportBrowser extends UDPMulticastTransportBase {
  private peerConnections: Map<string, BrowserPeerConnection> = new Map();
  private localDataChannel?: RTCDataChannel;
  private webSocketFallback?: WebSocket;
  private signallingServer: string;

  constructor(config: Partial<UDPMulticastConfig> & { signallingServer?: string }) {
    super(config);
    
    // Browser uses WebSocket signalling server for initial peer discovery
    this.signallingServer = config.signallingServer || `ws://localhost:${config.multicastPort || 37472}`;
    
    console.log(`📡 Browser P2P: Using signalling server ${this.signallingServer}`);
  }

  /**
   * Initialize WebSocket signalling for peer discovery (multicast equivalent)
   */
  protected async initializeMulticastSocket(): Promise<void> {
    console.log(`📡 Browser P2P: Initializing WebSocket signalling`);
    
    return new Promise((resolve, reject) => {
      this.webSocketFallback = new WebSocket(this.signallingServer);
      
      this.webSocketFallback.onopen = () => {
        console.log(`✅ Browser P2P: Connected to signalling server`);
        resolve();
      };
      
      this.webSocketFallback.onerror = (error) => {
        console.error('❌ Browser P2P: Signalling server connection failed:', error);
        reject(new Error('Failed to connect to signalling server'));
      };
      
      this.webSocketFallback.onmessage = (event) => {
        try {
          const messageBuffer = this.stringToBuffer(event.data);
          this.handleIncomingUDPMessage(messageBuffer, { address: 'signalling', port: 0 });
        } catch (error: any) {
          console.warn('⚠️ Browser P2P: Failed to process signalling message:', error.message);
        }
      };
    });
  }

  /**
   * Initialize WebRTC peer connections for direct communication (unicast equivalent)
   */
  protected async initializeUnicastSocket(): Promise<void> {
    console.log(`📡 Browser P2P: WebRTC peer connections ready`);
    
    // WebRTC connections are created on-demand when peers are discovered
    // This method satisfies the abstract interface but doesn't need initialization
  }

  /**
   * Send message via WebSocket signalling (multicast equivalent)
   */
  protected async sendMulticastMessage(message: Buffer): Promise<void> {
    if (!this.webSocketFallback || this.webSocketFallback.readyState !== WebSocket.OPEN) {
      throw new Error('Signalling WebSocket not connected');
    }
    
    const messageStr = this.bufferToString(message);
    this.webSocketFallback.send(messageStr);
  }

  /**
   * Send message via WebRTC DataChannel to specific peer (unicast equivalent)
   */
  protected async sendUnicastMessage(targetPort: number, message: Buffer): Promise<void> {
    // In browser context, targetPort maps to peerId
    const targetPeerId = `peer-${targetPort}`;
    const peer = this.peerConnections.get(targetPeerId);
    
    if (!peer || !peer.dataChannel || peer.dataChannel.readyState !== 'open') {
      // Fallback to WebSocket signalling if direct connection not available
      console.log(`⚡ Browser P2P: Direct channel unavailable for ${targetPeerId}, using signalling fallback`);
      await this.sendMulticastMessage(message);
      return;
    }
    
    const messageStr = this.bufferToString(message);
    peer.dataChannel.send(messageStr);
  }

  /**
   * Create WebRTC peer connection for direct communication
   */
  private async createPeerConnection(peerId: string): Promise<BrowserPeerConnection> {
    console.log(`🤝 Browser P2P: Creating WebRTC connection to ${peerId}`);
    
    const peerConnection = new RTCPeerConnection({
      iceServers: [
        { urls: 'stun:stun.l.google.com:19302' },
        { urls: 'stun:stun1.l.google.com:19302' }
      ]
    });
    
    // Create data channel for P2P communication
    const dataChannel = peerConnection.createDataChannel('jtag-p2p', {
      ordered: true,
      maxRetransmits: 3
    });
    
    dataChannel.onopen = () => {
      console.log(`✅ Browser P2P: DataChannel open with ${peerId}`);
    };
    
    dataChannel.onmessage = (event) => {
      try {
        const messageBuffer = this.stringToBuffer(event.data);
        this.handleIncomingUDPMessage(messageBuffer, { address: peerId, port: 0 });
      } catch (error: any) {
        console.warn(`⚠️ Browser P2P: DataChannel message error from ${peerId}:`, error.message);
      }
    };
    
    dataChannel.onerror = (error) => {
      console.error(`❌ Browser P2P: DataChannel error with ${peerId}:`, error);
    };
    
    const peer: BrowserPeerConnection = {
      peerId,
      peerConnection,
      dataChannel,
      state: 'connecting'
    };
    
    this.peerConnections.set(peerId, peer);
    return peer;
  }

  /**
   * Handle WebRTC signalling for peer connection establishment
   */
  private async handleWebRTCSignalling(peerId: string, signal: unknown): Promise<void> {
    let peer = this.peerConnections.get(peerId);
    
    if (!peer) {
      peer = await this.createPeerConnection(peerId);
    }
    
    // Handle WebRTC signalling messages (offer, answer, ICE candidates)
    // This would integrate with the signalling server protocol
    // For now, we use WebSocket fallback for all communication
    console.log(`📡 Browser P2P: Received WebRTC signal from ${peerId}:`, signal);
  }

  /**
   * Clean up WebRTC connections and WebSocket signalling
   */
  protected async cleanup(): Promise<void> {
    console.log('🧹 Browser P2P: Cleaning up connections');
    
    // Close all peer connections
    for (const [peerId, peer] of this.peerConnections) {
      try {
        if (peer.dataChannel) {
          peer.dataChannel.close();
        }
        peer.peerConnection.close();
      } catch (error: any) {
        console.warn(`⚠️ Browser P2P: Peer cleanup warning for ${peerId}:`, error.message);
      }
    }
    this.peerConnections.clear();
    
    // Close signalling WebSocket
    if (this.webSocketFallback) {
      try {
        this.webSocketFallback.close();
      } catch (error: any) {
        console.warn('⚠️ Browser P2P: Signalling cleanup warning:', error.message);
      }
      this.webSocketFallback = undefined;
    }
  }

  /**
   * Get browser hostname (limited information available)
   */
  protected getHostname(): string {
    return window.location.hostname || 'browser-client';
  }

  /**
   * Get browser platform information
   */
  protected getPlatform(): string {
    const nav = window.navigator;
    return `${nav.userAgent} (${nav.platform})`;
  }

  /**
   * Convert Buffer to string for browser WebSocket/DataChannel transmission
   */
  private bufferToString(buffer: Buffer): string {
    return buffer.toString('base64');
  }

  /**
   * Convert string back to Buffer for processing
   */
  private stringToBuffer(str: string): Buffer {
    return Buffer.from(str, 'base64');
  }

  /**
   * Reconnect to P2P network
   */
  async reconnect(): Promise<void> {
    console.log('🔄 Browser P2P: Reconnecting to P2P network...');
    await this.disconnect();
    await this.initialize();
  }
}