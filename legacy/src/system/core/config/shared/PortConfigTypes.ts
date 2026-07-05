/**
 * Port Configuration Types - Shared interfaces for cross-platform port management
 * 
 * Contains only types and interfaces - no Node.js-specific imports.
 * Safe for browser and server environments.
 */

/**
 * Configuration for a JTAG instance with dynamic ports
 */
export interface JTAGInstanceConfig {
  readonly nodeId: string;
  readonly wsPort: number;
  readonly httpPort: number;
  readonly multicastPort: number;
  readonly unicastPort: number;
  readonly nodeType: 'server' | 'browser' | 'test';
  readonly capabilities: string[];
}

/**
 * Port range configuration for dynamic allocation
 */
export interface PortRangeConfig {
  readonly startPort: number;
  readonly endPort: number;
  readonly reservedPorts: readonly number[];
}

/**
 * Port availability check result
 */
export interface PortAvailabilityResult {
  readonly port: number;
  readonly available: boolean;
  readonly error?: string;
}

/**
 * Environment variables for instance configuration
 */
export interface InstanceEnvironmentVars {
  readonly JTAG_NODE_ID: string;
  readonly JTAG_WS_PORT: string;
  readonly JTAG_HTTP_PORT: string;
  readonly JTAG_MULTICAST_PORT: string;
  readonly JTAG_UNICAST_PORT: string;
  readonly JTAG_NODE_TYPE: string;
  readonly JTAG_CAPABILITIES: string;
  readonly JTAG_TEST_MODE: string;
}

/**
 * Default port ranges for different environments
 */
export const PORT_RANGES = {
  DEVELOPMENT: { startPort: 9000, endPort: 9099, reservedPorts: [9000, 9001] }, // HTTP_PORT and WS_PORT from config.env
  TESTING: { startPort: 9100, endPort: 9299, reservedPorts: [] },
  PRODUCTION: { startPort: 8000, endPort: 8099, reservedPorts: [] },
  P2P_TESTING: { startPort: 9300, endPort: 9599, reservedPorts: [] }
} as const;

/**
 * Port configuration validation result
 */
export interface PortConfigValidation {
  readonly valid: boolean;
  readonly errors: readonly string[];
}

/**
 * Port manager status
 */
export interface PortManagerStatus {
  readonly allocated: number;
  readonly instances: number;
  readonly portRange: PortRangeConfig;
}