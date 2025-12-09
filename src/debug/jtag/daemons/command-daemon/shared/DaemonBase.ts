/**
 * Base Daemon - Abstract foundation for all JTAG daemons
 * 
 * Provides common daemon functionality while enforcing proper separation of concerns.
 * All daemons inherit from this to ensure consistent architecture.
 */

import { JTAGModule } from '../../../system/core/shared/JTAGModule';
import type { JTAGEnvironment, JTAGContext, JTAGMessage, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import {JTAGMessageFactory, useEnvironment} from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter, MessageSubscriber } from '../../../system/core/router/shared/JTAGRouter';
import { type RouterResult } from '../../../system/core/router/shared/RouterTypes';
import { type BaseResponsePayload } from '../../../system/core/types/ResponseTypes';

export interface DaemonEntry {
  name: string;
  className: string;
  daemonClass: new (...args: any[]) => DaemonBase;
}

/**
 * Simple logger interface for daemons
 * Browser-safe: No-op logger that doesn't spam console
 * Server overrides this with proper file-based Logger in server-side constructors
 */
interface DaemonLogger {
  info(message: string, ...args: any[]): void;
  warn(message: string, ...args: any[]): void;
  error(message: string, ...args: any[]): void;
  debug(message: string, ...args: any[]): void;
}

export abstract class DaemonBase extends JTAGModule implements MessageSubscriber {
  public readonly router: JTAGRouter;
  public abstract readonly subpath: string;
  public readonly uuid: string;
  protected log: DaemonLogger;

  // Resource management (Phase 1: Daemon lifecycle consistency)
  private unsubscribeFunctions: (() => void)[] = [];
  private intervals: Map<string, ReturnType<typeof setInterval>> = new Map();

  constructor(name: string, context: JTAGContext, router: JTAGRouter) {
    super(name, context);
    this.router = router;
    this.uuid = `${name}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;

    // Browser-safe no-op logger (prevents console spam)
    // Server-side subclasses MUST override this with Logger.create() in their constructor
    this.log = {
      info: () => {},
      warn: () => {},
      error: () => {},
      debug: () => {}
    };

    // Initialization will be called explicitly after construction completes
    // to ensure all subclass properties are properly initialized
  }

  /**
   * Initialize daemon - called explicitly after construction to allow subpath access
   */
  async initializeDaemon(): Promise<void> {
    // Register with router using subpath
    this.router.registerSubscriber(this.subpath, this);
    
    // Initialize daemon-specific functionality
    await this.initialize();
  }

  /**
   * Initialize daemon-specific functionality
   * Called automatically after construction
   */
  protected abstract initialize(): Promise<void>;

  /**
   * Handle incoming messages (MessageSubscriber interface)
   * Each daemon implements its own message handling logic
   */
  abstract handleMessage(message: JTAGMessage): Promise<BaseResponsePayload>;

  /**
   * Get endpoint (MessageSubscriber interface)
   */
  get endpoint(): string {
    return this.subpath;
  }


  protected createRequestMessage(endpoint:string, payload: JTAGPayload): JTAGMessage {
    return JTAGMessageFactory.createRequest(
      this.context,
      this.subpath,
      endpoint,
      payload
    );
  }


  /**
   * Execute remote daemon operation - universal cross-environment method
   * Allows any daemon to call operations on any other daemon
   */
  protected async executeRemote(message: JTAGMessage, environment: JTAGEnvironment): Promise<RouterResult> {
    message.endpoint = useEnvironment(message.endpoint, environment);
    this.log.info(`⚡ ${this.toString()}: Executing remote operation: ${message.endpoint}`);
    return this.router.postMessage(message);
  }

  /**
   * Register an event subscription for automatic cleanup
   * Use this when subscribing to events to ensure cleanup happens
   *
   * @example
   * const unsub = Events.subscribe('data:users:created', handler);
   * this.registerSubscription(unsub);
   */
  protected registerSubscription(unsubscribe: () => void): void {
    this.unsubscribeFunctions.push(unsubscribe);
  }

  /**
   * Unsubscribe from all registered events
   * Called automatically during shutdown
   */
  protected cleanupSubscriptions(): void {
    if (this.unsubscribeFunctions.length === 0) {
      return;
    }

    this.log.debug(`Cleaning up ${this.unsubscribeFunctions.length} event subscription(s)`);
    for (const unsub of this.unsubscribeFunctions) {
      try {
        unsub();
      } catch (error) {
        this.log.error(`Failed to unsubscribe:`, error);
      }
    }
    this.unsubscribeFunctions = [];
  }

  /**
   * Register a named interval for automatic cleanup
   * Automatically handles async errors in callback
   *
   * @example
   * this.registerInterval('monitoring', async () => {
   *   await this.monitorHealth();
   * }, 5000);
   */
  protected registerInterval(
    name: string,
    callback: () => void | Promise<void>,
    intervalMs: number
  ): void {
    // Clear existing interval with same name
    this.clearInterval(name);

    const interval = setInterval(() => {
      const result = callback();
      if (result instanceof Promise) {
        result.catch((error: Error) => {
          this.log.error(`Interval '${name}' error:`, error);
        });
      }
    }, intervalMs);

    this.intervals.set(name, interval);
    this.log.debug(`Registered interval '${name}' (${intervalMs}ms)`);
  }

  /**
   * Clear a specific named interval
   * Returns true if interval existed and was cleared
   */
  protected clearInterval(name: string): boolean {
    const interval = this.intervals.get(name);
    if (interval) {
      clearInterval(interval);
      this.intervals.delete(name);
      this.log.debug(`Cleared interval '${name}'`);
      return true;
    }
    return false;
  }

  /**
   * Clear all registered intervals
   * Called automatically during shutdown
   */
  protected cleanupIntervals(): void {
    if (this.intervals.size === 0) {
      return;
    }

    this.log.debug(`Cleaning up ${this.intervals.size} interval(s)`);
    for (const [name, interval] of this.intervals) {
      clearInterval(interval);
      this.log.debug(`Cleared interval '${name}'`);
    }
    this.intervals.clear();
  }

  /**
   * Wait for another daemon to be ready before executing callback
   * Useful for initialization dependencies (e.g., wait for DataDaemon)
   *
   * @example
   * this.onDaemonReady('data', async () => {
   *   await this.loadInitialData();
   * });
   */
  protected onDaemonReady(
    daemonName: string,
    callback: () => Promise<void>
  ): void {
    // Dynamic import Events to avoid circular dependency
    // In browser environment, this is no-op
    if (typeof window !== 'undefined') {
      return;
    }

    try {
      // Use require to avoid top-level import (DaemonBase is shared code)
      const { Events } = require('../../../system/core/shared/Events');

      const unsub = Events.subscribe('system:ready', async (payload: any) => {
        if (payload?.daemon === daemonName) {
          this.log.info(`📡 ${this.toString()}: ${daemonName} is ready`);
          try {
            await callback();
          } catch (error) {
            this.log.error(`Failed to handle ${daemonName} ready:`, error);
          }
        }
      });
      this.registerSubscription(unsub);
    } catch (error) {
      this.log.error(`Failed to setup onDaemonReady for ${daemonName}:`, error);
    }
  }

  /**
   * Defer execution until after initialization completes
   * Alternative to setTimeout with better logging and error handling
   *
   * @example
   * this.deferInitialization(async () => {
   *   await this.catchUpOnMissedEvents();
   * }, 2000);
   */
  protected deferInitialization(
    callback: () => Promise<void>,
    delayMs: number = 2000
  ): void {
    setTimeout(async () => {
      try {
        await callback();
      } catch (error) {
        this.log.error('Deferred initialization failed:', error);
      }
    }, delayMs);
  }

  /**
   * Override in subclasses for custom cleanup logic
   * Called before automatic cleanup in shutdown()
   *
   * @example
   * protected async cleanup(): Promise<void> {
   *   // Shutdown persona clients
   *   for (const client of this.clients.values()) {
   *     await client.shutdown();
   *   }
   * }
   */
  protected async cleanup(): Promise<void> {
    // Default: no-op (override in subclasses if needed)
  }

  /**
   * Cleanup resources when daemon shuts down
   * Automatically calls cleanupSubscriptions() and cleanupIntervals()
   * Override cleanup() for daemon-specific logic
   */
  async shutdown(): Promise<void> {
    this.log.info(`🔄 ${this.toString()}: Shutting down...`);

    // Call subclass cleanup first
    await this.cleanup();

    // Then automatic cleanup
    this.cleanupSubscriptions();
    this.cleanupIntervals();
  }
}