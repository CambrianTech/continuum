/**
 * External Client Detector - Clean automatic WebSocket client detection
 * 
 * Replaces sloppy correlation prefix approach with intelligent message analysis.
 * Detects external WebSocket clients based on clean endpoint patterns.
 */

import type { JTAGMessage } from '../../types/JTAGTypes';
import { JTAGMessageTypes } from '../../types/JTAGTypes';

export class ExternalClientDetector {
  private readonly externalCorrelations = new Set<string>();
  private readonly correlationTimestamps = new Map<string, number>();
  private readonly TTL_MS = 5 * 60 * 1000; // 5 minutes TTL
  private cleanupTimer?: NodeJS.Timeout;

  /**
   * Detect if message comes from external WebSocket client
   * 
   * External clients can be:
   * 1. Clean endpoints: commands/ping, commands/screenshot (pure WebSocket clients)
   * 2. Client prefixed correlations: client_* (JTAG clients using WebSocket transport)
   */
  isExternalClient(message: JTAGMessage): boolean {
    // Only requests and responses have correlation IDs
    if (!JTAGMessageTypes.isRequest(message) && !JTAGMessageTypes.isResponse(message)) {
      return false;
    }

    const correlationId = this.getCorrelationId(message);
    
    // Method 1: Detect client_ correlation prefix (JTAG clients over WebSocket)
    if (correlationId && correlationId.startsWith('client_')) {
      return true;
    }

    // Method 2: Clean command endpoints without environment prefixes (direct WebSocket clients)
    const hasCleanEndpoint = message.endpoint.startsWith('commands/') && 
                             !message.endpoint.includes('server/') && 
                             !message.endpoint.includes('browser/');

    // External clients have clean origins (not internal daemon routes)
    const hasCleanOrigin = !message.origin.includes('server/') && 
                           !message.origin.includes('browser/') &&
                           !message.origin.includes('daemon');

    return hasCleanEndpoint && hasCleanOrigin;
  }

  /**
   * Register external correlation for response routing
   */
  registerExternal(correlationId: string): void {
    const timestamp = Date.now();
    
    // console.debug(`🔗 ExternalClientDetector: Registering ${correlationId}`);
    this.externalCorrelations.add(correlationId);
    this.correlationTimestamps.set(correlationId, timestamp);
    
    // console.debug(`📊 ExternalClientDetector: Registered ${correlationId}, set now has ${this.externalCorrelations.size} items`);
    
    // Start intelligent cleanup if not already running
    this.startAutomaticCleanup();
  }

  /**
   * Check if correlation belongs to external client
   */
  isExternal(correlationId: string): boolean {
    const result = this.externalCorrelations.has(correlationId);
    return result;
  }

  /**
   * Clean up completed correlations
   */
  removeCorrelation(correlationId: string): void {
    const removed = this.externalCorrelations.delete(correlationId);
    this.correlationTimestamps.delete(correlationId);
    
    if (removed) {
      // console.debug(`🧹 ExternalClientDetector: Cleaned up ${correlationId}, set now has ${this.externalCorrelations.size} items`);
    } else {
      console.warn(`🧹 ExternalClientDetector: No cleanup needed for ${correlationId}`);
    }
  }
  
  /**
   * INTELLIGENCE: Automatic cleanup of stale correlations
   */
  private startAutomaticCleanup(): void {
    if (this.cleanupTimer) return; // Already running
    
    // console.debug('🤖 ExternalClientDetector: Starting intelligent auto-cleanup');
    
    this.cleanupTimer = setInterval(() => {
      this.cleanupStaleCorrelations();
    }, 30000); // Clean up every 30 seconds
  }
  
  /**
   * INTELLIGENCE: Remove correlations older than TTL
   */
  private cleanupStaleCorrelations(): void {
    const now = Date.now();
    const staleIds: string[] = [];
    
    // Find stale correlations
    for (const [id, timestamp] of this.correlationTimestamps) {
      if (now - timestamp > this.TTL_MS) {
        staleIds.push(id);
      }
    }
    
    // Remove stale correlations
    if (staleIds.length > 0) {
      // console.debug(`🧹 ExternalClientDetector: Auto-cleaning ${staleIds.length} stale correlations`);
      
      for (const id of staleIds) {
        this.externalCorrelations.delete(id);
        this.correlationTimestamps.delete(id);
      }

      // console.debug(`📊 ExternalClientDetector: After cleanup: ${this.externalCorrelations.size} active correlations`);
    }
    
    // Stop timer if no correlations left
    if (this.externalCorrelations.size === 0 && this.cleanupTimer) {
      // console.debug('🛑 ExternalClientDetector: Stopping auto-cleanup (no correlations)');
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = undefined;
    }
  }
  
  /**
   * Manual cleanup for shutdown
   */
  cleanup(): void {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = undefined;
    }
    this.externalCorrelations.clear();
    this.correlationTimestamps.clear();
    // console.debug('🧹 ExternalClientDetector: Manual cleanup completed');
  }

  /**
   * Get correlation ID safely with proper type checking
   */
  getCorrelationId(message: JTAGMessage): string | null {
    if (JTAGMessageTypes.isRequest(message) || JTAGMessageTypes.isResponse(message)) {
      return (message as any).correlationId;
    }
    return null;
  }
}