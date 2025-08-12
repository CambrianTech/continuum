/**
 * MessageTypeGuards - Type-safe payload validation and type guards
 * 
 * Eliminates `any` types by providing proper type guards for message payloads.
 * Used by MessagePriorityStrategy and other systems that need to inspect payloads.
 */

import type { JTAGPayload } from '../../types/JTAGTypes';
import { TypeUtilities } from '../../types/TypeUtilities';

/**
 * Console message payload types
 */
export interface ConsoleMessagePayload extends JTAGPayload {
  readonly level: 'error' | 'warn' | 'info' | 'debug' | 'log';
  readonly message: string;
  readonly timestamp?: string;
  readonly additionalContext?: string;
}

/**
 * Health message payload types
 */
export interface HealthMessagePayload extends JTAGPayload {
  readonly status: 'healthy' | 'unhealthy' | 'connecting' | 'disconnected';
  readonly latency?: number;
  readonly error?: string;
}

/**
 * Error message payload types
 */
export interface ErrorMessagePayload extends JTAGPayload {
  readonly error: string | Error;
  readonly code?: string | number;
  readonly stack?: string;
  readonly errorContext?: Record<string, unknown>;
}

/**
 * System message payload types
 */
export interface SystemMessagePayload extends JTAGPayload {
  readonly type: 'startup' | 'shutdown' | 'restart' | 'alert';
  readonly message: string;
  readonly severity?: 'low' | 'medium' | 'high' | 'critical';
}

/**
 * Type guards for message payloads
 */
export class MessageTypeGuards {
  
  /**
   * Check if payload is a console message
   */
  static isConsolePayload(payload: unknown): payload is ConsoleMessagePayload {
    if (!TypeUtilities.hasProperties(payload, ['level', 'message'])) {
      return false;
    }
    
    return (
      typeof payload.level === 'string' &&
      typeof payload.message === 'string' &&
      ['error', 'warn', 'info', 'debug', 'log'].includes(payload.level)
    );
  }

  /**
   * Check if console payload is error level
   */
  static isConsoleError(payload: unknown): payload is ConsoleMessagePayload & { level: 'error' } {
    return this.isConsolePayload(payload) && payload.level === 'error';
  }

  /**
   * Check if console payload is warning level
   */
  static isConsoleWarn(payload: unknown): payload is ConsoleMessagePayload & { level: 'warn' } {
    return this.isConsolePayload(payload) && payload.level === 'warn';
  }

  /**
   * Check if console payload matches specific level
   */
  static isConsoleLevel(payload: unknown, level: ConsoleMessagePayload['level']): payload is ConsoleMessagePayload {
    return this.isConsolePayload(payload) && payload.level === level;
  }

  /**
   * Check if payload is a health message
   */
  static isHealthPayload(payload: unknown): payload is HealthMessagePayload {
    if (!TypeUtilities.hasProperty(payload, 'status')) {
      return false;
    }
    
    return (
      typeof payload.status === 'string' &&
      ['healthy', 'unhealthy', 'connecting', 'disconnected'].includes(payload.status)
    );
  }

  /**
   * Check if payload is an error message
   */
  static isErrorPayload(payload: unknown): payload is ErrorMessagePayload {
    if (!TypeUtilities.hasProperty(payload, 'error')) {
      return false;
    }
    
    return (typeof payload.error === 'string' || payload.error instanceof Error);
  }

  /**
   * Check if payload is a system message
   */
  static isSystemPayload(payload: unknown): payload is SystemMessagePayload {
    if (!TypeUtilities.hasProperties(payload, ['type', 'message'])) {
      return false;
    }
    
    return (
      typeof payload.type === 'string' &&
      typeof payload.message === 'string' &&
      ['startup', 'shutdown', 'restart', 'alert'].includes(payload.type)
    );
  }

  /**
   * Check if system payload is critical
   */
  static isCriticalSystemPayload(payload: unknown): payload is SystemMessagePayload & { severity: 'critical' } {
    return this.isSystemPayload(payload) && payload.severity === 'critical';
  }

  /**
   * Extract console level safely
   */
  static getConsoleLevel(payload: unknown): ConsoleMessagePayload['level'] | null {
    return this.isConsolePayload(payload) ? payload.level : null;
  }

  /**
   * Extract error message safely
   */
  static getErrorMessage(payload: unknown): string | null {
    if (this.isErrorPayload(payload)) {
      return payload.error instanceof Error ? payload.error.message : payload.error;
    }
    return null;
  }

  /**
   * Extract health status safely
   */
  static getHealthStatus(payload: unknown): HealthMessagePayload['status'] | null {
    return this.isHealthPayload(payload) ? payload.status : null;
  }

  /**
   * Extract system message type safely
   */
  static getSystemType(payload: unknown): SystemMessagePayload['type'] | null {
    return this.isSystemPayload(payload) ? payload.type : null;
  }

  /**
   * Check if payload has a specific property with type safety
   */
  static hasProperty<K extends string>(
    payload: unknown,
    property: K
  ): payload is Record<K, unknown> {
    return payload !== null && typeof payload === 'object' && property in payload;
  }

  /**
   * Get property value with type safety
   */
  static getProperty<K extends string, T>(
    payload: unknown,
    property: K,
    validator: (value: unknown) => value is T
  ): T | null {
    if (this.hasProperty(payload, property)) {
      const value = (payload as Record<K, unknown>)[property];
      return validator(value) ? value : null;
    }
    return null;
  }
}