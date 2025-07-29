/**
 * Transport Configuration - Auto-detection and optimization for transport selection
 * 
 * Extracted from TransportFactory to handle transport configuration logic.
 */

import { JTAG_ENVIRONMENTS } from '@shared/JTAGTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { TransportConfig } from './TransportTypes';

export class TransportConfigHelper {
  /**
   * Auto-detect optimal transport configuration for environment
   */
  static detectOptimalConfig(environment: JTAGContext['environment']): TransportConfig {
    // In browser, prefer WebSocket client
    if (environment === JTAG_ENVIRONMENTS.BROWSER) {
      return {
        preferred: 'websocket',
        fallback: true,
        serverUrl: 'ws://localhost:9001'
      };
    }
    
    // On server, prefer WebSocket server
    if (environment === JTAG_ENVIRONMENTS.SERVER) {
      return {
        preferred: 'websocket',
        fallback: true,
        serverPort: 9001
      };
    }
    
    // Remote contexts use HTTP by default
    return {
      preferred: 'http',
      fallback: false
    };
  }

  /**
   * Merge user config with detected optimal config
   */
  static mergeWithDefaults(
    environment: JTAGContext['environment'], 
    userConfig: Partial<TransportConfig> = {}
  ): TransportConfig {
    const defaults = this.detectOptimalConfig(environment);
    return { ...defaults, ...userConfig };
  }

  /**
   * Validate transport configuration
   */
  static validateConfig(config: TransportConfig): void {
    const { preferred, serverPort, serverUrl } = config;

    if (preferred === 'websocket') {
      if (!serverPort && !serverUrl) {
        throw new Error('WebSocket transport requires either serverPort or serverUrl');
      }
    }

    if (serverPort && (serverPort < 1 || serverPort > 65535)) {
      throw new Error(`Invalid server port: ${serverPort}. Must be between 1-65535`);
    }
  }
}