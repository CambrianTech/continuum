/**
 * HTTP Transport - REST API transport implementation
 * 
 * Extracted from root HTTPTransport.ts into proper modular structure.
 * Provides stateless HTTP-based message transport for cross-context communication.
 */

import { TransportBase } from '../../shared/TransportBase';
import type { JTAGMessage } from '../../../core/types/JTAGTypes';
import type { TransportSendResult } from '../../shared/TransportTypes';

export class HTTPTransport extends TransportBase {
  public readonly name = 'http-transport';
  
  private baseUrl: string;

  constructor(baseUrl: string) {
    super();
    this.baseUrl = baseUrl;
    this.connected = true; // HTTP is stateless, so always "connected" if fetch is available
  }

  async send(message: JTAGMessage): Promise<TransportSendResult> {
    const endpoint = `${this.baseUrl}/api/jtag/message`;
    
    console.log(`📤 HTTP Transport: Sending message to ${endpoint}`);
    
    try {
      const response = await fetch(endpoint, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(message)
      });

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      const result = await response.json();
      console.log(`✅ HTTP Transport: Message sent successfully`);
      
      return this.createResult(true);
    } catch (error) {
      console.error(`❌ HTTP Transport: Send failed:`, error);
      throw error;
    }
  }

  isConnected(): boolean {
    // HTTP is stateless, so always "connected" if fetch is available
    return typeof fetch !== 'undefined';
  }

  async disconnect(): Promise<void> {
    console.log(`🔌 HTTP Transport: No persistent connection to disconnect`);
    this.connected = false;
  }

  async reconnect(): Promise<void> {
    console.log(`🔄 HTTP Transport: No reconnection needed for stateless HTTP`);
    this.connected = true;
  }

  /**
   * Start HTTP server for receiving messages (server-side only)
   */
  async startServer(port: number): Promise<void> {
    console.log(`🔗 HTTP Transport: Would start HTTP server on port ${port}`);
    
    // In real implementation, would use Express or similar:
    // const express = require('express');
    // const app = express();
    // app.use(express.json());
    // 
    // app.post('/api/jtag/message', (req, res) => {
    //   const message = req.body;
    //   // Route message to local subscribers
    //   res.json({ success: true, timestamp: new Date().toISOString() });
    // });
    // 
    // app.listen(port, () => {
    //   console.log(`HTTP server listening on port ${port}`);
    // });
  }
}