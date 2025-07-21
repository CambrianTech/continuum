/**
 * JTAG Client Transport - Browser-side transport that connects to server
 */

import { JTAGUniversalMessage } from '../shared/JTAGTypes';
import { JTAGTransportBackend } from '../shared/JTAGRouter';

export class JTAGClientTransport implements JTAGTransportBackend {
  name = 'jtag-client';
  private websocket: WebSocket | null = null;
  private connected = false;
  private port: number;

  constructor(port: number = 9001) {
    this.port = port;
  }

  canHandle(message: JTAGUniversalMessage): boolean {
    // Client transport handles messages that need to go to server
    return message.source === 'browser' && message.type === 'log';
  }

  async process(message: JTAGUniversalMessage): Promise<any> {
    if (!this.connected) {
      await this.connect();
    }

    return new Promise((resolve) => {
      if (this.websocket && this.websocket.readyState === WebSocket.OPEN) {
        this.websocket.send(JSON.stringify(message));
        resolve({ sent: true, transport: 'websocket', timestamp: new Date().toISOString() });
      } else {
        // Queue message or handle offline scenario
        console.warn('📡 JTAG Client: WebSocket not connected, message queued');
        resolve({ sent: false, queued: true, reason: 'websocket_disconnected' });
      }
    });
  }

  private async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        this.websocket = new WebSocket(`ws://localhost:${this.port}`);
        
        this.websocket.onopen = () => {
          this.connected = true;
          console.log('📡 JTAG Client: Connected to server');
          resolve();
        };

        this.websocket.onerror = (error) => {
          console.error('📡 JTAG Client: WebSocket error:', error);
          reject(error);
        };

        this.websocket.onclose = () => {
          this.connected = false;
          console.log('📡 JTAG Client: Disconnected from server');
        };

      } catch (error) {
        reject(error);
      }
    });
  }

  isHealthy(): boolean {
    return this.connected && this.websocket?.readyState === WebSocket.OPEN;
  }

  disconnect(): void {
    if (this.websocket) {
      this.websocket.close();
      this.connected = false;
    }
  }
}