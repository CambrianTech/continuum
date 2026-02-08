/**
 * ORM Logger - Operation logging and metrics for migration
 *
 * Tracks:
 * - Operation counts per collection
 * - Latency per operation type
 * - Shadow mode mismatches
 * - Errors and warnings
 */

import { shouldLog, LOG_ALL_OPERATIONS } from './ORMConfig';

export type ORMOperation = 'store' | 'query' | 'read' | 'update' | 'remove' | 'count' | 'batch' | 'vectorSearch' | 'listCollections' | 'clear' | 'clearAll' | 'truncate';

interface OperationMetrics {
  count: number;
  totalMs: number;
  maxMs: number;
  errors: number;
}

interface CollectionMetrics {
  operations: Record<ORMOperation, OperationMetrics>;
  shadowMismatches: number;
}

/**
 * In-memory metrics storage
 */
const metrics: Record<string, CollectionMetrics> = {};

/**
 * Get or create metrics for a collection
 */
function getMetrics(collection: string): CollectionMetrics {
  if (!metrics[collection]) {
    metrics[collection] = {
      operations: {
        store: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        query: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        read: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        update: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        remove: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        count: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        batch: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        vectorSearch: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        listCollections: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        clear: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        clearAll: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
        truncate: { count: 0, totalMs: 0, maxMs: 0, errors: 0 },
      },
      shadowMismatches: 0,
    };
  }
  return metrics[collection];
}

/**
 * Log an operation start (returns function to call on completion)
 */
export function logOperationStart(
  operation: ORMOperation,
  collection: string,
  details?: Record<string, unknown>
): () => void {
  const startTime = Date.now();
  const shouldLogThis = shouldLog(collection) || LOG_ALL_OPERATIONS;

  if (shouldLogThis) {
    console.log(`[ORM] ${operation} ${collection}`, details ? JSON.stringify(details).slice(0, 200) : '');
  }

  return () => {
    const durationMs = Date.now() - startTime;
    const m = getMetrics(collection).operations[operation];
    m.count++;
    m.totalMs += durationMs;
    m.maxMs = Math.max(m.maxMs, durationMs);

    if (shouldLogThis) {
      console.log(`[ORM] ${operation} ${collection} completed in ${durationMs}ms`);
    }

    // Warn on slow operations
    if (durationMs > 100) {
      console.warn(`[ORM] SLOW: ${operation} ${collection} took ${durationMs}ms`);
    }
  };
}

/**
 * Log an operation error
 */
export function logOperationError(
  operation: ORMOperation,
  collection: string,
  error: unknown
): void {
  const m = getMetrics(collection).operations[operation];
  m.errors++;

  console.error(`[ORM] ERROR: ${operation} ${collection}:`, error);
}

/**
 * Log a shadow mode mismatch
 */
export function logShadowMismatch(
  operation: ORMOperation,
  collection: string,
  tsResult: unknown,
  rustResult: unknown,
  differences: string[]
): void {
  getMetrics(collection).shadowMismatches++;

  console.error(`[ORM] SHADOW MISMATCH: ${operation} ${collection}`);
  console.error('  Differences:', differences);
  console.error('  TypeScript result:', JSON.stringify(tsResult).slice(0, 500));
  console.error('  Rust result:', JSON.stringify(rustResult).slice(0, 500));
}

/**
 * Get metrics summary for all collections
 */
export function getMetricsSummary(): Record<string, {
  totalOperations: number;
  totalErrors: number;
  avgLatencyMs: number;
  shadowMismatches: number;
}> {
  const summary: Record<string, {
    totalOperations: number;
    totalErrors: number;
    avgLatencyMs: number;
    shadowMismatches: number;
  }> = {};

  for (const [collection, m] of Object.entries(metrics)) {
    let totalOps = 0;
    let totalErrors = 0;
    let totalMs = 0;

    for (const op of Object.values(m.operations)) {
      totalOps += op.count;
      totalErrors += op.errors;
      totalMs += op.totalMs;
    }

    summary[collection] = {
      totalOperations: totalOps,
      totalErrors: totalErrors,
      avgLatencyMs: totalOps > 0 ? Math.round(totalMs / totalOps) : 0,
      shadowMismatches: m.shadowMismatches,
    };
  }

  return summary;
}

/**
 * Reset all metrics (for testing)
 */
export function resetMetrics(): void {
  for (const key of Object.keys(metrics)) {
    delete metrics[key];
  }
}

/**
 * Print metrics summary to console
 */
export function printMetricsSummary(): void {
  const summary = getMetricsSummary();
  console.log('\n[ORM] === Metrics Summary ===');
  for (const [collection, m] of Object.entries(summary)) {
    console.log(`  ${collection}: ${m.totalOperations} ops, ${m.totalErrors} errors, ${m.avgLatencyMs}ms avg, ${m.shadowMismatches} mismatches`);
  }
  console.log('=============================\n');
}
