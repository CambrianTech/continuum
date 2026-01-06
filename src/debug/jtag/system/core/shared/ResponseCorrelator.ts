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
      // Expected when response arrives after timeout - not an error
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
    
    // Silently reject - caller will handle error
    return true;
  }

  /**
   * Generate unique correlation ID using proper UUID
   */
  generateCorrelationId(): string {
    const { generateUUID } = require('../types/CrossPlatformUUID');
    return generateUUID();
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

  /**
   * Reject all pending requests (e.g., on disconnect)
   * Silently fails pending requests without console spam
   */
  rejectAll(reason: string = 'Connection lost'): void {
    const count = this.pendingRequests.size;
    if (count === 0) return;

    for (const [id, request] of this.pendingRequests.entries()) {
      clearTimeout(request.timeout);
      request.reject(new Error(reason));
    }

    this.pendingRequests.clear();
    console.log(`🔌 ResponseCorrelator: Rejected ${count} pending requests (${reason})`);
  }

  /**
   * Get count of pending requests
   */
  get pendingCount(): number {
    return this.pendingRequests.size;
  }
}