/**
 * Base Command Types - Shared type definitions for commands
 */

export type { CommandDefinition, CommandResult, ContinuumContext } from './BaseCommand';

// Additional shared types for command implementations
export interface ValidationResult {
  valid: boolean;
  missing: string[];
}

export interface MessageResponse {
  success: boolean;
  data?: unknown;
  error?: string;
  total?: number;
}

export interface SessionFilter {
  owner?: string;
  starter?: string;
  type?: string;
  active?: boolean;
  user?: string;
}

export interface ConnectionIdentity {
  starter: string;
  identity: {
    name?: string;
    user?: string;
    project?: string;
    branch?: string;
  };
}