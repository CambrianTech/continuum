/**
 * ProcessLifecycle - Standard Lifecycle Patterns for Child Processes
 *
 * Reusable lifecycle management for process-based daemons.
 * Used by LoggerDaemon, cognition layer (persona/tools), and other multiprocess components.
 */

import type { IPCMessage, IPCResponse } from './IPCProtocol.js';
import {
  isIPCMessage,
  isIPCResponse,
  createSuccessResponse,
  createErrorResponse
} from './IPCProtocol.js';

/**
 * Lifecycle state for a child process
 */
export type LifecycleState =
  | 'initializing'   // Process is starting up
  | 'ready'          // Process is operational
  | 'busy'           // Process is working on tasks
  | 'idle'           // Process is waiting for work
  | 'shutting-down'  // Process is gracefully shutting down
  | 'terminated';    // Process has exited

/**
 * Lifecycle hooks for child process implementation
 */
export interface LifecycleHooks {
  /**
   * Called when process starts (before ready state)
   * Use for initialization: loading config, opening connections, etc.
   */
  onInitialize?: () => Promise<void>;

  /**
   * Called when process becomes ready (after initialization)
   */
  onReady?: () => Promise<void>;

  /**
   * Called when shutdown is requested (before terminating)
   * Use for cleanup: flushing queues, closing connections, saving state
   */
  onShutdown?: () => Promise<void>;

  /**
   * Called when process encounters fatal error
   */
  onError?: (error: Error) => Promise<void>;

  /**
   * Called for each incoming IPC message
   * Return IPCResponse or throw error
   */
  onMessage: (message: IPCMessage) => Promise<IPCResponse>;
}

/**
 * Lifecycle statistics
 */
export interface LifecycleStats {
  readonly state: LifecycleState;
  readonly startTime: string;           // ISO timestamp
  readonly uptime: number;               // Milliseconds
  readonly messagesReceived: number;     // Total messages handled
  readonly messagesSucceeded: number;    // Successful responses
  readonly messagesFailed: number;       // Failed responses
  readonly lastMessageTime?: string;     // ISO timestamp
  readonly lastError?: string;           // Last error message
}

/**
 * LifecycleManager - Manages lifecycle state and hooks for child processes
 *
 * Usage in child process entry point:
 * ```typescript
 * const lifecycle = new LifecycleManager({
 *   onInitialize: async () => {
 *     // Load config, open DB connections, etc.
 *   },
 *   onShutdown: async () => {
 *     // Flush queues, close connections, etc.
 *   },
 *   onMessage: async (message) => {
 *     // Handle IPC messages
 *     return createSuccessResponse('log-response', { written: true });
 *   }
 * });
 *
 * await lifecycle.start();
 * ```
 */
export class LifecycleManager {
  private _state: LifecycleState = 'initializing';
  private readonly _startTime: string;
  private _messagesReceived = 0;
  private _messagesSucceeded = 0;
  private _messagesFailed = 0;
  private _lastMessageTime?: string;
  private _lastError?: string;

  constructor(private readonly hooks: LifecycleHooks) {
    this._startTime = new Date().toISOString();
  }

  /**
   * Start the process lifecycle
   * Sets up IPC listeners and runs initialization hooks
   */
  async start(): Promise<void> {
    try {
      // Run initialization hook
      if (this.hooks.onInitialize) {
        await this.hooks.onInitialize();
      }

      // Transition to ready state
      this._state = 'ready';
      if (this.hooks.onReady) {
        await this.hooks.onReady();
      }

      // Set up IPC message handling
      this.setupIPCHandlers();

      // Log ready state
      console.log(`[LifecycleManager] Process ready (pid: ${process.pid})`);

    } catch (error) {
      this._state = 'terminated';
      this._lastError = error instanceof Error ? error.message : String(error);
      console.error(`[LifecycleManager] Initialization failed:`, error);

      if (this.hooks.onError) {
        await this.hooks.onError(error instanceof Error ? error : new Error(String(error)));
      }

      process.exit(1);
    }
  }

  /**
   * Gracefully shut down the process
   */
  async shutdown(): Promise<void> {
    if (this._state === 'shutting-down' || this._state === 'terminated') {
      return;
    }

    console.log(`[LifecycleManager] Shutdown requested (state: ${this._state})`);
    this._state = 'shutting-down';

    try {
      // Run shutdown hook
      if (this.hooks.onShutdown) {
        await this.hooks.onShutdown();
      }

      this._state = 'terminated';
      console.log(`[LifecycleManager] Shutdown complete`);

      // Exit cleanly
      process.exit(0);

    } catch (error) {
      this._lastError = error instanceof Error ? error.message : String(error);
      console.error(`[LifecycleManager] Shutdown failed:`, error);

      if (this.hooks.onError) {
        await this.hooks.onError(error instanceof Error ? error : new Error(String(error)));
      }

      // Force exit even on error
      process.exit(1);
    }
  }

  /**
   * Get current lifecycle statistics
   */
  getStats(): LifecycleStats {
    return {
      state: this._state,
      startTime: this._startTime,
      uptime: Date.now() - new Date(this._startTime).getTime(),
      messagesReceived: this._messagesReceived,
      messagesSucceeded: this._messagesSucceeded,
      messagesFailed: this._messagesFailed,
      lastMessageTime: this._lastMessageTime,
      lastError: this._lastError
    };
  }

  /**
   * Get current state
   */
  get state(): LifecycleState {
    return this._state;
  }

  /**
   * Set up IPC message handlers
   */
  private setupIPCHandlers(): void {
    // Handle IPC messages from parent
    process.on('message', async (rawMessage: unknown) => {
      // Validate message structure
      if (!isIPCMessage(rawMessage)) {
        console.error(`[LifecycleManager] Invalid IPC message:`, rawMessage);
        return;
      }

      const message = rawMessage as IPCMessage;
      this._messagesReceived++;
      this._lastMessageTime = new Date().toISOString();

      try {
        // Handle system messages first
        if (message.type === 'shutdown') {
          await this.shutdown();
          return;
        }

        if (message.type === 'health-check') {
          const stats = this.getStats();
          const response = createSuccessResponse(
            'health-response',
            {
              uptime: stats.uptime,
              memoryUsage: process.memoryUsage().rss,
              queueSize: 0 // Override in child implementation if needed
            },
            message.messageId
          );
          process.send?.(response);
          this._messagesSucceeded++;
          return;
        }

        if (message.type === 'ping') {
          const response = createSuccessResponse(
            'pong',
            { timestamp: new Date().toISOString() },
            message.messageId
          );
          process.send?.(response);
          this._messagesSucceeded++;
          return;
        }

        // Update state based on activity
        const previousState = this._state;
        if (this._state === 'idle' || this._state === 'ready') {
          this._state = 'busy';
        }

        // Delegate to custom message handler
        const response = await this.hooks.onMessage(message);

        // Send response back to parent
        if (process.send) {
          process.send(response);
        }

        // Update statistics
        if (response.success) {
          this._messagesSucceeded++;
        } else {
          this._messagesFailed++;
          this._lastError = response.error;
        }

        // Return to idle if no more work
        if (previousState === 'idle' || previousState === 'ready') {
          this._state = 'idle';
        }

      } catch (error) {
        this._messagesFailed++;
        this._lastError = error instanceof Error ? error.message : String(error);

        console.error(`[LifecycleManager] Message handling failed:`, error);

        // Send error response
        const errorResponse = createErrorResponse(
          `${message.type}-response`,
          this._lastError,
          message.messageId
        );

        if (process.send) {
          process.send(errorResponse);
        }

        // Call error hook
        if (this.hooks.onError) {
          await this.hooks.onError(error instanceof Error ? error : new Error(String(error)));
        }
      }
    });

    // Handle process signals
    process.on('SIGINT', () => this.shutdown());
    process.on('SIGTERM', () => this.shutdown());

    // Handle uncaught errors
    process.on('uncaughtException', async (error: Error) => {
      console.error(`[LifecycleManager] Uncaught exception:`, error);
      this._lastError = error.message;

      if (this.hooks.onError) {
        await this.hooks.onError(error);
      }

      // Force shutdown
      await this.shutdown();
    });

    process.on('unhandledRejection', async (reason: unknown) => {
      const error = reason instanceof Error ? reason : new Error(String(reason));
      console.error(`[LifecycleManager] Unhandled rejection:`, error);
      this._lastError = error.message;

      if (this.hooks.onError) {
        await this.hooks.onError(error);
      }

      // Force shutdown
      await this.shutdown();
    });
  }
}

/**
 * Helper to create a minimal lifecycle for simple processes
 *
 * Usage:
 * ```typescript
 * createSimpleLifecycle(async (message) => {
 *   // Handle message
 *   return createSuccessResponse('response', { result: 'ok' });
 * });
 * ```
 */
export function createSimpleLifecycle(
  onMessage: (message: IPCMessage) => Promise<IPCResponse>
): LifecycleManager {
  return new LifecycleManager({ onMessage });
}
