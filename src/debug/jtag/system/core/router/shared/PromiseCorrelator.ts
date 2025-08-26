/**
 * PromiseCorrelator - Dedicated request-response promise correlation
 * 
 * CLEAR SEPARATION: Handles only request-response patterns with promises
 * Events are handled by EventDistributor (fire-and-forget)
 * 
 * DESIGN PRINCIPLES:
 * - Request messages create pending promises
 * - Response messages resolve pending promises  
 * - Timeout handling for abandoned requests
 * - Clean correlation ID management
 */

import type { JTAGMessage, JTAGResponsePayload } from '../../types/ResponseTypes';
import { JTAGMessageTypes } from '../../types/JTAGTypes';
import type { JTAGTransport } from '../../../transports';
import { TRANSPORT_TYPES } from '../../../transports';
import { ResponseCorrelator } from '../../shared/ResponseCorrelator';
import { 
  CLIENT_CORRELATION_PREFIX,
  TIMEOUTS 
} from './RouterConstants';
import type { RequestResult } from './RouterTypes';

export interface IPromiseCorrelator {
  sendRequest(message: JTAGMessage): Promise<RequestResult>;
  handleResponse(response: JTAGMessage): Promise<boolean>;
  registerRequestTransport(type: TRANSPORT_TYPES, transport: JTAGTransport): void;
  cleanup(): void;
}

export interface PendingRequest {
  readonly correlationId: string;
  readonly originalMessage: JTAGMessage;
  readonly timestamp: number;
  readonly timeout: number;
}

/**
 * PromiseCorrelator - Clean request-response promise handling
 * 
 * RESPONSIBILITIES:
 * - Send requests and wait for responses
 * - Correlate responses with pending requests  
 * - Handle timeouts and retries
 * - Manage external client correlations
 */
export class PromiseCorrelator implements IPromiseCorrelator {
  private readonly responseCorrelator: ResponseCorrelator;
  private readonly requestTransports = new Map<TRANSPORT_TYPES, JTAGTransport>();
  private readonly pendingRequests = new Map<string, PendingRequest>();

  constructor(correlationTimeout: number = TIMEOUTS.CORRELATION) {
    this.responseCorrelator = new ResponseCorrelator(correlationTimeout);
  }

  /**
   * Send request and wait for correlated response
   */
  async sendRequest(message: JTAGMessage): Promise<RequestResult> {
    if (!JTAGMessageTypes.isRequest(message)) {
      throw new Error('PromiseCorrelator: Only request messages are supported');
    }

    console.log(`🎯 PromiseCorrelator: Sending request ${message.correlationId} to ${message.endpoint}`);

    // Create pending request tracking
    const pendingRequest: PendingRequest = {
      correlationId: message.correlationId,
      originalMessage: message,
      timestamp: Date.now(),
      timeout: TIMEOUTS.CORRELATION
    };

    this.pendingRequests.set(message.correlationId, pendingRequest);

    try {
      // Create promise that will resolve when response arrives
      const responsePromise = this.responseCorrelator.createRequest(message.correlationId);

      // Send the request
      await this.sendRequestMessage(message);
      
      //console.debug(`📤 PromiseCorrelator: Request sent, awaiting response...`);
      
      // Wait for correlated response
      const response = await responsePromise;
      
      //console.debug(`✅ PromiseCorrelator: Response received for ${message.correlationId}`);
      
      // Clean up pending request
      this.pendingRequests.delete(message.correlationId);
      
      return { 
        success: true, 
        resolved: true, 
        response: response as JTAGResponsePayload 
      };

    } catch (error) {
      // Clean up on error
      this.pendingRequests.delete(message.correlationId);
      console.error(`❌ PromiseCorrelator: Request failed:`, error);
      throw error;
    }
  }

  /**
   * Handle incoming response message
   */
  async handleResponse(response: JTAGMessage): Promise<boolean> {
    if (!JTAGMessageTypes.isResponse(response)) {
      console.warn('PromiseCorrelator: Received non-response message');
      return false;
    }

    console.log(`📨 PromiseCorrelator: Received response for ${response.correlationId}`);

    // Check if we have a pending request for this correlation
    const pendingRequest = this.pendingRequests.get(response.correlationId);
    if (!pendingRequest) {
      console.warn(`⚠️ PromiseCorrelator: No pending request found for ${response.correlationId}`);
    }

    // Resolve the correlated promise
    const resolved = this.responseCorrelator.resolveRequest(response.correlationId, response.payload);

    // Handle external client responses (route back via transport)
    if (resolved && response.correlationId.startsWith(CLIENT_CORRELATION_PREFIX)) {
      await this.routeExternalResponse(response);
    }

    // Clean up pending request
    if (pendingRequest) {
      this.pendingRequests.delete(response.correlationId);
    }

    return resolved;
  }

  /**
   * Register transport for sending requests
   */
  registerRequestTransport(type: TRANSPORT_TYPES, transport: JTAGTransport): void {
    this.requestTransports.set(type, transport);
    console.log(`📡 PromiseCorrelator: Registered ${type} transport for requests`);
  }

  /**
   * Send request message via appropriate transport
   */
  private async sendRequestMessage(message: JTAGMessage): Promise<void> {
    // Determine appropriate transport based on message endpoint
    const transportType = this.determineRequestTransport(message);
    const transport = this.requestTransports.get(transportType);

    if (!transport) {
      throw new Error(`No transport available for request: ${transportType}`);
    }

    await transport.send(message);
  }

  /**
   * Determine appropriate transport for request
   */
  private determineRequestTransport(message: JTAGMessage): TRANSPORT_TYPES {
    // P2P requests use P2P transport
    if (message.endpoint.includes('/remote/')) {
      return TRANSPORT_TYPES.P2P;
    }

    // Default to cross-context transport
    return TRANSPORT_TYPES.CROSS_CONTEXT;
  }

  /**
   * Route external client response back via WebSocket
   */
  private async routeExternalResponse(response: JTAGMessage): Promise<void> {
    console.log(`📡 PromiseCorrelator: Routing external response ${response.correlationId} back to client`);
    
    const webSocketTransport = this.requestTransports.get(TRANSPORT_TYPES.CROSS_CONTEXT);
    if (!webSocketTransport) {
      console.warn(`⚠️ PromiseCorrelator: No WebSocket transport for external response ${response.correlationId}`);
      return;
    }

    try {
      await webSocketTransport.send(response);
      console.log(`✅ PromiseCorrelator: External response sent for ${response.correlationId}`);
    } catch (error) {
      console.error(`❌ PromiseCorrelator: Failed to send external response ${response.correlationId}:`, error);
    }
  }

  /**
   * Register external client correlation (for browser clients)
   */
  registerExternalCorrelation(correlationId: string): void {
    console.log(`🔗 PromiseCorrelator: Registering external correlation ${correlationId}`);
    
    this.responseCorrelator.createRequest(correlationId).catch(error => {
      console.warn(`⚠️ External correlation ${correlationId} failed: ${error.message}`);
    });
  }

  /**
   * Get pending requests for debugging
   */
  getPendingRequests(): PendingRequest[] {
    return Array.from(this.pendingRequests.values());
  }

  /**
   * Get correlation statistics
   */
  getStats(): {
    pendingRequests: number;
    oldestPendingAge: number | null;
  } {
    const pending = Array.from(this.pendingRequests.values());
    const now = Date.now();

    return {
      pendingRequests: pending.length,
      oldestPendingAge: pending.length > 0 
        ? Math.max(...pending.map(r => now - r.timestamp))
        : null
    };
  }

  /**
   * Clean up resources
   */
  cleanup(): void {
    this.pendingRequests.clear();
    // ResponseCorrelator cleanup handled internally
  }
}