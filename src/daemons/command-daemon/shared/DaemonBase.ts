/**
 * Base Daemon - Abstract foundation for all JTAG daemons
 *
 * Provides common daemon functionality while enforcing proper separation of concerns.
 * All daemons inherit from this to ensure consistent architecture.
 *
 * LIFECYCLE:
 *   CREATED → STARTING → READY (or FAILED) → STOPPED
 *
 * STARTUP QUEUE:
 *   Messages arriving during STARTING state are queued and processed once READY.
 *   This ensures no messages are lost during daemon initialization.
 */

import { JTAGModule } from '../../../system/core/shared/JTAGModule';
import type { JTAGEnvironment, JTAGContext, JTAGMessage, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import {JTAGMessageFactory, useEnvironment} from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter, MessageSubscriber } from '../../../system/core/router/shared/JTAGRouter';
import { type RouterResult } from '../../../system/core/router/shared/RouterTypes';
import { type BaseResponsePayload } from '../../../system/core/types/ResponseTypes';

// ============================================================================
// Lifecycle Types
// ============================================================================

/**
 * Daemon lifecycle states
 */
export enum DaemonLifecycleState {
  CREATED = 'created',     // Constructor complete, not registered
  STARTING = 'starting',   // Registered with router, initialize() running
  READY = 'ready',         // Fully initialized, accepting messages
  FAILED = 'failed',       // initialize() threw an error
  STOPPED = 'stopped'      // shutdown() called
}

/**
 * Queued message during startup
 */
interface QueuedDaemonMessage {
  message: JTAGMessage;
  resolve: (response: BaseResponsePayload) => void;
  reject: (error: Error) => void;
  queuedAt: number;
  timeoutId?: ReturnType<typeof setTimeout>;
}

/**
 * Queue configuration for startup buffering
 */
export interface DaemonQueueConfig {
  maxQueueSize: number;    // Default: 100
  queueTimeout: number;    // Default: 30000 (30s)
  dropOldest: boolean;     // Default: true (overflow behavior)
}

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

  // ============================================================================
  // Lifecycle State & Startup Queue
  // ============================================================================

  /** Current lifecycle state */
  private _lifecycleState: DaemonLifecycleState = DaemonLifecycleState.CREATED;

  /** Queue for messages arriving during initialization */
  private readonly startupQueue: QueuedDaemonMessage[] = [];

  /** Error that caused initialization to fail (if any) */
  private initializationError?: Error;

  /** Queue configuration - override in subclass to customize */
  protected readonly queueConfig: DaemonQueueConfig = {
    maxQueueSize: 100,
    queueTimeout: 30000,  // 30 seconds
    dropOldest: true
  };

  /** Get current lifecycle state */
  get lifecycleState(): DaemonLifecycleState {
    return this._lifecycleState;
  }

  /** Check if daemon is ready to process messages */
  get isReady(): boolean {
    return this._lifecycleState === DaemonLifecycleState.READY;
  }

  /** Check if daemon failed to initialize */
  get isFailed(): boolean {
    return this._lifecycleState === DaemonLifecycleState.FAILED;
  }

  /** Get current startup queue size (for metrics) */
  get startupQueueSize(): number {
    return this.startupQueue.length;
  }

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
   *
   * LIFECYCLE: CREATED → STARTING → READY (or FAILED)
   *
   * Messages arriving during STARTING are queued and processed once READY.
   *
   * TWO-PHASE INITIALIZATION:
   * 1. initialize() - BLOCKING: Core setup required before READY (fast, minimal)
   * 2. initializeDeferred() - NON-BLOCKING: Heavy work runs AFTER READY in background
   *
   * This allows daemons to accept messages quickly while heavy initialization
   * (external connections, data loading, health checks) runs off the critical path.
   */
  async initializeDaemon(): Promise<void> {
    const initStart = Date.now();

    // Transition: CREATED → STARTING
    this._lifecycleState = DaemonLifecycleState.STARTING;

    // Register with router IMMEDIATELY (can receive messages now - they'll queue)
    this.router.registerSubscriber(this.subpath, this);

    try {
      // PHASE 1: Core initialization (BLOCKING - keep this fast!)
      await this.initialize();

      // Transition: STARTING → READY
      this._lifecycleState = DaemonLifecycleState.READY;

      // Drain queued messages
      await this.flushStartupQueue();

      // Emit ready event for health monitoring
      this.emitReady();

      const coreInitMs = Date.now() - initStart;

      // PHASE 2: Deferred initialization (NON-BLOCKING - runs in background)
      // This happens AFTER the daemon is READY and can process messages
      this.runDeferredInitialization(coreInitMs);

    } catch (error) {
      // Transition: STARTING → FAILED
      this._lifecycleState = DaemonLifecycleState.FAILED;
      this.initializationError = error instanceof Error ? error : new Error(String(error));

      // Reject all queued messages
      this.rejectQueuedMessages(this.initializationError);

      // Re-throw so caller knows initialization failed
      throw error;
    }
  }

  /**
   * Run deferred initialization in background (non-blocking)
   */
  private runDeferredInitialization(coreInitMs: number): void {
    // Use setImmediate to yield to event loop before starting deferred work
    const scheduleDeferred = typeof setImmediate !== 'undefined'
      ? setImmediate
      : (fn: () => void) => setTimeout(fn, 0);

    scheduleDeferred(async () => {
      const deferredStart = Date.now();
      try {
        await this.initializeDeferred();
        const deferredMs = Date.now() - deferredStart;

        // Emit metrics for observability
        this.emitInitMetrics(coreInitMs, deferredMs);

      } catch (error) {
        // Deferred init failures are logged but don't fail the daemon
        // The daemon is already READY and processing messages
        this.log.error(`Deferred initialization failed:`, error);
      }
    });
  }

  /**
   * Emit initialization metrics for observability
   */
  private emitInitMetrics(coreInitMs: number, deferredMs: number): void {
    try {
      if (typeof window !== 'undefined') return;

      const { Events } = require('../../../system/core/shared/Events');
      Events.emit('system:daemon:init-metrics', {
        daemon: this.name,
        subpath: this.subpath,
        coreInitMs,
        deferredMs,
        totalMs: coreInitMs + deferredMs,
        timestamp: Date.now()
      });

      // Log if initialization was slow
      if (coreInitMs > 100) {
        this.log.warn(`Slow core init: ${coreInitMs}ms (should be <100ms)`);
      }
    } catch {
      // Ignore - Events module may not be available
    }
  }

  /**
   * PHASE 1: Core initialization (BLOCKING)
   *
   * Override this for MINIMAL setup required before the daemon can process messages.
   * Keep this FAST (<100ms) - move heavy work to initializeDeferred().
   *
   * Examples of what belongs here:
   * - Event subscriptions
   * - In-memory data structure setup
   * - Router registration (done automatically)
   *
   * Examples of what does NOT belong here:
   * - Database connections (move to deferred)
   * - External API calls (move to deferred)
   * - File I/O (move to deferred)
   * - Health checks (move to deferred)
   */
  protected abstract initialize(): Promise<void>;

  /**
   * PHASE 2: Deferred initialization (NON-BLOCKING)
   *
   * Override this for HEAVY work that can run after the daemon is READY.
   * This runs in the background - the daemon is already accepting messages.
   *
   * Examples of what belongs here:
   * - Database connections and migrations
   * - External service connections (APIs)
   * - Loading cached data
   * - Health check initialization
   * - Periodic task registration
   *
   * Failures here are logged but don't fail the daemon.
   */
  protected async initializeDeferred(): Promise<void> {
    // Default: no-op (backward compatible with existing daemons)
  }

  /**
   * Process a message - SUBCLASSES MUST IMPLEMENT THIS
   * (Previously called handleMessage - renamed to allow base class lifecycle management)
   */
  protected abstract processMessage(message: JTAGMessage): Promise<BaseResponsePayload>;

  /**
   * Handle incoming messages (MessageSubscriber interface)
   *
   * Routes messages based on lifecycle state:
   * - READY: Process immediately
   * - STARTING: Queue for later
   * - FAILED/STOPPED: Return error
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    // FAST PATH: Daemon is ready, process immediately
    if (this._lifecycleState === DaemonLifecycleState.READY) {
      return await this.processMessage(message);
    }

    // QUEUE PATH: Daemon is starting, queue for later
    if (this._lifecycleState === DaemonLifecycleState.STARTING) {
      return await this.queueMessageForStartup(message);
    }

    // FAILED PATH: Daemon failed to initialize
    if (this._lifecycleState === DaemonLifecycleState.FAILED) {
      // Cast to include error - will be preserved at runtime
      return {
        success: false,
        timestamp: new Date().toISOString(),
        context: this.context,
        sessionId: this.context.uuid,
        error: `Daemon ${this.name} failed to initialize: ${this.initializationError?.message}`
      } as BaseResponsePayload & { error: string };
    }

    // STOPPED or CREATED: Should not receive messages
    return {
      success: false,
      timestamp: new Date().toISOString(),
      context: this.context,
      sessionId: this.context.uuid,
      error: `Daemon ${this.name} is not running (state: ${this._lifecycleState})`
    } as BaseResponsePayload & { error: string };
  }

  /**
   * Get endpoint (MessageSubscriber interface)
   */
  get endpoint(): string {
    return this.subpath;
  }

  // ============================================================================
  // Startup Queue Methods
  // ============================================================================

  /**
   * Queue a message for processing once initialization completes
   */
  private async queueMessageForStartup(message: JTAGMessage): Promise<BaseResponsePayload> {
    return new Promise((resolve, reject) => {
      // Check queue capacity
      if (this.startupQueue.length >= this.queueConfig.maxQueueSize) {
        if (this.queueConfig.dropOldest) {
          // Drop oldest message to make room
          const dropped = this.startupQueue.shift();
          if (dropped) {
            if (dropped.timeoutId) clearTimeout(dropped.timeoutId);
            dropped.reject(new Error('Message dropped due to startup queue overflow'));
            this.log.warn(`Dropped oldest queued message due to overflow (queue size: ${this.queueConfig.maxQueueSize})`);
          }
        } else {
          // Reject new message
          reject(new Error(`Daemon ${this.name} startup queue full (${this.queueConfig.maxQueueSize})`));
          return;
        }
      }

      // Set timeout for this message
      const timeoutId = setTimeout(() => {
        const index = this.startupQueue.findIndex(q => q.message === message);
        if (index !== -1) {
          const queued = this.startupQueue.splice(index, 1)[0];
          queued.reject(new Error(`Queued message timed out after ${this.queueConfig.queueTimeout}ms`));
        }
      }, this.queueConfig.queueTimeout);

      // Add to queue
      this.startupQueue.push({
        message,
        resolve,
        reject,
        queuedAt: Date.now(),
        timeoutId
      });

      this.log.debug(`Queued message for ${message.endpoint} (queue size: ${this.startupQueue.length})`);
    });
  }

  /**
   * Process all queued messages after initialization completes
   */
  private async flushStartupQueue(): Promise<void> {
    if (this.startupQueue.length === 0) return;

    this.log.info(`Flushing ${this.startupQueue.length} queued message(s)`);

    // Process in FIFO order
    while (this.startupQueue.length > 0) {
      const queued = this.startupQueue.shift()!;

      // Clear timeout
      if (queued.timeoutId) clearTimeout(queued.timeoutId);

      try {
        const response = await this.processMessage(queued.message);
        queued.resolve(response);
      } catch (error) {
        queued.reject(error instanceof Error ? error : new Error(String(error)));
      }
    }
  }

  /**
   * Reject all queued messages when initialization fails
   */
  private rejectQueuedMessages(error: Error): void {
    while (this.startupQueue.length > 0) {
      const queued = this.startupQueue.shift()!;
      if (queued.timeoutId) clearTimeout(queued.timeoutId);
      queued.reject(error);
    }
  }

  /**
   * Emit ready event for health monitoring integration
   */
  private emitReady(): void {
    try {
      // Dynamic import to avoid circular dependency
      if (typeof window !== 'undefined') return;

      const { Events } = require('../../../system/core/shared/Events');
      Events.emit('system:daemon:ready', {
        daemon: this.name,
        subpath: this.subpath,
        timestamp: Date.now()
      });
    } catch {
      // Ignore - Events module may not be available
    }
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

    // Transition: * → STOPPED
    this._lifecycleState = DaemonLifecycleState.STOPPED;

    // Reject any queued messages (they won't be processed)
    this.rejectQueuedMessages(new Error(`Daemon ${this.name} is shutting down`));

    // Call subclass cleanup first
    await this.cleanup();

    // Then automatic cleanup
    this.cleanupSubscriptions();
    this.cleanupIntervals();
  }
}