/**
 * LogBatcher - Batches log messages before sending to Rust worker
 *
 * Reduces IPC calls from ~1000/sec to ~10/sec by buffering WriteLogPayload
 * entries and flushing them as a single batch command.
 *
 * Flush triggers:
 * - Every 100ms (timer)
 * - When 50 messages are queued (capacity)
 *
 * All sends are fire-and-forget — log delivery is best-effort.
 */

import type { WriteLogPayload } from '../../../shared/ipc/logger/LoggerMessageTypes';
import type { LoggerWorkerClient } from '../../../shared/ipc/logger/LoggerWorkerClient';

export class LogBatcher {
  private _buffer: WriteLogPayload[] = [];
  private _flushTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly _flushIntervalMs = 100;
  private readonly _maxBatchSize = 50;
  private _destroyed = false;

  constructor(private readonly _workerClient: LoggerWorkerClient) {}

  /**
   * Queue a log payload for batched delivery.
   * Never blocks — worst case the buffer grows until next flush.
   */
  queue(payload: WriteLogPayload): void {
    if (this._destroyed) return;

    this._buffer.push(payload);

    if (this._buffer.length >= this._maxBatchSize) {
      this.flush();
    } else if (!this._flushTimer) {
      this._flushTimer = setTimeout(() => this.flush(), this._flushIntervalMs);
    }
  }

  /**
   * Flush all buffered messages to the Rust worker as a single batch.
   * Fire-and-forget — errors are silently dropped (logging errors in a logger = infinite loop).
   */
  flush(): void {
    if (this._flushTimer) {
      clearTimeout(this._flushTimer);
      this._flushTimer = null;
    }

    if (this._buffer.length === 0) return;

    const batch = this._buffer;
    this._buffer = [];

    // Single IPC call for entire batch
    this._workerClient.writeLogsBatch(batch).catch(() => {});
  }

  /**
   * Flush remaining messages and stop accepting new ones.
   */
  destroy(): void {
    this._destroyed = true;
    this.flush();
  }
}
