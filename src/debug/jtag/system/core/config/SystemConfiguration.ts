/**
 * System Configuration - Central configuration management for all JTAG components
 * 
 * Provides centralized configuration that propagates to:
 * - Transport factories 
 * - Client connections
 * - Router endpoints
 * - Channel factories
 * - Port allocations
 */

import type { JTAGInstanceConfig } from './shared/PortConfigTypes';

export interface JTAGSystemConfiguration {
  readonly instance: JTAGInstanceConfig;
  readonly transport: {
    readonly websocket: {
      readonly serverPort: number;
      readonly clientUrl: string;
    };
    readonly http: {
      readonly serverPort: number;
      readonly baseUrl: string;
    };
    readonly udp: {
      readonly multicastPort: number;
      readonly unicastPort: number;
      readonly multicastAddress: string;
    };
  };
  readonly routing: {
    readonly enableP2P: boolean;
    readonly maxHops: number;
    readonly discoveryInterval: number;
  };
  readonly debugging: {
    readonly enableVerboseLogging: boolean;
    readonly testMode: boolean;
  };
}

/**
 * Global system configuration manager
 */
export class SystemConfiguration {
  private static _instance: SystemConfiguration | null = null;
  private readonly config: JTAGSystemConfiguration;

  private constructor(config: JTAGSystemConfiguration) {
    this.config = config;
  }

  /**
   * Initialize system configuration (should be called once at startup)
   */
  static initialize(instanceConfig?: JTAGInstanceConfig): SystemConfiguration {
    if (SystemConfiguration._instance) {
      console.warn('SystemConfiguration already initialized, using existing instance');
      return SystemConfiguration._instance;
    }

    // Get configuration from environment, fallback to defaults
    const instance = instanceConfig || SystemConfiguration.getDefaultConfig();
    
    const config: JTAGSystemConfiguration = {
      instance,
      transport: {
        websocket: {
          serverPort: instance.wsPort,
          clientUrl: `ws://localhost:${instance.wsPort}`
        },
        http: {
          serverPort: instance.httpPort,
          baseUrl: `http://localhost:${instance.httpPort}`
        },
        udp: {
          multicastPort: instance.multicastPort,
          unicastPort: instance.unicastPort,
          multicastAddress: '239.192.74.71' // JTAG multicast group
        }
      },
      routing: {
        enableP2P: true,
        maxHops: 8,
        discoveryInterval: 30000
      },
      debugging: {
        enableVerboseLogging: (typeof process !== 'undefined' && process.env?.JTAG_VERBOSE === 'true') || false,
        testMode: (typeof process !== 'undefined' && process.env?.JTAG_TEST_MODE === 'true') || false
      }
    };

    SystemConfiguration._instance = new SystemConfiguration(config);
    return SystemConfiguration._instance;
  }

  /**
   * Get current system configuration instance
   */
  static getInstance(): SystemConfiguration {
    if (!SystemConfiguration._instance) {
      return SystemConfiguration.initialize();
    }
    return SystemConfiguration._instance;
  }

  /**
   * Get full configuration
   */
  getConfig(): JTAGSystemConfiguration {
    return this.config;
  }

  /**
   * Get instance configuration
   */
  getInstanceConfig(): JTAGInstanceConfig {
    return this.config.instance;
  }

  /**
   * Get WebSocket server port
   */
  getWebSocketPort(): number {
    return this.config.transport.websocket.serverPort;
  }

  /**
   * Get WebSocket client URL
   */
  getWebSocketUrl(): string {
    return this.config.transport.websocket.clientUrl;
  }

  /**
   * Get HTTP server port
   */
  getHTTPPort(): number {
    return this.config.transport.http.serverPort;
  }

  /**
   * Get HTTP base URL
   */
  getHTTPBaseUrl(): string {
    return this.config.transport.http.baseUrl;
  }

  /**
   * Get UDP multicast configuration
   */
  getUDPConfig(): { readonly port: number; readonly address: string; readonly unicastPort: number } {
    return {
      port: this.config.transport.udp.multicastPort,
      address: this.config.transport.udp.multicastAddress,
      unicastPort: this.config.transport.udp.unicastPort
    } as const;
  }

  /**
   * Check if running in test mode
   */
  isTestMode(): boolean {
    return this.config.debugging.testMode;
  }

  /**
   * Check if P2P networking is enabled
   */
  isP2PEnabled(): boolean {
    return this.config.routing.enableP2P;
  }

  /**
   * Get node ID
   */
  getNodeId(): string {
    return this.config.instance.nodeId;
  }

  /**
   * Get node capabilities
   */
  getCapabilities(): readonly string[] {
    return this.config.instance.capabilities;
  }

  /**
   * Update configuration (for testing) - creates new instance
   */
  updateConfig(updates: Partial<JTAGSystemConfiguration>): void {
    // Create new configuration object - cannot mutate readonly config
    const newConfig: JTAGSystemConfiguration = { 
      ...this.config, 
      ...updates 
    };
    
    // Replace with new instance (for testing only)
    (this as any).config = newConfig;
  }

  /**
   * Reset configuration (for testing)
   */
  static reset(): void {
    SystemConfiguration._instance = null;
  }

  /**
   * Create environment configuration from environment variables
   * Browser-safe implementation
   */
  static fromEnvironment(): JTAGInstanceConfig | null {
    // Browser doesn't have process.env
    if (typeof process === 'undefined' || !process.env) {
      return null;
    }
    
    const nodeId = process.env.JTAG_NODE_ID;
    const wsPort = process.env.JTAG_WS_PORT;
    const httpPort = process.env.JTAG_HTTP_PORT;
    
    if (!nodeId || !wsPort || !httpPort) {
      return null;
    }

    return {
      nodeId,
      wsPort: parseInt(wsPort, 10),
      httpPort: parseInt(httpPort, 10),
      multicastPort: parseInt(process.env.JTAG_MULTICAST_PORT || '37471', 10),
      unicastPort: parseInt(process.env.JTAG_UNICAST_PORT || '0', 10),
      nodeType: (process.env.JTAG_NODE_TYPE as any) || 'server',
      capabilities: process.env.JTAG_CAPABILITIES?.split(',') || []
    };
  }

  /**
   * Get default configuration with dynamic port assignment integration
   */
  static getDefaultConfig(): JTAGInstanceConfig {
    // Try environment first
    const envConfig = this.fromEnvironment();
    if (envConfig) {
      return envConfig;
    }

    // Try to get ports from running example's package.json
    if (typeof require !== 'undefined' && typeof process !== 'undefined') {
      try {
        // First try the current working directory (might be in an example)
        let pkg;
        try {
          pkg = require(process.cwd() + '/package.json');
        } catch {
          // Fall back to main package.json
          pkg = require('../../../../../../package.json');
        }
        
        const httpPort = pkg.config?.port || 9002;  
        const wsPort = httpPort - 1; // WebSocket is HTTP port - 1
        
        return {
          nodeId: 'default-node',
          wsPort: wsPort,
          httpPort: httpPort,
          multicastPort: 37471,
          unicastPort: wsPort + 1000,
          nodeType: 'server',
          capabilities: ['screenshot', 'file-operations']
        };
      } catch (error) {
        // Fall through to ultimate fallback
      }
    }

    // Ultimate fallback (works in browser and server)
    return {
      nodeId: 'fallback-node',
      wsPort: 9001,
      httpPort: 9002,
      multicastPort: 37471,
      unicastPort: 9003,
      nodeType: 'server',
      capabilities: []
    };
  }

  /**
   * Get configuration summary for logging
   */
  getSummary(): string {
    return `JTAG System Configuration:
  Node ID: ${this.config.instance.nodeId}
  WebSocket: ${this.config.transport.websocket.clientUrl}
  HTTP: ${this.config.transport.http.baseUrl}
  UDP: ${this.config.transport.udp.multicastAddress}:${this.config.transport.udp.multicastPort}
  P2P: ${this.config.routing.enableP2P ? 'enabled' : 'disabled'}
  Test Mode: ${this.config.debugging.testMode ? 'enabled' : 'disabled'}`;
  }
}

/**
 * Convenience function to get system configuration
 */
export function getSystemConfig(): SystemConfiguration {
  return SystemConfiguration.getInstance();
}

/**
 * Convenience function to get WebSocket URL
 */
export function getWebSocketUrl(): string {
  return getSystemConfig().getWebSocketUrl();
}

/**
 * Convenience function to get HTTP base URL
 */
export function getHTTPBaseUrl(): string {
  return getSystemConfig().getHTTPBaseUrl();
}