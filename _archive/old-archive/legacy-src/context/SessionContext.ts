/**
 * Session Context Management with AsyncLocalStorage + Semaphore
 * 
 * Provides thread-safe ContinuumContext tracking that automatically propagates
 * through async call chains. Used by UniversalLogger for session-aware
 * console.log routing.
 */

import { AsyncLocalStorage } from 'async_hooks';
import type { ContinuumContext } from '../types/shared/core/ContinuumTypes';

export class SessionContext {
  private static contextStore = new AsyncLocalStorage<ContinuumContext>();
  private static isLocked = false;
  private static lockQueue: Array<() => void> = [];

  /**
   * Simple lock mechanism for thread-safe operations
   */
  private static async acquireLock(): Promise<void> {
    if (!this.isLocked) {
      this.isLocked = true;
      return;
    }
    
    return new Promise((resolve) => {
      this.lockQueue.push(resolve);
    });
  }

  private static releaseLock(): void {
    if (this.lockQueue.length > 0) {
      const next = this.lockQueue.shift();
      if (next) next();
    } else {
      this.isLocked = false;
    }
  }

  /**
   * Run function with ContinuumContext
   * All async operations within this function will inherit the context
   */
  static async withContext<T>(context: ContinuumContext, fn: () => Promise<T>): Promise<T> {
    await this.acquireLock();
    try {
      return this.contextStore.run(context, fn);
    } finally {
      this.releaseLock();
    }
  }

  /**
   * Run function with ContinuumContext (synchronous version)
   * For simple operations that don't need lock protection
   */
  static withContextSync<T>(context: ContinuumContext, fn: () => T): T {
    return this.contextStore.run(context, fn);
  }

  /**
   * Get current ContinuumContext (lock protected)
   * Use this for operations that need guaranteed consistency
   */
  static async getCurrentContext(): Promise<ContinuumContext | null> {
    await this.acquireLock();
    try {
      return this.contextStore.getStore() || null;
    } finally {
      this.releaseLock();
    }
  }

  /**
   * Get current ContinuumContext (synchronous version)
   * Fast access for console overrides and performance-critical paths
   */
  static getCurrentContextSync(): ContinuumContext | null {
    return this.contextStore.getStore() || null;
  }

  /**
   * Get current session ID (convenience method)
   */
  static getCurrentSessionSync(): string | null {
    const context = this.getCurrentContextSync();
    return context?.sessionId || null;
  }

  /**
   * Set context for ContinuumContext-based operations
   * Convenience method for command system integration
   */
  static async withContinuumContext<T>(context: ContinuumContext | undefined, fn: () => Promise<T>): Promise<T> {
    if (context) {
      return await this.withContext(context, fn);
    } else {
      return await fn();
    }
  }

  /**
   * Debug: Get current context info
   */
  static getContextInfo(): { sessionId: string | null; hasContext: boolean; context: ContinuumContext | null } {
    const context = this.getCurrentContextSync();
    return {
      sessionId: context?.sessionId || null,
      hasContext: context !== null,
      context
    };
  }
}