/**
 * SentinelEventBridge — Bridges Rust sentinel process events to TypeScript Events
 *
 * Uses Rust sentinel/await (tokio::sync::watch channel) for zero-polling completion
 * detection. Each watched handle gets a single long-lived IPC call that blocks until
 * the sentinel completes — no interval-based polling.
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
import { SentinelWorkspaceManager } from './SentinelWorkspaceManager';

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
  registeredAt: number;
  /** AbortController to cancel the long-await on unwatch */
  abortController: AbortController;
}

/**
 * SentinelEventBridge — singleton that uses Rust sentinel/await for push-style
 * completion detection. No polling interval.
 */
class SentinelEventBridge {
  private _watched = new Map<string, WatchedSentinel>();

  /**
   * Start watching a sentinel handle. A long-lived IPC call (sentinel/await)
   * blocks until the sentinel completes — no polling.
   *
   * @param handle Rust sentinel handle ID
   * @param type Category type (e.g., 'training', 'build')
   * @param metadata Arbitrary context that flows through to all events
   */
  watch(handle: string, type: string, metadata: Record<string, unknown> = {}): void {
    if (this._watched.has(handle)) return;

    const abortController = new AbortController();
    const watched: WatchedSentinel = {
      handle,
      metadata: { type, ...metadata },
      lastStatus: 'running',
      registeredAt: Date.now(),
      abortController,
    };

    this._watched.set(handle, watched);
    console.log(`[SentinelEventBridge] Watching ${handle} (type=${type}) via await`);

    // Fire-and-forget: long-await IPC call resolves when sentinel completes
    this._awaitCompletion(watched).catch(err => {
      console.warn(`[SentinelEventBridge] Await error for ${handle}:`, err);
      this._watched.delete(handle);
    });
  }

  /**
   * Stop watching a sentinel handle.
   */
  unwatch(handle: string): void {
    const watched = this._watched.get(handle);
    if (watched) {
      watched.abortController.abort();
      this._watched.delete(handle);
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
   * Get all currently watched sentinels with their metadata.
   * Used by SentinelAwarenessSource to inject active sentinel state into RAG.
   */
  get activeSentinels(): ReadonlyArray<{ handle: string; metadata: WatchMetadata; lastStatus: string; registeredAt: number }> {
    return Array.from(this._watched.values()).map(w => ({
      handle: w.handle,
      metadata: w.metadata,
      lastStatus: w.lastStatus,
      registeredAt: w.registeredAt,
    }));
  }

  /**
   * Initialize the bridge (called at server startup).
   */
  initialize(): void {
    // Ready — no polling infrastructure needed
  }

  /**
   * Shutdown the bridge (called at server shutdown).
   */
  shutdown(): void {
    // Abort all outstanding await calls
    for (const watched of this._watched.values()) {
      watched.abortController.abort();
    }
    this._watched.clear();
  }

  // ─── Private ────────────────────────────────────────────────────────────────

  /**
   * Long-await a sentinel's completion via Rust sentinel/await IPC.
   * Single IPC call blocks on tokio::sync::watch channel — zero polling.
   */
  private async _awaitCompletion(watched: WatchedSentinel): Promise<void> {
    const { handle, abortController } = watched;
    const client = RustCoreIPCClient.getInstance();

    // sentinel/await blocks until completion (Rust watch channel, not polling)
    // Use a generous IPC timeout — sentinels can run for hours
    const ipcTimeoutMs = 4 * 60 * 60 * 1000; // 4 hours max

    try {
      const { response } = await client.requestFull({
        command: 'sentinel/await',
        handle,
        timeout: Math.floor(ipcTimeoutMs / 1000),
      }, ipcTimeoutMs);

      // Check if we were aborted while waiting
      if (abortController.signal.aborted) return;

      if (response.success) {
        const sentinel = (response.result as { handle: SentinelHandle }).handle;
        this._handleTerminalStatus(watched, sentinel);
      } else {
        // Await call itself failed (handle not found, watch closed)
        console.warn(`[SentinelEventBridge] Await failed for ${handle}: ${response.error}`);
        this._watched.delete(handle);
      }
    } catch (err) {
      if (abortController.signal.aborted) return;
      // Re-throw so the caller's catch handles it
      throw err;
    }
  }

  /**
   * Handle terminal status — emit events, release workspace, clean up.
   */
  private _handleTerminalStatus(watched: WatchedSentinel, sentinel: SentinelHandle): void {
    const { handle, metadata } = watched;
    const status = sentinel.status;
    const durationMs = sentinel.endTime
      ? sentinel.endTime - sentinel.startTime
      : Date.now() - watched.registeredAt;

    // Release workspace on terminal status
    SentinelWorkspaceManager.release(handle, status === 'completed').catch(err => {
      console.warn(`[SentinelEventBridge] Workspace release failed for ${handle}:`, err);
    });

    if (status === 'completed') {
      Events.emit(`sentinel:${handle}:complete`, {
        handle,
        ...metadata,
        status: 'completed',
        exitCode: sentinel.exitCode ?? 0,
        durationMs,
      });
      Events.emit('sentinel:complete', {
        handle,
        ...metadata,
        status: 'completed',
        exitCode: sentinel.exitCode ?? 0,
        durationMs,
      });
      console.log(`[SentinelEventBridge] ${handle} completed (${durationMs}ms)`);
    } else if (status === 'failed') {
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
    } else if (status === 'cancelled') {
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
    }

    watched.lastStatus = status;
    this._watched.delete(handle);
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
