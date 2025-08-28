// ISSUES: 0 open, last updated 2025-07-23 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * JTAG Universal Command Bus - Core Types
 * 
 * Foundation type system for the entire JTAG architecture. Defines the core
 * abstractions that enable cross-context, type-safe message routing.
 * 
 * CORE ARCHITECTURE:
 * - JTAGContext: Environment identification and module correlation
 * - JTAGPayload: Base class for all transportable data
 * - JTAGMessage: Complete message envelope with routing metadata
 * - Message Factory: Type-safe message construction patterns
 * - SessionUUID: Type-safe UUID validation for session accountability
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Type validation and serialization/deserialization
 * - Integration tests: Cross-context message transport
 * - Performance tests: Payload encoding/decoding efficiency
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Environment constants prevent string literal errors
 * - Abstract payload base ensures consistent serialization
 * - Message factory eliminates manual routing header construction
 * - UUID correlation enables distributed debugging across contexts
 */

import { type UUID } from './CrossPlatformUUID';

// Re-export UUID for use in other modules
export type { UUID };
import type { BaseResponsePayload } from './ResponseTypes';

/**
 * JTAG Environment Constants
 * 
 * Centralized environment identifiers to prevent string literal errors.
 * Used throughout the system for context-aware routing and module discovery.
 */
export const JTAG_ENVIRONMENTS = {
  SERVER: 'server',
  BROWSER: 'browser', 
  REMOTE: 'remote'
} as const;

export type JTAGEnvironment = typeof JTAG_ENVIRONMENTS[keyof typeof JTAG_ENVIRONMENTS];


/**
 * JTAG Context - Universal Module Identity with Secure Configuration
 * 
 * Shared context object that uniquely identifies execution environment.
 * All modules within the same context share this identity for correlation.
 * 
 * SECURITY: Configuration access is environment-aware:
 * - Server contexts get server configuration (with secrets)
 * - Client contexts get client-safe configuration (no secrets)
 * - Cross-contamination is prevented at the type level
 * 
 * @param uuid - Unique identifier for this context instance
 * @param environment - Execution environment (server/browser/remote)
 * @param getConfig - Environment-appropriate configuration accessor
 */
export interface JTAGContext {
  uuid: UUID;
  environment: JTAGEnvironment;
  readonly config: import('../../shared/SecureConfigTypes').JTAGConfig;
  getConfig(): JTAGContextConfig;
}

/**
 * Environment-specific configuration interface
 * Each environment gets only the configuration it needs
 */
export type JTAGContextConfig = 
  | { type: 'server'; config: import('../../shared/SecureConfigTypes').JTAGServerConfiguration }
  | { type: 'client'; config: import('../../shared/SecureConfigTypes').JTAGClientConfiguration }
  | { type: 'test'; config: import('../../shared/SecureConfigTypes').JTAGTestConfiguration };

export const extractEnvironment = (endpoint: string, parts?: string[]): JTAGEnvironment | undefined => {
    parts = parts ?? endpoint.split('/');
    return parts.length > 1 ? parts[0] as JTAGEnvironment : undefined;
}

export const useEnvironment = (endpoint: string, environment: JTAGEnvironment): string => {
  const parts = endpoint.split('/');
  const existingEnvironment = extractEnvironment(endpoint, parts);

  if (existingEnvironment) {
    parts[0] = environment; // Replace existing environment
  } else {
    parts.unshift(environment);
  }
  return `$${parts.join('/')}`;
}

/**
 * JTAG Payload Base Class
 * 
 * Abstract foundation for all data transported through the JTAG system.
 * Provides consistent serialization and cross-platform encoding capabilities.
 * 
 * UNIVERSAL CONTEXT & SESSION:
 * - Every payload carries routing context (environment identity)
 * - Every payload carries session identity (cross-system accountability)
 * - Enables distributed session management with automatic traceability
 * 
 * DESIGN PATTERN:
 * - All daemon responses extend this base
 * - Ensures type safety in message routing
 * - Cross-platform encoding (Node.js Buffer vs browser btoa)
 */
export interface JTAGPayload {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
}

/**
 * Functional factory for creating payloads - eliminates constructor complexity
 * Rust-like inheritance: creates payload from source + differences
 */
export const createPayload = <T>(
  context: JTAGContext,
  sessionId: UUID,
  data: T
): T & JTAGPayload => ({
  context,
  sessionId,
  ...data
});

/**
 * Transform payload: inherit from source + apply differences
 * Eliminates repetitive field copying - pure functional approach
 */
export const transformPayload = <TSource extends JTAGPayload, TTarget>(
  source: TSource,
  differences: TTarget
): TTarget & JTAGPayload => ({
  ...source,
  ...differences,
  context: source.context,
  sessionId: source.sessionId
});

/**
 * Cross-platform payload encoding for transport
 * Uses Node.js Buffer or browser btoa depending on environment
 */
export const encodePayload = (payload: JTAGPayload): string => {
  const jsonString = JSON.stringify(payload);
  if (typeof Buffer !== 'undefined') {
    return Buffer.from(jsonString, 'utf-8').toString('base64');
  } else {
    // Browser fallback
    return btoa(jsonString);
  }
};

/**
 * Cross-platform payload decoding from transport
 * Reverses encoding process with error handling for malformed data
 */
export const decodePayload = <T extends JTAGPayload>(encoded: string): T => {
  try {
    let jsonString: string;
    if (typeof Buffer !== 'undefined') {
      jsonString = Buffer.from(encoded, 'base64').toString('utf-8');
    } else {
      // Browser fallback
      jsonString = atob(encoded);
    }
    return JSON.parse(jsonString) as T;
  } catch (error) {
    throw new Error(`Failed to decode payload: ${error}`);
  }
};

/**
 * Check equality between two payloads
 */
export const payloadEquals = (payload1: JTAGPayload, payload2: JTAGPayload): boolean => {
  return payloadHashCode(payload1) === payloadHashCode(payload2);
};

/**
 * Generate hash code for payload equality comparison
 */
export const payloadHashCode = (payload: JTAGPayload): string => {
  const jsonString = JSON.stringify(payload);
  let hash = 0;
  for (let i = 0; i < jsonString.length; i++) {
    const char = jsonString.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash; // Convert to 32-bit integer
  }
  return hash.toString(16);
};

/**
 * Base JTAG Message interface
 */
export interface JTAGMessageBase<T extends JTAGPayload = JTAGPayload> {
  context: JTAGContext;
  origin: string;          // "route/from/and/subpaths"
  endpoint: string;        // "route/to/and/subpaths"
  payload: T;
  hashCode(): string;      // Required for deduplication
}

/**
 * Correlation interface - ensures request/response messages have unique correlation IDs
 */
export interface ICorrelation {
  readonly correlationId: UUID;
}

/**
 * Fire-and-forget event message (no response expected)
 */
export interface JTAGEventMessage<T extends JTAGPayload = JTAGPayload> extends JTAGMessageBase<T> {
  readonly messageType: 'event';
}

/**
 * Request message (expects a response) - automatically generates correlationId
 */
export interface JTAGRequestMessage<T extends JTAGPayload = JTAGPayload> extends JTAGMessageBase<T>, ICorrelation {
  readonly messageType: 'request';
  readonly correlationId: UUID;
}

/**
 * Response message (response to a request) - inherits correlationId from request
 */
export interface JTAGResponseMessage<T extends JTAGPayload = JTAGPayload> extends JTAGMessageBase<T>, ICorrelation {
  readonly messageType: 'response';
  readonly request: JTAGRequestMessage<T>; // Reference to the original request message
  readonly correlationId: UUID; // Must match request.correlationId
}

/**
 * Union type for all JTAG messages
 */
export type JTAGMessage<T extends JTAGPayload = JTAGPayload> = 
  | JTAGEventMessage<T>
  | JTAGRequestMessage<T>
  | JTAGResponseMessage<T>;

/**
 * Type guards for message types
 */
export const JTAGMessageTypes = {
  isEvent: <T extends JTAGPayload>(message: JTAGMessage<T>): message is JTAGEventMessage<T> => {
    return message.messageType === 'event';
  },
  
  isRequest: <T extends JTAGPayload>(message: JTAGMessage<T>): message is JTAGRequestMessage<T> => {
    return message.messageType === 'request';
  },
  
  isResponse: <T extends JTAGPayload>(message: JTAGMessage<T>): message is JTAGResponseMessage<T> => {
    return message.messageType === 'response';
  }
};

// Specialized type guard for response messages with JTAGResponsePayload
export const isJTAGResponseMessage = (message: JTAGMessage): message is JTAGResponseMessage<import('./ResponseTypes').JTAGResponsePayload> => {
  return message.messageType === 'response';
};

/**
 * Message factory functions
 */
export class JTAGMessageFactory {
  /**
   * Create an event message (fire-and-forget)
   */
  static createEvent<T extends JTAGPayload>(
    context: JTAGContext,
    origin: string,
    endpoint: string,
    payload: T
  ): JTAGEventMessage<T> {
    const message = {
      messageType: 'event' as const,
      context,
      origin,
      endpoint,
      payload,
      hashCode(): string {
        return JTAGMessageUtils.generateMessageHash(this);
      }
    };
    return message;
  }

  /**
   * Create a request message (expects response)
   */
  static createRequest<T extends JTAGPayload>(
    context: JTAGContext,
    origin: string,
    endpoint: string,
    payload: T,
    correlationId?: string
  ): JTAGRequestMessage<T> {
    const message = {
      messageType: 'request' as const,
      context,
      origin,
      endpoint,
      payload,
      correlationId: correlationId ?? JTAGMessageFactory.generateCorrelationId(),
      hashCode(): string {
        return JTAGMessageUtils.generateMessageHash(this);
      }
    };
    return message;
  }

  /**
   * Create a response message - automatically inherits correlationId from request
   */
  static createResponse<T extends JTAGPayload>(
    context: JTAGContext,
    origin: string,
    endpoint: string,
    payload: T,
    request: JTAGRequestMessage<T>
  ): JTAGResponseMessage<T> {
    const message = {
      messageType: 'response' as const,
      context,
      origin,
      endpoint,
      payload,
      request,
      correlationId: request.correlationId, // Automatically inherit from request
      hashCode(): string {
        return JTAGMessageUtils.generateMessageHash(this);
      }
    };
    return message;
  }


  /**
   * Generate unique correlation ID
   */
  static generateCorrelationId(): string {
    return `corr_${Date.now()}_${Math.random().toString(36).substr(2, 8)}`;
  }
};

/**
 * Message utilities for deduplication (Kotlin-style)
 */
export class JTAGMessageUtils {
  /**
   * Generate hash code for a message instance (used by message.hashCode())
   */
  static generateMessageHash(message: JTAGMessage): string {
    return JTAGMessageUtils.hashCode(message);
  }

  /**
   * Generate hash code for a message (for deduplication)
   */
  static hashCode(message: JTAGMessage): string {
    // Hash based on origin, endpoint, and payload content (not context.uuid to allow deduplication across contexts)
    const content = JSON.stringify({
      origin: message.origin,
      endpoint: message.endpoint,
      payloadHash: payloadHashCode(message.payload)
    });
    
    return JTAGMessageUtils.simpleHash(content);
  }

  /**
   * Check if two messages are equal (for deduplication)
   */
  static equals(msg1: JTAGMessage, msg2: JTAGMessage): boolean {
    return JTAGMessageUtils.hashCode(msg1) === JTAGMessageUtils.hashCode(msg2);
  }

  /**
   * Simple hash function for string content
   */
  private static simpleHash(str: string): string {
    let hash = 0;
    if (str.length === 0) return '0';
    
    for (let i = 0; i < str.length; i++) {
      const char = str.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash & hash; // Convert to 32-bit integer
    }
    
    return Math.abs(hash).toString(36);
  }
}

/**
 * Command Parameters interface
 */
export interface CommandParams extends JTAGPayload {
  // Base command parameters - specific commands add specific fields
}

/**
 * Command Result interface  
 */
export interface CommandResult extends JTAGPayload {
  // Base command results - specific commands add specific fields
  // Note: Some commands extend BaseResponsePayload for standardized success/timestamp
}

/**
 * Command Message type
 */
export type CommandMessage<T extends CommandParams = CommandParams> = JTAGMessage<T>;

/**
 * Session and context propagation through explicit payload parameters
 * No global state - everything flows through payload chain
 */