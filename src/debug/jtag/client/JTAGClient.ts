/**
 * Emergency JTAG - Browser Client Implementation
 * 
 * Browser-specific Emergency JTAG implementation.
 * Provides client-side logging with automatic server relay.
 */

import { JTAGBase } from '../shared/JTAGBase';
import type { JTAGConfig } from '../shared/JTAGTypes';

export class EmergencyJTAGClient extends JTAGBase {
  /**
   * Initialize Emergency JTAG for browser context
   */
  static initializeClient(overrides?: Partial<JTAGConfig>): void {
    const clientConfig = {
      context: 'browser' as const,
      enableRemoteLogging: true,
      enableConsoleOutput: true,
      ...overrides
    };
    
    super.initialize(clientConfig);
  }

  /**
   * Browser-specific diagnostic info
   */
  static getBrowserDiagnostics() {
    return {
      userAgent: typeof navigator !== 'undefined' ? navigator.userAgent : 'Unknown',
      url: typeof window !== 'undefined' ? window.location.href : 'Unknown',
      timestamp: new Date().toISOString(),
      viewport: typeof window !== 'undefined' ? {
        width: window.innerWidth,
        height: window.innerHeight
      } : null
    };
  }

  /**
   * Log browser-specific information
   */
  static logBrowserContext(component: string, message: string, includeContext = true): void {
    const data = includeContext ? this.getBrowserDiagnostics() : undefined;
    super.log(component, `[BROWSER] ${message}`, data);
  }
}