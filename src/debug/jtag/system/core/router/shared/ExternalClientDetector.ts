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

  /**
   * Detect if message comes from external WebSocket client
   * 
   * External clients use clean endpoints: commands/ping, commands/screenshot
   * Internal systems use prefixed: server/commands/ping, browser/commands/screenshot
   */
  isExternalClient(message: JTAGMessage): boolean {
    // Only requests and responses have correlation IDs
    if (!JTAGMessageTypes.isRequest(message) && !JTAGMessageTypes.isResponse(message)) {
      return false;
    }

    // External clients use clean command endpoints without environment prefixes
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
    console.log(`🚨 BEFORE ADD: ExternalClientDetector registering ${correlationId}`);
    this.externalCorrelations.add(correlationId);
    console.log(`🚨 AFTER ADD: ExternalClientDetector registered ${correlationId}, set now has ${this.externalCorrelations.size} items: [${Array.from(this.externalCorrelations).join(', ')}]`);
  }

  /**
   * Check if correlation belongs to external client
   */
  isExternal(correlationId: string): boolean {
    const result = this.externalCorrelations.has(correlationId);
    console.log(`🔍 ExternalClientDetector: Checking ${correlationId}: hasInSet=${result}, setSize=${this.externalCorrelations.size}, setContents=[${Array.from(this.externalCorrelations).join(', ')}]`);
    return result;
  }

  /**
   * Clean up completed correlations
   */
  removeCorrelation(correlationId: string): void {
    this.externalCorrelations.delete(correlationId);
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