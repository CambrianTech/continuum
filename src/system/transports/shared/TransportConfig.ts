/**
 * Transport Configuration - Auto-detection and optimization for transport selection
 * 
 * Extracted from TransportFactory to handle transport configuration logic.
 */

import { JTAG_ENVIRONMENTS } from '../../core/types/JTAGTypes';
import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { TransportConfig } from './TransportTypes';

export class TransportConfigHelper {
  /**
   * Auto-detect optimal transport configuration for environment
   */
  static detectOptimalConfig(environment: JTAGContext['environment'], context?: JTAGContext): Partial<TransportConfig> {
    // Fallback to 9001 (should match WS_PORT in config.env) - context should always provide this
    const wsPort = context?.config?.instance?.ports?.websocket_server || 9001;
    
    // In browser, prefer WebSocket client
    if (environment === JTAG_ENVIRONMENTS.BROWSER) {
      return {
        protocol: 'websocket',
        role: 'client',
        fallback: true,
        serverUrl: `ws://localhost:${wsPort}`
      };
    }
    
    // On server, prefer WebSocket server
    if (environment === JTAG_ENVIRONMENTS.SERVER) {
      return {
        protocol: 'websocket',
        role: 'server',
        fallback: true,
        serverPort: wsPort
      };
    }
    
    // Remote contexts use HTTP by default
    return {
      protocol: 'http',
      role: 'client',
      fallback: false
    };
  }

  /**
   * Merge user config with detected optimal config
   */
  static mergeWithDefaults(
    environment: JTAGContext['environment'], 
    userConfig: Partial<TransportConfig> = {}
  ): Partial<TransportConfig> {
    const defaults = this.detectOptimalConfig(environment);
    return { ...defaults, ...userConfig };
  }

  /**
   * Validate transport configuration
   */
  static validateConfig(config: TransportConfig): void {
    const { protocol, serverPort, serverUrl } = config;

    if (protocol === 'websocket') {
      if (!serverPort && !serverUrl) {
        throw new Error('WebSocket transport requires either serverPort or serverUrl');
      }
    }

    if (serverPort && (serverPort < 1 || serverPort > 65535)) {
      throw new Error(`Invalid server port: ${serverPort}. Must be between 1-65535`);
    }
  }
}