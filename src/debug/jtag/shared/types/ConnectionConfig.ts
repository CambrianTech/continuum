/**
 * Connection Configuration - Shared types for clean config passing
 * 
 * This interface defines the connection configuration struct that gets:
 * 1. Created by example code (server-only config reading)
 * 2. Passed to API system (no config reading in API)
 * 3. Served to browser via clean HTTP endpoint
 * 4. Used everywhere (agents, health, transport, etc.)
 * 
 * CRITICAL: No config reading happens in API code - only in example code.
 */

/**
 * Complete connection configuration for a JTAG instance
 * Created once by example code, passed everywhere else
 */
export interface ConnectionConfig {
  /** WebSocket server port */
  readonly websocketPort: number;
  
  /** HTTP server port */  
  readonly httpPort: number;
  
  /** Working directory for this example */
  readonly workingDir: string;
  
  /** Example name (test-bench, widget-ui, etc.) */
  readonly exampleName: string;
  
  /** Optional multicast port for P2P discovery */
  readonly multicastPort?: number;
  
  /** Optional unicast port for P2P communication */
  readonly unicastPort?: number;
}

/**
 * Factory function signature for creating connection config
 * Implemented in example code only - never in API code
 */
export type ConnectionConfigFactory = () => ConnectionConfig;

/**
 * Validation result for connection configuration
 */
export interface ConnectionConfigValidation {
  readonly valid: boolean;
  readonly errors: readonly string[];
}

/**
 * Validate a connection configuration struct
 */
export function validateConnectionConfig(config: ConnectionConfig): ConnectionConfigValidation {
  const errors: string[] = [];
  
  if (!config.websocketPort || config.websocketPort < 1 || config.websocketPort > 65535) {
    errors.push('websocketPort must be a valid port number (1-65535)');
  }
  
  if (!config.httpPort || config.httpPort < 1 || config.httpPort > 65535) {
    errors.push('httpPort must be a valid port number (1-65535)');
  }
  
  if (!config.workingDir || typeof config.workingDir !== 'string') {
    errors.push('workingDir must be a non-empty string');
  }
  
  if (!config.exampleName || typeof config.exampleName !== 'string') {
    errors.push('exampleName must be a non-empty string');
  }
  
  if (config.websocketPort === config.httpPort) {
    errors.push('websocketPort and httpPort cannot be the same');
  }
  
  return {
    valid: errors.length === 0,
    errors
  };
}