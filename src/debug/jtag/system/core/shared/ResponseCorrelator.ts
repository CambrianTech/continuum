/**
 * Response Correlator - Maintains Promise chains for command-response patterns
 * 
 * Transforms fire-and-forget message bus into request-response system
 */

import type { TimerHandle } from '../types/CrossPlatformTypes';
import type { JTAGPayload, UUID } from '../types/JTAGTypes';
import type { JTAGResponsePayload } from '../types/ResponseTypes';

export interface PendingRequest {
  resolve: (result: JTAGResponsePayload) => void;
  reject: (error: Error) => void;
  timestamp: number;
  timeout: TimerHandle;
}

export interface CorrelatorStatus {
  pending: number;
  oldest: number;
}

export class ResponseCorrelator {
  private pendingRequests = new Map<UUID, PendingRequest>();
  private defaultTimeoutMs: number;

  constructor(defaultTimeoutMs: number = 30000) {
    this.defaultTimeoutMs = defaultTimeoutMs;
  }

  /**
   * Create a new request correlation and return Promise
   * Only for outgoing requests - should not be called by incoming request handlers
   */
  createRequest(correlationId: UUID, timeoutMs?: number): Promise<JTAGResponsePayload> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pendingRequests.delete(correlationId);
        reject(new Error(`Request timeout after ${timeoutMs ?? this.defaultTimeoutMs}ms`));
      }, timeoutMs ?? this.defaultTimeoutMs);

      this.pendingRequests.set(correlationId, {
        resolve,
        reject,
        timestamp: Date.now(),
        timeout
      });

    });
  }

  /**
   * Resolve a pending request with response
   */
  resolveRequest(correlationId: UUID, response: JTAGResponsePayload): boolean {
    const pending = this.pendingRequests.get(correlationId);
    if (!pending) {
      console.warn(`⚠️ ResponseCorrelator: No pending request for ${correlationId}`);
      return false;
    }

    clearTimeout(pending.timeout);
    this.pendingRequests.delete(correlationId);
    pending.resolve(response);
    
    return true;
  }

  /**
   * Reject a pending request with error
   */
  rejectRequest(correlationId: string, error: Error): boolean {
    const pending = this.pendingRequests.get(correlationId);
    if (!pending) {
      return false;
    }

    clearTimeout(pending.timeout);
    this.pendingRequests.delete(correlationId);
    pending.reject(error);
    
    console.log(`❌ ResponseCorrelator: Rejected request ${correlationId}`);
    return true;
  }

  /**
   * Generate unique correlation ID
   */
  generateCorrelationId(): string {
    return `req_${Date.now()}_${Math.random().toString(36).substr(2, 8)}`;
  }

  /**
   * Get status of pending requests
   */
  get status(): CorrelatorStatus {
    const now = Date.now();
    return {
      pending: this.pendingRequests.size,
      oldest: this.pendingRequests.size > 0 
        ? Math.min(...Array.from(this.pendingRequests.values()).map(r => now - r.timestamp))
        : 0
    };
  }

  /**
   * Clean up expired/orphaned requests
   */
  cleanup(): void {
    const now = Date.now();
    let cleaned = 0;

    for (const [id, request] of this.pendingRequests.entries()) {
      if (now - request.timestamp > this.defaultTimeoutMs * 2) {
        clearTimeout(request.timeout);
        this.pendingRequests.delete(id);
        request.reject(new Error('Request cleanup - likely orphaned'));
        cleaned++;
      }
    }

    if (cleaned > 0) {
      console.log(`🧹 ResponseCorrelator: Cleaned ${cleaned} expired requests`);
    }
  }
}