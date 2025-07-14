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

import { randomUUID, type UUID } from "crypto";

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
  // Environment context
  environment?: 'client' | 'server' | 'browser';
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
  generate: (): UUID => randomUUID(),
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
    environment?: 'client' | 'server' | 'browser';
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