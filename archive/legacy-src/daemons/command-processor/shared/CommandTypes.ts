/**
 * Shared Command Types - Universal command interfaces for symmetric daemon architecture
 * 
 * These types are used across both server and client command processing daemons,
 * enabling a unified mental model and consistent development patterns.
 * 
 * Used by:
 * - src/daemons/command-processor/server/ (Node.js daemon)
 * - src/daemons/command-processor/client/ (Browser daemon) 
 * - src/ui/browser/daemons/CommandDaemon.ts (existing browser implementation)
 */

// ✅ STRONGLY TYPED COMMAND REQUEST
export interface TypedCommandRequest<T = unknown> {
  readonly command: string;
  readonly parameters: T;
  readonly context: Record<string, any>;
  readonly continuumContext?: any;
  readonly routing?: CommandRouting;
}

// ✅ COMMAND ROUTING AND EXECUTION STRATEGY
export interface CommandRouting {
  readonly preferredProvider: 'browser' | 'python' | 'cloud' | 'mesh' | 'auto';
  readonly fallbackAllowed: boolean;
  readonly meshDistribution: boolean;
  readonly qualityRequirement: 'fast' | 'balanced' | 'accurate';
}

// ✅ COMMAND IMPLEMENTATION METADATA
export interface CommandImplementation {
  readonly name: string;
  readonly provider: 'browser' | 'python' | 'cloud' | 'mesh';
  readonly status: 'available' | 'degraded' | 'unavailable';
  readonly quality: 'basic' | 'standard' | 'premium';
  readonly cost: CommandCost;
  readonly capabilities: readonly string[];
}

// ✅ COMMAND COST STRUCTURE
export interface CommandCost {
  readonly type: 'free' | 'per_execution' | 'per_minute' | 'subscription';
  readonly amount: number;
  readonly currency: string;
}

// ✅ COMMAND EXECUTION TRACKING
export interface CommandExecution<T = unknown, R = unknown> {
  readonly id: string;
  readonly command: string;
  readonly parameters: T;
  readonly implementation: CommandImplementation;
  readonly startTime: Date;
  readonly status: 'pending' | 'running' | 'completed' | 'failed';
  readonly result?: R;
  readonly error?: string;
  readonly executionTime?: number;
}

// ✅ COMMAND EXECUTION STATES
export type CommandExecutionStatus = 'pending' | 'running' | 'completed' | 'failed';

// ✅ COMMAND PROVIDER TYPES
export type CommandProvider = 'browser' | 'python' | 'cloud' | 'mesh';

// ✅ COMMAND QUALITY LEVELS  
export type CommandQuality = 'basic' | 'standard' | 'premium';

// ✅ COMMAND EXECUTION FACTORIES
export class CommandExecutionFactory {
  static create<T = unknown>(
    command: string,
    parameters: T,
    implementation: CommandImplementation
  ): CommandExecution<T> {
    return {
      id: `exec-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      command,
      parameters,
      implementation,
      startTime: new Date(),
      status: 'pending'
    };
  }

  static fromRequest<T = unknown>(
    request: TypedCommandRequest<T>,
    implementation: CommandImplementation
  ): CommandExecution<T> {
    return CommandExecutionFactory.create(
      request.command,
      request.parameters,
      implementation
    );
  }
}

// ✅ TYPE GUARDS FOR RUNTIME VALIDATION
export function isTypedCommandRequest(obj: unknown): obj is TypedCommandRequest {
  return typeof obj === 'object' && obj !== null &&
    typeof (obj as any).command === 'string' &&
    typeof (obj as any).parameters !== 'undefined';
}

export function isCommandExecution(obj: unknown): obj is CommandExecution {
  return typeof obj === 'object' && obj !== null &&
    typeof (obj as any).id === 'string' &&
    typeof (obj as any).command === 'string' &&
    typeof (obj as any).status === 'string' &&
    ['pending', 'running', 'completed', 'failed'].includes((obj as any).status);
}

export function isCommandImplementation(obj: unknown): obj is CommandImplementation {
  return typeof obj === 'object' && obj !== null &&
    typeof (obj as any).name === 'string' &&
    typeof (obj as any).provider === 'string' &&
    ['browser', 'python', 'cloud', 'mesh'].includes((obj as any).provider);
}