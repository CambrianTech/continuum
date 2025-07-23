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
 * JTAG Context - Universal Module Identity
 * 
 * Shared context object that uniquely identifies execution environment.
 * All modules within the same context share this identity for correlation.
 * 
 * @param uuid - Unique identifier for this context instance
 * @param environment - Execution environment (server/browser/remote)
 */
export interface JTAGContext {
  uuid: string;
  environment: JTAGEnvironment;
}

/**
 * JTAG Payload Base Class
 * 
 * Abstract foundation for all data transported through the JTAG system.
 * Provides consistent serialization and cross-platform encoding capabilities.
 * 
 * DESIGN PATTERN:
 * - All daemon responses extend this base
 * - Ensures type safety in message routing
 * - Cross-platform encoding (Node.js Buffer vs browser btoa)
 */
export abstract class JTAGPayload {
  /**
   * Cross-platform payload encoding for transport
   * Uses Node.js Buffer or browser btoa depending on environment
   */
  encode(): string {
    const jsonString = JSON.stringify(this);
    if (typeof Buffer !== 'undefined') {
      return Buffer.from(jsonString, 'utf-8').toString('base64');
    } else {
      // Browser fallback
      return btoa(jsonString);
    }
  }

  /**
   * Cross-platform payload decoding from transport
   * Reverses encoding process with error handling for malformed data
   */
  static decode<T extends JTAGPayload>(this: new() => T, encoded: string): T {
    try {
      let jsonString: string;
      if (typeof Buffer !== 'undefined') {
        jsonString = Buffer.from(encoded, 'base64').toString('utf-8');
      } else {
        // Browser fallback
        jsonString = atob(encoded);
      }
      const data = JSON.parse(jsonString);
      const instance = new this();
      Object.assign(instance, data);
      return instance;
    } catch (error) {
      throw new Error(`Failed to decode ${this.name}: ${error}`);
    }
  }

  /**
   * Check equality with another payload
   */
  equals(other: JTAGPayload): boolean {
    return this.hashCode() === other.hashCode();
  }

  /**
   * Generate hash code for equality comparison
   */
  hashCode(): string {
    const jsonString = JSON.stringify(this);
    let hash = 0;
    for (let i = 0; i < jsonString.length; i++) {
      const char = jsonString.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash & hash; // Convert to 32-bit integer
    }
    return hash.toString(16);
  }
}

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
 * Fire-and-forget event message (no response expected)
 */
export interface JTAGEventMessage<T extends JTAGPayload = JTAGPayload> extends JTAGMessageBase<T> {
  readonly messageType: 'event';
}

/**
 * Request message (expects a response)
 */
export interface JTAGRequestMessage<T extends JTAGPayload = JTAGPayload> extends JTAGMessageBase<T> {
  readonly messageType: 'request';
  readonly correlationId: string;
}

/**
 * Response message (response to a request)
 */
export interface JTAGResponseMessage<T extends JTAGPayload = JTAGPayload> extends JTAGMessageBase<T> {
  readonly messageType: 'response';
  readonly correlationId: string;
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
    correlationId: string
  ): JTAGRequestMessage<T> {
    const message = {
      messageType: 'request' as const,
      context,
      origin,
      endpoint,
      payload,
      correlationId,
      hashCode(): string {
        return JTAGMessageUtils.generateMessageHash(this);
      }
    };
    return message;
  }

  /**
   * Create a response message
   */
  static createResponse<T extends JTAGPayload>(
    context: JTAGContext,
    origin: string,
    endpoint: string,
    payload: T,
    correlationId: string
  ): JTAGResponseMessage<T> {
    const message = {
      messageType: 'response' as const,
      context,
      origin,
      endpoint,
      payload,
      correlationId,
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
      payloadHash: message.payload.hashCode()
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
 * Command Parameters base class
 */
export abstract class CommandParams extends JTAGPayload {
  // Base command parameters - subclasses add specific fields
}

/**
 * Command Message type
 */
export type CommandMessage<T extends CommandParams = CommandParams> = JTAGMessage<T>;

/**
 * Screenshot Command Parameters
 */
export class ScreenshotParams extends CommandParams {
  filename: string;
  selector?: string;
  options?: {
    width?: number;
    height?: number;
    fullPage?: boolean;
    quality?: number;
    format?: 'png' | 'jpeg' | 'webp';
    delay?: number;
  };

  constructor(filename: string, selector?: string, options?: ScreenshotParams['options']) {
    super();
    this.filename = filename;
    this.selector = selector;
    this.options = options;
  }
}

/**
 * Screenshot Command Result
 */
export class ScreenshotResult extends JTAGPayload {
  success: boolean;
  filepath: string;
  filename: string;
  environment: JTAGEnvironment;
  timestamp: string;
  options?: ScreenshotParams['options'];
  error?: string;
  metadata?: {
    width: number;
    height: number;
    size: number;
    selector?: string;
  };

  constructor(data: Partial<ScreenshotResult>) {
    super();
    this.success = data.success ?? false;
    this.filepath = data.filepath ?? '';
    this.filename = data.filename ?? '';
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
    this.options = data.options;
    this.error = data.error;
    this.metadata = data.metadata;
  }
}