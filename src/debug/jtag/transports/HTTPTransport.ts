/**
 * HTTP Transport Implementation
 * 
 * REST API transport for cross-context communication when WebSocket isn't available.
 */
import type { JTAGTransport } from '@transports/TransportFactory';
import type { JTAGMessage } from '@shared/JTAGTypes';

export class HTTPTransport implements JTAGTransport {
  name = 'http-transport';
  private baseUrl: string;

  constructor(baseUrl: string = 'http://localhost:9002') {
    this.baseUrl = baseUrl;
  }

  async send(message: JTAGMessage): Promise<{ success: boolean; timestamp: string; data?: unknown }> {
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
      
      return { success: true, timestamp: new Date().toISOString(), data: result };
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