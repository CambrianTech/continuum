/**
 * Signaling Configuration - Clean dependency injection
 * 
 * The signaling system should be TOLD what to monitor, not discover it.
 * This interface defines what the system needs to be passed, not what it should read.
 * 
 * ARCHITECTURE PRINCIPLE: API systems don't read app-specific config - they receive it.
 */

/**
 * Configuration for what the signaling system should monitor
 * Created by the calling system (examples, tests, etc.) and passed in
 */
export interface SignalingConfig {
  /** Ports that should be monitored for readiness */
  readonly portsToMonitor: {
    readonly websocketPort: number;
    readonly httpPort: number;
    readonly multicastPort?: number;
  };
  
  /** Paths where the signaling system should write signals */
  readonly signalPaths: {
    readonly signalDirectory: string;
    readonly logDirectory: string;
    readonly screenshotDirectory?: string;
  };
  
  /** Instance identification for multi-instance scenarios */
  readonly instanceConfig: {
    readonly instanceId: string;
    readonly instanceName: string;
    readonly workingDirectory: string;
  };
  
  /** What to consider as "system ready" */
  readonly readinessConfig: {
    readonly minCommandCount: number;
    readonly timeoutMs: number;
    readonly requiredServices: readonly string[];
  };
}

/**
 * Factory function signature - examples provide this, API uses it
 * This maintains the separation: examples know their config, API doesn't
 */
export type SignalingConfigFactory = () => SignalingConfig;

/**
 * Validate signaling configuration
 */
export function validateSignalingConfig(config: SignalingConfig): { valid: boolean; errors: string[] } {
  const errors: string[] = [];
  
  if (!config.portsToMonitor.websocketPort || config.portsToMonitor.websocketPort < 1) {
    errors.push('websocketPort must be a valid port number');
  }
  
  if (!config.portsToMonitor.httpPort || config.portsToMonitor.httpPort < 1) {
    errors.push('httpPort must be a valid port number');
  }
  
  if (!config.signalPaths.signalDirectory) {
    errors.push('signalDirectory path is required');
  }
  
  if (!config.signalPaths.logDirectory) {
    errors.push('logDirectory path is required');
  }
  
  if (!config.instanceConfig.instanceId) {
    errors.push('instanceId is required');
  }
  
  if (!config.instanceConfig.instanceName) {
    errors.push('instanceName is required');
  }
  
  return {
    valid: errors.length === 0,
    errors
  };
}