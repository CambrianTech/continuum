/**
 * CorrelationManager - Dedicated correlation mapping system for request-response tracking
 * 
 * Extracted from JTAGRouter to improve modularity and separation of concerns.
 * Handles the mapping between processing tokens and correlation IDs for ResponseCorrelator.
 */

import { REQUEST_TOKEN_PREFIX, RESPONSE_TOKEN_PREFIX } from './RouterConstants';

export class CorrelationManager {
  // Map req: processing tokens to raw correlation IDs for ResponseCorrelator
  private readonly reqToCorrelation = new Map<string, string>(); // req:abc123 -> abc123
  
  // Map res: processing tokens to corresponding req: tokens for lookup
  private readonly resToReq = new Map<string, string>(); // res:abc123 -> req:abc123

  /**
   * Register a new request correlation mapping
   */
  registerRequest(correlationId: string): void {
    const reqToken = `${REQUEST_TOKEN_PREFIX}${correlationId}`;
    const resToken = `${RESPONSE_TOKEN_PREFIX}${correlationId}`;
    this.reqToCorrelation.set(reqToken, correlationId);
    this.resToReq.set(resToken, reqToken);
  }

  /**
   * Get correlation ID from request token
   */
  getCorrelationFromReq(reqToken: string): string | undefined {
    return this.reqToCorrelation.get(reqToken);
  }

  /**
   * Get request token from response token
   */
  getReqFromRes(resToken: string): string | undefined {
    return this.resToReq.get(resToken);
  }

  /**
   * Clean up correlation mappings for a given correlation ID
   */
  cleanup(correlationId: string): void {
    const reqToken = `${REQUEST_TOKEN_PREFIX}${correlationId}`;
    const resToken = `${RESPONSE_TOKEN_PREFIX}${correlationId}`;
    this.reqToCorrelation.delete(reqToken);
    this.resToReq.delete(resToken);
  }

  /**
   * Get current mapping statistics for debugging
   */
  getStats(): { reqMappings: number; resMappings: number } {
    return {
      reqMappings: this.reqToCorrelation.size,
      resMappings: this.resToReq.size
    };
  }

  /**
   * Clear all mappings (useful for testing or cleanup)
   */
  clear(): void {
    this.reqToCorrelation.clear();
    this.resToReq.clear();
  }
}