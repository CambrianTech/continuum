import { randomUUID, type UUID } from "crypto";

// Common types for context properties
export interface WebSocketServer {
  send: (message: unknown) => void;
  broadcast: (message: unknown) => void;
  clients: Set<unknown>;
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
    sessionId: options.sessionId ?? randomUUID(),
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
    
    // Validate UUID format - UUID is a string with specific format
    try {
      const uuid = ctx.sessionId;
      // UUID v4 format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
      return typeof uuid === 'string' && 
             uuid.length === 36 && 
             /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(uuid);
    } catch {
      return false;
    }
  },
} as const;