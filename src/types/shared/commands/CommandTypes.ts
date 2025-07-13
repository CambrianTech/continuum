/**
 * Command Types - Universal command system interfaces
 * 
 * Shared types for command definitions, execution, and results
 */

import { UUID } from '../core/UserPersona';

export enum CommandCategory {
  CORE = 'core',
  COMMUNICATION = 'communication',
  DEVELOPMENT = 'development',
  SYSTEM = 'system',
  AI = 'ai',
  FILE = 'file',
  MONITORING = 'monitoring'
}

export enum ParameterType {
  STRING = 'string',
  NUMBER = 'number',
  BOOLEAN = 'boolean',
  OBJECT = 'object',
  ARRAY = 'array',
  UUID = 'uuid'
}

export interface CommandParameter {
  readonly type: ParameterType;
  readonly description: string;
  readonly required: boolean;
  readonly defaultValue?: unknown;
  readonly validation?: {
    readonly pattern?: string;
    readonly min?: number;
    readonly max?: number;
    readonly enum?: readonly string[];
  };
}

export interface CommandDefinition {
  readonly name: string;
  readonly description: string;
  readonly category: CommandCategory;
  readonly parameters: Record<string, CommandParameter>;
  readonly examples: ReadonlyArray<{
    readonly description: string;
    readonly command: string;
  }>;
  readonly permissions?: readonly string[];
  readonly deprecated?: boolean;
}

export interface CommandContext {
  readonly sessionId: UUID;
  readonly userPersonaId: UUID;
  readonly connectionId?: string;
  readonly requestId: UUID;
  readonly timestamp: number;
  readonly permissions: readonly string[];
}

export interface CommandResult<T = unknown> {
  readonly success: boolean;
  readonly data?: T;
  readonly error?: string;
  readonly warnings?: readonly string[];
  readonly metadata?: Record<string, unknown>;
  readonly executionTime?: number;
}

export interface CommandExecution {
  readonly command: string;
  readonly parameters: Record<string, unknown>;
  readonly context: CommandContext;
  readonly startTime: Date;
  readonly endTime?: Date;
  readonly result?: CommandResult;
}