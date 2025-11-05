// ISSUES: 1 open, last updated 2025-07-14 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * 🔧 IMPROVEMENTS:
 * - [ ] Issue #1: Add SessionPaths and SessionStructure interfaces to ContinuumContext for better session directory management
 * - [ ] Issue #2: Implement UUID validation and generation utilities for sessionId consistency
 * - [ ] Issue #3: Refactor ContinuumContextFactory to use new UUID utilities
 * - [ ] Issue #4: Ensure all commands use the new ContinuumContext structure for session management
 * - [ ] Issue #5: Add comprehensive tests for ContinuumContextFactory and UUID utilities
 * - [ ] Issue #6: Document ContinuumContext structure and usage patterns for developers
 */

// Browser-compatible UUID type - flexible to support both UUIDs and string IDs
export type UUID = string;

// Browser-compatible UUID generation
function generateUUID(): UUID {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID() as UUID;
  } else {
    // Fallback for browser environments
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
      const r = Math.random() * 16 | 0;
      const v = c === 'x' ? r : (r & 0x3 | 0x8);
      return v.toString(16);
    }) as UUID;
  }
}

/**
 * Core execution environment type - fundamental to all Continuum operations
 * 
 * Environment types define where code execution originates and drives log file naming:
 * - 'browser': Human user interactions through web interface → browser.log
 * - 'server': Local server-side operations and daemons → server.log  
 * - 'remote': Commands from other machines (git hooks, remote clients) → remote.log
 * - 'agent': External AI users (agent-based users like Claude) → agent.log
 * - 'persona': Internal AI personas/identities within Continuum → persona.log
 */
export type ContinuumEnvironment = 'browser' | 'server' | 'remote' | 'agent' | 'persona';

/**
 * Stack frame for execution context tracking
 */
export interface ExecutionFrame {
  environment: ContinuumEnvironment;
  location: string; // Component, command, or daemon name
  description?: string; // Human-readable description of this execution frame
  timestamp: string;
  metadata?: Record<string, unknown>;
}

// Common types for context properties
export interface WebSocketServer {
  send: (message: unknown) => void;
  broadcast: (message: unknown) => void;
  clients: Set<unknown>;
}

/**
 * Session-based file paths following the established structure
 */
export interface SessionPaths {
  base: string;
  logs: string;
  screenshots: string;
  recordings: string;
  files: string;
  devtools: string;
}

export interface ContinuumContext {
  continuum?: ContinuumInstance;
  webSocketServer?: WebSocketServer;
  continuumStatus?: Record<string, unknown>;
  sessionId: UUID;
  userId?: string;
  // Multi-session support
  sessionType?: 'development' | 'production' | 'test';
  sessionOwner?: string;
  sessionStartTime?: string;
  // Environment context (current execution environment)
  environment?: ContinuumEnvironment;
  // Execution stack for call tracing
  executionStack?: ExecutionFrame[];
  // Session paths for consistent file operations
  sessionPaths?: SessionPaths;
  // Extensible for future context needs
  [key: string]: unknown;
}

export interface ContinuumInstance {
  version: string;
  config: Record<string, unknown>;
  daemons: Map<string, unknown>;
  context: ContinuumContext;
  // Multi-session support
  sessions?: Map<string, ContinuumContext>;
}

/**
 * Context factory utilities for managing ContinuumContext instances
 */
/**
 * UUID validation and generation utilities
 */
export const uuidValidator = {
  /**
   * Validate UUID format
   */
  validate: (uuid: string): uuid is UUID => {
    return typeof uuid === 'string' && 
           uuid.length === 36 && 
           /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(uuid);
  },
  
  /**
   * Generate a new UUID
   */
  generate: (): UUID => generateUUID(),
} as const;

export const continuumContextFactory = {
  /**
   * Create a new ContinuumContext with required fields
   */
  create: (options: {
    sessionId?: UUID;
    userId?: string;
    sessionType?: 'development' | 'production' | 'test';
    sessionOwner?: string;
    environment?: ContinuumEnvironment;
    [key: string]: unknown;
  } = {}): ContinuumContext => ({
    sessionId: options.sessionId ?? uuidValidator.generate(),
    sessionStartTime: new Date().toISOString(),
    ...options,
  }),

  /**
   * Merge contexts with proper precedence
   */
  merge: (base: ContinuumContext, override: Partial<ContinuumContext>): ContinuumContext => ({
    ...base,
    ...override,
    // Preserve critical fields
    sessionId: override.sessionId ?? base.sessionId,
  }),

  /**
   * Clone context with different environment
   * Preserves session, user, and other context but changes execution environment
   */
  withEnvironment: (context: ContinuumContext, environment: ContinuumEnvironment): ContinuumContext => ({
    ...context,
    environment,
  }),

  /**
   * Push new execution frame onto context stack
   * Creates proper call stack trace through system layers
   */
  push: (context: ContinuumContext, frame: {
    environment: ContinuumEnvironment;
    location: string;
    description?: string;
    metadata?: Record<string, unknown>;
  }): ContinuumContext => {
    const newFrame: ExecutionFrame = {
      ...frame,
      timestamp: new Date().toISOString(),
    };
    
    return {
      ...context,
      environment: frame.environment,
      executionStack: [...(context.executionStack || []), newFrame],
    };
  },

  /**
   * Pop execution frame from context stack
   * Returns to previous execution environment
   */
  pop: (context: ContinuumContext): ContinuumContext => {
    const stack = context.executionStack || [];
    if (stack.length === 0) {
      return context;
    }
    
    const newStack = stack.slice(0, -1);
    const previousEnvironment = newStack.length > 0 ? newStack[newStack.length - 1].environment : context.environment;
    
    const result: ContinuumContext = {
      ...context,
      executionStack: newStack,
    };
    
    if (previousEnvironment) {
      result.environment = previousEnvironment;
    }
    
    return result;
  },

  /**
   * Validate that a context has required fields
   */
  validate: (context: unknown): context is ContinuumContext => {
    if (typeof context !== 'object' || context === null) {
      return false;
    }
    
    const ctx = context as ContinuumContext;
    if (typeof ctx.sessionId !== 'string' || ctx.sessionId === null) {
      return false;
    }
    
    // Use the UUID validation utility
    return uuidValidator.validate(ctx.sessionId);
  },
} as const;