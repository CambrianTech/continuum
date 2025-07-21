/**
 * REST API Transport for JTAG
 * 
 * This transport demonstrates how to add new transport layers to JTAG.
 * Messages are sent via HTTP REST API calls instead of WebSocket.
 * 
 * Usage:
 * - Logs → POST /api/jtag/logs
 * - Screenshots → POST /api/jtag/screenshots  
 * - Exec → POST /api/jtag/exec
 */

import { JTAGTransportBackend } from '../shared/JTAGRouter';
import { JTAGUniversalMessage, JTAG_MESSAGE_TYPES } from '../shared/JTAGTypes';

export class RestApiTransport implements JTAGTransportBackend {
  public readonly name = 'rest-api';
  private baseUrl: string;
  private headers: Record<string, string>;
  private healthy: boolean = true;

  constructor(baseUrl: string = 'http://localhost:3000', customHeaders: Record<string, string> = {}) {
    this.baseUrl = baseUrl.replace(/\/$/, ''); // Remove trailing slash
    this.headers = {
      'Content-Type': 'application/json',
      'X-JTAG-Transport': 'rest-api',
      ...customHeaders
    };
  }

  canHandle(message: JTAGUniversalMessage): boolean {
    // This transport can handle all standard JTAG message types
    const supportedTypes = [
      JTAG_MESSAGE_TYPES.LOG,
      JTAG_MESSAGE_TYPES.SCREENSHOT,
      JTAG_MESSAGE_TYPES.EXEC,
      JTAG_MESSAGE_TYPES.HEALTH
    ];
    
    return supportedTypes.includes(message.type as any);
  }

  isHealthy(): boolean {
    return this.healthy;
  }

  async process(message: JTAGUniversalMessage): Promise<any> {
    try {
      const endpoint = this.getEndpointForMessageType(message.type);
      const url = `${this.baseUrl}${endpoint}`;

      console.log(`📡 REST Transport: ${message.type.toUpperCase()} → ${url}`);

      const response = await fetch(url, {
        method: 'POST',
        headers: this.headers,
        body: JSON.stringify({
          message,
          timestamp: new Date().toISOString(),
          transport: 'rest-api'
        })
      });

      if (!response.ok) {
        this.healthy = false;
        throw new Error(`REST API responded with ${response.status}: ${response.statusText}`);
      }

      const result = await response.json();
      this.healthy = true; // Mark as healthy on successful response
      
      return {
        success: true,
        transport: 'rest-api',
        endpoint: url,
        httpStatus: response.status,
        data: result
      };

    } catch (error: any) {
      this.healthy = false; // Mark as unhealthy on error
      console.error(`❌ REST Transport Error:`, error.message);
      
      return {
        success: false,
        transport: 'rest-api',
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }

  private getEndpointForMessageType(messageType: string): string {
    switch (messageType) {
      case JTAG_MESSAGE_TYPES.LOG:
        return '/api/jtag/logs';
      case JTAG_MESSAGE_TYPES.SCREENSHOT:
        return '/api/jtag/screenshots';
      case JTAG_MESSAGE_TYPES.EXEC:
        return '/api/jtag/exec';
      case JTAG_MESSAGE_TYPES.HEALTH:
        return '/api/jtag/health';
      default:
        return '/api/jtag/messages';
    }
  }

  /**
   * Register routes for this transport in JTAG router
   */
  static registerRoutes(jtagRouter: any): void {
    // Add specific routes for REST API transport
    jtagRouter.addRoute('logs-to-rest', {
      pattern: { 
        type: 'conditional', 
        condition: (msg: JTAGUniversalMessage) => 
          msg.type === JTAG_MESSAGE_TYPES.LOG && 
          (msg.payload as any)?.restApiTarget === true 
      },
      transport: 'rest-api',
      enabled: true,
      metadata: { 
        priority: 7, 
        description: 'Route logs marked for REST API to REST transport' 
      }
    });

    jtagRouter.addRoute('screenshots-to-rest', {
      pattern: { 
        type: 'conditional',
        condition: (msg: JTAGUniversalMessage) => 
          msg.type === JTAG_MESSAGE_TYPES.SCREENSHOT && 
          (msg.payload as any)?.format === 'api' 
      },
      transport: 'rest-api',
      enabled: true,
      metadata: { 
        priority: 6, 
        description: 'Route API-format screenshots to REST transport' 
      }
    });

    console.log('🔗 REST Transport: Routes registered with JTAG router');
  }

  /**
   * Health check method that can be called periodically
   */
  async healthCheck(): Promise<boolean> {
    try {
      const response = await fetch(`${this.baseUrl}/api/jtag/health`, {
        method: 'GET',
        headers: { 'X-JTAG-Transport': 'rest-api-health-check' },
        signal: AbortSignal.timeout(5000) // 5 second timeout
      });
      
      this.healthy = response.ok;
      return this.healthy;
      
    } catch (error) {
      this.healthy = false;
      return false;
    }
  }
}