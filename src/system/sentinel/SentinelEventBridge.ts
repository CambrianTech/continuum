/**
 * SentinelEventBridge — Bridges Rust sentinel process events to TypeScript Events
 *
 * The Rust SentinelModule manages processes with kill_on_drop, handle tracking,
 * and log capture. But its internal MessageBus events don't cross the IPC boundary
 * to TypeScript. This bridge polls Rust sentinel handles and emits TypeScript Events
 * so that widgets, services, and completion handlers can subscribe.
 *
 * Events emitted:
 *   sentinel:{handle}:status   — { handle, status, progress, type, metadata }
 *   sentinel:{handle}:complete — { handle, type, exitCode, durationMs, metadata }
 *   sentinel:{handle}:error    — { handle, type, error, exitCode, metadata }
 *   sentinel:{handle}:output   — { handle, type, lines[], metadata }
 *
 * Generic events (for SentinelEscalationService compatibility):
 *   sentinel:complete — { handle, status: 'completed', ... }
 *   sentinel:error    — { handle, status: 'failed', ... }
 *
 * Usage:
 *   sentinelEventBridge.watch(handle, 'training', { personaId, traitType });
 *   Events.subscribe('sentinel:{handle}:complete', (payload) => { ... });
 */

import { Events } from '../core/shared/Events';
import { RustCoreIPCClient } from '../../workers/continuum-core/bindings/RustCoreIPC';
import type { SentinelHandle } from '../../workers/continuum-core/bindings/modules/sentinel';

/**
 * Metadata attached when watching a sentinel — flows through to all emitted events.
 */
export interface WatchMetadata {
  /** Sentinel type category (e.g., 'training', 'build', 'pipeline') */
  type: string;
  /** Caller-provided context that propagates to event subscribers */
  [key: string]: unknown;
}

/**
 * Internal tracked state for a watched sentinel.
 */
interface WatchedSentinel {
  handle: string;
  metadata: WatchMetadata;
  lastStatus: string;
  lastLogLineCount: number;
  registeredAt: number;
}

/**
 * SentinelEventBridge — singleton that polls Rust handles and emits TypeScript Events.
 */
class SentinelEventBridge {
  private _watched = new Map<string, WatchedSentinel>();
  private _pollTimer: ReturnType<typeof setInterval> | null = null;
  private _pollIntervalMs = 1000;
  private _polling = false;

  /**
   * Start watching a sentinel handle. Events will be emitted as its status changes.
   *
   * @param handle Rust sentinel handle ID
   * @param type Category type (e.g., 'training', 'build')
   * @param metadata Arbitrary context that flows through to all events
   */
  watch(handle: string, type: string, metadata: Record<string, unknown> = {}): void {
    this._watched.set(handle, {
      handle,
      metadata: { type, ...metadata },
      lastStatus: 'running',
      lastLogLineCount: 0,
      registeredAt: Date.now(),
    });
    this._ensurePolling();
    console.log(`[SentinelEventBridge] Watching ${handle} (type=${type})`);
  }

  /**
   * Stop watching a sentinel handle.
   */
  unwatch(handle: string): void {
    this._watched.delete(handle);
    if (this._watched.size === 0) {
      this._stopPolling();
    }
  }

  /**
   * Check if a handle is being watched.
   */
  isWatching(handle: string): boolean {
    return this._watched.has(handle);
  }

  /**
   * Get count of currently watched handles.
   */
  get watchCount(): number {
    return this._watched.size;
  }

  /**
   * Initialize the bridge (called at server startup).
   */
  initialize(): void {
    console.log('[SentinelEventBridge] Initialized — ready to bridge Rust sentinel events');
  }

  /**
   * Shutdown the bridge (called at server shutdown).
   */
  shutdown(): void {
    this._stopPolling();
    this._watched.clear();
  }

  // ─── Private ────────────────────────────────────────────────────────────────

  private _ensurePolling(): void {
    if (this._pollTimer) return;
    this._pollTimer = setInterval(() => this._poll(), this._pollIntervalMs);
  }

  private _stopPolling(): void {
    if (this._pollTimer) {
      clearInterval(this._pollTimer);
      this._pollTimer = null;
    }
  }

  private async _poll(): Promise<void> {
    // Guard against overlapping polls
    if (this._polling) return;
    this._polling = true;

    try {
      const client = RustCoreIPCClient.getInstance();

      // Snapshot the handles to iterate (avoid mutation during iteration)
      const handles = Array.from(this._watched.entries());

      for (const [handle, watched] of handles) {
        try {
          const statusResult = await client.sentinelStatus(handle);
          const sentinel = statusResult.handle;
          const currentStatus = sentinel.status;

          // Status transition detected
          if (currentStatus !== watched.lastStatus) {
            await this._handleStatusChange(watched, sentinel, currentStatus);
          }

          // Tail new output for progress (only while running)
          if (currentStatus === 'running') {
            await this._emitNewOutput(watched, client);
          }
        } catch {
          // Handle not found in Rust — sentinel was cleaned up
          console.warn(`[SentinelEventBridge] Handle ${handle} not found, unwatching`);
          this.unwatch(handle);
        }
      }
    } finally {
      this._polling = false;
    }
  }

  private async _handleStatusChange(
    watched: WatchedSentinel,
    sentinel: SentinelHandle,
    newStatus: string,
  ): Promise<void> {
    const { handle, metadata } = watched;
    const durationMs = sentinel.endTime
      ? sentinel.endTime - sentinel.startTime
      : Date.now() - watched.registeredAt;

    if (newStatus === 'completed') {
      // Per-handle event
      Events.emit(`sentinel:${handle}:complete`, {
        handle,
        ...metadata,
        status: 'completed',
        exitCode: sentinel.exitCode ?? 0,
        durationMs,
      });

      // Generic event (SentinelEscalationService listens for this)
      Events.emit('sentinel:complete', {
        handle,
        ...metadata,
        status: 'completed',
        exitCode: sentinel.exitCode ?? 0,
        durationMs,
      });

      console.log(`[SentinelEventBridge] ${handle} completed (${durationMs}ms)`);
      this.unwatch(handle);
    } else if (newStatus === 'failed') {
      Events.emit(`sentinel:${handle}:error`, {
        handle,
        ...metadata,
        status: 'failed',
        error: sentinel.error,
        exitCode: sentinel.exitCode ?? -1,
        durationMs,
      });

      Events.emit('sentinel:error', {
        handle,
        ...metadata,
        status: 'failed',
        error: sentinel.error,
        exitCode: sentinel.exitCode ?? -1,
        durationMs,
      });

      console.log(`[SentinelEventBridge] ${handle} failed: ${sentinel.error}`);
      this.unwatch(handle);
    } else if (newStatus === 'cancelled') {
      Events.emit(`sentinel:${handle}:error`, {
        handle,
        ...metadata,
        status: 'cancelled',
        error: 'Cancelled',
        durationMs,
      });

      Events.emit('sentinel:cancelled', {
        handle,
        ...metadata,
        status: 'cancelled',
        durationMs,
      });

      console.log(`[SentinelEventBridge] ${handle} cancelled`);
      this.unwatch(handle);
    }

    watched.lastStatus = newStatus;
  }

  private async _emitNewOutput(watched: WatchedSentinel, client: RustCoreIPCClient): Promise<void> {
    try {
      const logs = await client.sentinelLogsTail(watched.handle, 'stdout', 20);

      // Only emit if there are new lines since last poll
      if (logs.totalLines > watched.lastLogLineCount) {
        const newLineCount = logs.totalLines - watched.lastLogLineCount;
        const lines = logs.content.split('\n').slice(-newLineCount);

        Events.emit(`sentinel:${watched.handle}:output`, {
          handle: watched.handle,
          ...watched.metadata,
          lines,
          totalLines: logs.totalLines,
        });

        watched.lastLogLineCount = logs.totalLines;
      }
    } catch {
      // Log read failure is non-critical
    }
  }
}

/**
 * Singleton instance — import and use directly.
 */
export const sentinelEventBridge = new SentinelEventBridge();

/**
 * Initialize the event bridge (called during server startup).
 */
export function initializeSentinelEventBridge(): void {
  sentinelEventBridge.initialize();
}

/**
 * Shutdown the event bridge (called during server shutdown).
 */
export function shutdownSentinelEventBridge(): void {
  sentinelEventBridge.shutdown();
}
