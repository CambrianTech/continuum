/**
 * Process Registry Command Types - Shared
 * 
 * Provides process identification and cleanup commands for JTAG multi-instance coordination.
 * Enables P2P mesh networking by preventing process collision during startup/cleanup.
 */

import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

export type ProcessType = 'server' | 'browser' | 'test' | 'client';
export type ProcessCapability = 
  | 'websocket-server' 
  | 'command-execution' 
  | 'file-operations' 
  | 'console-logging'
  | 'screenshot' 
  | 'dom-interaction' 
  | 'browser-automation' 
  | 'console-forwarding'
  | 'test-execution' 
  | 'process-spawning' 
  | 'validation'
  | 'command-sending' 
  | 'result-receiving';

/**
 * Default port constants for process registry
 */
export const PROCESS_REGISTRY_PORTS = {
  DEFAULT_MULTICAST_PORT: 37471,
  DEFAULT_UNICAST_PORT_OFFSET: 1000
} as const;

/**
 * Default node identifiers
 */
export const DEFAULT_NODE_IDS = {
  DEFAULT_NODE: 'default-node'
} as const;

export interface ProcessRegistryEntry {
  readonly processId: string;        // Unique GUID for this process
  readonly nodeId: string;           // JTAG node identifier  
  readonly pid: number;              // System process ID
  readonly ports: readonly number[]; // Ports this process is using
  readonly startTime: number;        // Process start timestamp
  readonly processType: ProcessType;
  readonly description: string;      // Human-readable description
  readonly parentProcessId?: string; // Parent JTAG process (if any)
  readonly capabilities: readonly ProcessCapability[]; // What this process can do
}

/**
 * Register process parameters
 */
export interface RegisterProcessParams extends JTAGPayload {
  readonly processType: ProcessType;
  readonly description: string;
  readonly ports?: readonly number[];
  readonly capabilities?: readonly ProcessCapability[];
  readonly parentProcessId?: string;
}

/**
 * Register process result
 */
export interface RegisterProcessResult extends JTAGPayload {
  readonly success: boolean;
  readonly processId?: string;
  readonly error?: string;
}

/**
 * List processes parameters
 */
export interface ListProcessesParams extends JTAGPayload {
  readonly filterByPorts?: readonly number[];
  readonly filterByType?: ProcessType;
  readonly includeStale?: boolean;
}

/**
 * List processes result
 */
export interface ListProcessesResult extends JTAGPayload {
  readonly success: boolean;
  readonly processes: readonly ProcessRegistryEntry[];
  readonly error?: string;
}

/**
 * Cleanup processes parameters
 */
export interface CleanupProcessesParams extends JTAGPayload {
  readonly forceAll?: boolean;           // Kill all processes, ignore registry
  readonly preserveActive?: boolean;     // Preserve active JTAG processes
  readonly targetProcessId?: string;    // Only cleanup specific process
  readonly targetPorts?: readonly number[];      // Only cleanup specific ports
}

/**
 * Cleanup processes result
 */
export interface CleanupProcessesResult extends JTAGPayload {
  readonly success: boolean;
  readonly killedProcesses: readonly ProcessRegistryEntry[];
  readonly preservedProcesses: readonly ProcessRegistryEntry[];
  readonly cleanedPorts: readonly number[];
  readonly errors: readonly string[];
}

/**
 * Create register process parameters with defaults
 */
export function createRegisterProcessParams(
  context: JTAGContext,
  sessionId: UUID,
  processType: ProcessType,
  description: string,
  options: Partial<RegisterProcessParams> = {}
): RegisterProcessParams {
  return {
    context,
    sessionId,
    processType,
    description,
    ports: options.ports || [],
    capabilities: options.capabilities || [],
    parentProcessId: options.parentProcessId,
    ...options
  };
}

/**
 * Create list processes parameters with defaults
 */
export function createListProcessesParams(
  context: JTAGContext,
  sessionId: UUID,
  options: Partial<ListProcessesParams> = {}
): ListProcessesParams {
  return {
    context,
    sessionId,
    filterByPorts: options.filterByPorts,
    filterByType: options.filterByType,
    includeStale: options.includeStale ?? false,
    ...options
  };
}

/**
 * Create cleanup processes parameters with defaults
 */
export function createCleanupProcessesParams(
  context: JTAGContext,
  sessionId: UUID,
  options: Partial<CleanupProcessesParams> = {}
): CleanupProcessesParams {
  return {
    context,
    sessionId,
    forceAll: options.forceAll ?? false,
    preserveActive: options.preserveActive ?? true,
    targetProcessId: options.targetProcessId,
    targetPorts: options.targetPorts,
    ...options
  };
}