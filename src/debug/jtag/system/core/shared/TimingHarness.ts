/**
 * TimingHarness - High-Resolution Performance Instrumentation
 *
 * Port of Rust timing.rs to TypeScript for comprehensive performance analysis.
 * Uses high-resolution timers (process.hrtime.bigint() or performance.now()).
 *
 * ARCHITECTURE:
 * - Nanosecond precision where available (Node.js)
 * - Lock-free design (single-threaded JS)
 * - Structured JSONL output for analysis
 * - Per-operation phase breakdown
 *
 * USAGE:
 * ```typescript
 * const timer = TimingHarness.start('rag/build-context');
 * timer.setMeta('personaId', personaId);
 *
 * // Phase 1: Load sources
 * const sources = await loadSources();
 * timer.mark('load_sources');
 *
 * // Phase 2: Build context
 * const context = await buildContext(sources);
 * timer.mark('build_context');
 *
 * // Phase 3: Serialize
 * const result = serialize(context);
 * timer.mark('serialize');
 *
 * timer.finish(); // Logs to file and console
 * ```
 */

import { createWriteStream, existsSync, mkdirSync, type WriteStream } from 'fs';
import { dirname } from 'path';

// ============================================================================
// Types
// ============================================================================

export interface TimingRecord {
  // Identity
  requestId: string;
  timestampMs: number;

  // Request info
  operation: string;
  category: string;
  meta: Record<string, string | number | boolean>;

  // Phase timings (microseconds for readability)
  phases: Record<string, number>;

  // Totals
  totalUs: number;
  totalMs: number;

  // Context
  success: boolean;
  error?: string;
}

export interface PercentileStats {
  count: number;
  minUs: number;
  maxUs: number;
  meanUs: number;
  p50Us: number;
  p95Us: number;
  p99Us: number;
}

// ============================================================================
// High-Resolution Time
// ============================================================================

const isNode = typeof process !== 'undefined' && process.hrtime?.bigint;

function getNowNs(): bigint {
  if (isNode) {
    return process.hrtime.bigint();
  }
  // Browser fallback: performance.now() in milliseconds -> nanoseconds
  return BigInt(Math.round(performance.now() * 1_000_000));
}

function generateId(): string {
  return Math.random().toString(36).substring(2, 15) +
         Math.random().toString(36).substring(2, 15);
}

// ============================================================================
// TimingHarness - Main Timer Class
// ============================================================================

export class TimingHarness {
  private requestId: string;
  private operation: string;
  private category: string;
  private startTime: bigint;
  private phaseStart: bigint;
  private phases: Map<string, bigint> = new Map();
  private meta: Record<string, string | number | boolean> = {};
  private success = true;
  private errorMsg?: string;

  private constructor(operation: string, category?: string) {
    this.requestId = generateId();
    this.operation = operation;
    this.category = category || operation.split('/')[0] || 'unknown';
    this.startTime = getNowNs();
    this.phaseStart = this.startTime;
  }

  /**
   * Start a new timing harness for an operation
   */
  static start(operation: string, category?: string): TimingHarness {
    return new TimingHarness(operation, category);
  }

  /**
   * Mark the end of a phase and start the next
   */
  mark(phaseName: string): this {
    const now = getNowNs();
    const elapsed = now - this.phaseStart;
    this.phases.set(phaseName, elapsed);
    this.phaseStart = now;
    return this;
  }

  /**
   * Set metadata for this timing record
   */
  setMeta(key: string, value: string | number | boolean): this {
    this.meta[key] = value;
    return this;
  }

  /**
   * Mark as failed with error message
   */
  setError(error: string): this {
    this.success = false;
    this.errorMsg = error;
    return this;
  }

  /**
   * Finish timing and return the record
   */
  finish(): TimingRecord {
    const totalNs = getNowNs() - this.startTime;
    const totalUs = Number(totalNs / 1000n);
    const totalMs = totalUs / 1000;

    // Convert phase timings to microseconds
    const phasesUs: Record<string, number> = {};
    for (const [name, ns] of this.phases) {
      phasesUs[name] = Number(ns / 1000n);
    }

    const record: TimingRecord = {
      requestId: this.requestId,
      timestampMs: Date.now(),
      operation: this.operation,
      category: this.category,
      meta: this.meta,
      phases: phasesUs,
      totalUs,
      totalMs,
      success: this.success,
      error: this.errorMsg,
    };

    // Log to collector
    TimingCollector.instance.record(record);

    return record;
  }

  /**
   * Quick timing for a single operation (no phases)
   */
  static async time<T>(
    operation: string,
    fn: () => Promise<T>,
    meta?: Record<string, string | number | boolean>
  ): Promise<T> {
    const timer = TimingHarness.start(operation);
    if (meta) {
      for (const [k, v] of Object.entries(meta)) {
        timer.setMeta(k, v);
      }
    }
    try {
      const result = await fn();
      timer.mark('execute');
      timer.finish();
      return result;
    } catch (error) {
      timer.setError(error instanceof Error ? error.message : String(error));
      timer.finish();
      throw error;
    }
  }

  /**
   * Synchronous timing for a single operation
   */
  static timeSync<T>(
    operation: string,
    fn: () => T,
    meta?: Record<string, string | number | boolean>
  ): T {
    const timer = TimingHarness.start(operation);
    if (meta) {
      for (const [k, v] of Object.entries(meta)) {
        timer.setMeta(k, v);
      }
    }
    try {
      const result = fn();
      timer.mark('execute');
      timer.finish();
      return result;
    } catch (error) {
      timer.setError(error instanceof Error ? error.message : String(error));
      timer.finish();
      throw error;
    }
  }
}

// ============================================================================
// TimingCollector - Singleton for collecting and aggregating timing data
// ============================================================================

export class TimingCollector {
  private static _instance: TimingCollector;
  private records: TimingRecord[] = [];
  private maxRecords = 10000;
  private logPath: string;
  private logEnabled: boolean;
  private _writeStream: WriteStream | null = null;
  private _writeBuffer: string[] = [];
  private _flushTimer: ReturnType<typeof setInterval> | null = null;
  private static readonly FLUSH_INTERVAL_MS = 500;
  private static readonly MAX_BUFFER_SIZE = 100;

  // Per-category enable/disable (fine-grained control)
  private _categoryEnabled: Map<string, boolean> = new Map();

  private constructor() {
    // Default log path - can be overridden via env var
    this.logPath = process.env.JTAG_TIMING_LOG ||
      '/tmp/jtag-timing.jsonl';
    this.logEnabled = process.env.JTAG_TIMING_ENABLED !== 'false';

    // Set up async write stream (replaces appendFileSync which blocked event loop)
    if (this.logEnabled && isNode) {
      try {
        const dir = dirname(this.logPath);
        if (!existsSync(dir)) {
          mkdirSync(dir, { recursive: true });
        }
        this._writeStream = createWriteStream(this.logPath, { flags: 'a' });
        this._writeStream.on('error', () => {
          // Silently disable on write errors
          this._writeStream = null;
        });
      } catch {
        // Ignore initialization errors
      }

      // Periodic flush (instead of sync write per record)
      this._flushTimer = setInterval(() => this.flushBuffer(), TimingCollector.FLUSH_INTERVAL_MS);
    }
  }

  static get instance(): TimingCollector {
    if (!TimingCollector._instance) {
      TimingCollector._instance = new TimingCollector();
    }
    return TimingCollector._instance;
  }

  /**
   * Configure the collector
   */
  configure(options: { logPath?: string; maxRecords?: number; enabled?: boolean }): void {
    if (options.logPath) this.logPath = options.logPath;
    if (options.maxRecords) this.maxRecords = options.maxRecords;
    if (options.enabled !== undefined) this.logEnabled = options.enabled;
  }

  /**
   * Enable or disable timing for a specific category.
   * When disabled, records for that category are silently dropped.
   */
  setCategoryEnabled(category: string, enabled: boolean): void {
    this._categoryEnabled.set(category, enabled);
  }

  /**
   * Check if a category is enabled (default: true if not explicitly set)
   */
  isCategoryEnabled(category: string): boolean {
    return this._categoryEnabled.get(category) ?? true;
  }

  /**
   * Record a timing entry
   */
  record(timing: TimingRecord): void {
    // Check per-category filter
    if (!this.isCategoryEnabled(timing.category)) {
      return;
    }

    // Add to in-memory buffer
    this.records.push(timing);
    if (this.records.length > this.maxRecords) {
      this.records.shift();
    }

    // Buffer for async file write (never blocks event loop)
    if (this.logEnabled && this._writeStream) {
      this._writeBuffer.push(JSON.stringify(timing));
      if (this._writeBuffer.length >= TimingCollector.MAX_BUFFER_SIZE) {
        this.flushBuffer();
      }
    }

    // Console debug log for slow operations (>500ms — raised from 100ms to reduce spam)
    if (timing.totalMs > 500) {
      const phases = Object.entries(timing.phases)
        .map(([k, v]) => `${k}=${(v / 1000).toFixed(1)}ms`)
        .join(', ');
      console.debug(
        `\u23F1\uFE0F TIMING [${timing.category}] ${timing.operation}: ${timing.totalMs.toFixed(1)}ms (${phases})`
      );
    }
  }

  /**
   * Flush buffered timing records to disk (async, non-blocking)
   */
  private flushBuffer(): void {
    if (this._writeBuffer.length === 0 || !this._writeStream) {
      return;
    }

    const batch = this._writeBuffer.join('\n') + '\n';
    this._writeBuffer.length = 0;
    this._writeStream.write(batch);
  }

  /**
   * Get recent records for a category
   */
  getRecords(category?: string, limit = 100): TimingRecord[] {
    let filtered = this.records;
    if (category) {
      filtered = filtered.filter(r => r.category === category);
    }
    return filtered.slice(-limit);
  }

  /**
   * Compute percentile statistics for an operation
   */
  getStats(operation: string): PercentileStats {
    const values = this.records
      .filter(r => r.operation === operation)
      .map(r => r.totalUs);

    return this.computePercentiles(values);
  }

  /**
   * Compute percentile statistics for a category
   */
  getCategoryStats(category: string): PercentileStats {
    const values = this.records
      .filter(r => r.category === category)
      .map(r => r.totalUs);

    return this.computePercentiles(values);
  }

  private computePercentiles(values: number[]): PercentileStats {
    if (values.length === 0) {
      return { count: 0, minUs: 0, maxUs: 0, meanUs: 0, p50Us: 0, p95Us: 0, p99Us: 0 };
    }

    const sorted = [...values].sort((a, b) => a - b);
    const count = sorted.length;
    const sum = sorted.reduce((a, b) => a + b, 0);

    return {
      count,
      minUs: sorted[0],
      maxUs: sorted[count - 1],
      meanUs: Math.round(sum / count),
      p50Us: sorted[Math.floor(count * 0.5)],
      p95Us: sorted[Math.floor(count * 0.95)],
      p99Us: sorted[Math.floor(count * 0.99)],
    };
  }

  /**
   * Print summary to console
   */
  printSummary(): void {
    const byCategory = new Map<string, number[]>();
    for (const r of this.records) {
      const values = byCategory.get(r.category) || [];
      values.push(r.totalUs);
      byCategory.set(r.category, values);
    }

    console.log('\n\u{1F4CA} TIMING SUMMARY');
    console.log('═══════════════════════════════════════════════════════');
    console.log('Category       │ Count  │ P50      │ P95      │ P99      │');
    console.log('───────────────┼────────┼──────────┼──────────┼──────────┤');

    for (const [category, values] of byCategory) {
      const stats = this.computePercentiles(values);
      console.log(
        `${category.padEnd(14)} │ ${stats.count.toString().padStart(6)} │ ${this.formatUs(stats.p50Us).padStart(8)} │ ${this.formatUs(stats.p95Us).padStart(8)} │ ${this.formatUs(stats.p99Us).padStart(8)} │`
      );
    }
    console.log('═══════════════════════════════════════════════════════\n');
  }

  private formatUs(us: number): string {
    if (us >= 1_000_000) {
      return `${(us / 1_000_000).toFixed(2)}s`;
    } else if (us >= 1_000) {
      return `${(us / 1_000).toFixed(1)}ms`;
    } else {
      return `${us}\u00B5s`;
    }
  }

  /**
   * Clear all records
   */
  clear(): void {
    this.records = [];
  }

  /**
   * Shutdown: flush remaining buffer and close stream
   */
  shutdown(): void {
    this.flushBuffer();
    if (this._flushTimer) {
      clearInterval(this._flushTimer);
      this._flushTimer = null;
    }
    if (this._writeStream) {
      this._writeStream.end();
      this._writeStream = null;
    }
  }
}

// ============================================================================
// Convenience Exports
// ============================================================================

export const Timing = TimingHarness;
export const collector = TimingCollector.instance;
