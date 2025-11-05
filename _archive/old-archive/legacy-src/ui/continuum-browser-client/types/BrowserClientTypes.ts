/**
 * Browser Client Specific Types
 * Core types for the browser client module only
 */

export type ContinuumState = 'initializing' | 'connecting' | 'connected' | 'ready' | 'error';

export interface ContinuumAPI {
  readonly version: string;
  readonly state: ContinuumState;
  sessionId: string | null;
  clientId: string | null;
  
  // Core methods
  isConnected(): boolean;
  execute(command: string, params?: Record<string, unknown>): Promise<CommandResult>;
  
  // Dynamic method attachment
  attachMethod(name: string, method: (...args: unknown[]) => unknown): void;
  hasMethod(name: string): boolean;
  
  // Lifecycle events
  onStateChange(callback: (state: ContinuumState) => void): void;
  onReady(callback: () => void): void;
}

export interface CommandResult {
  success: boolean;
  data?: Record<string, unknown>;
  error?: string;
}