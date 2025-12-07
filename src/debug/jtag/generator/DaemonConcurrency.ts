/**
 * Daemon Concurrency Helpers
 *
 * Reusable concurrency primitives for generated daemons.
 * Each daemon can configure these based on their needs.
 *
 * See: docs/patterns/DAEMON-CONCURRENCY-PATTERN.md
 */

/**
 * Token Bucket Rate Limiter
 * Rejects requests when rate limit exceeded (fast rejection)
 */
export class RateLimiter {
  private tokens: number;
  private readonly maxTokens: number;
  private readonly refillRate: number; // tokens per second
  private lastRefill: number;

  constructor(maxTokens: number, refillRate: number) {
    this.maxTokens = maxTokens;
    this.refillRate = refillRate;
    this.tokens = maxTokens;
    this.lastRefill = Date.now();
  }

  /**
   * Try to consume a token
   * @returns true if request allowed, false if rate limited
   */
  tryConsume(): boolean {
    this.refill();

    if (this.tokens >= 1) {
      this.tokens -= 1;
      return true;
    }

    return false;
  }

  private refill(): void {
    const now = Date.now();
    const timePassed = (now - this.lastRefill) / 1000; // seconds
    const tokensToAdd = timePassed * this.refillRate;

    this.tokens = Math.min(this.maxTokens, this.tokens + tokensToAdd);
    this.lastRefill = now;
  }

  /**
   * Get current token count (for monitoring)
   */
  get availableTokens(): number {
    this.refill();
    return this.tokens;
  }
}

/**
 * Async Request Queue
 * Serializes operations to prevent race conditions
 */
export class AsyncQueue<T> {
  private queue: Array<() => Promise<T>> = [];
  private processing = false;

  /**
   * Add task to queue
   * @returns Promise that resolves when task completes
   */
  async enqueue(task: () => Promise<T>): Promise<T> {
    return new Promise((resolve, reject) => {
      this.queue.push(async () => {
        try {
          const result = await task();
          resolve(result);
          return result;
        } catch (error) {
          reject(error);
          throw error;
        }
      });

      this.processQueue();
    });
  }

  private async processQueue(): Promise<void> {
    if (this.processing) return;

    this.processing = true;

    while (this.queue.length > 0) {
      const task = this.queue.shift();
      if (task) {
        try {
          await task();
        } catch (error) {
          // Error already handled in enqueue promise
        }
      }
    }

    this.processing = false;
  }

  /**
   * Get queue size (for monitoring)
   */
  get size(): number {
    return this.queue.length;
  }
}

/**
 * Semaphore for concurrency control
 * Limits number of concurrent operations
 */
export class Semaphore {
  private permits: number;
  private readonly maxPermits: number;
  private waitQueue: Array<() => void> = [];

  constructor(maxPermits: number) {
    this.maxPermits = maxPermits;
    this.permits = maxPermits;
  }

  /**
   * Acquire permit (waits if none available)
   */
  async acquire(): Promise<void> {
    if (this.permits > 0) {
      this.permits--;
      return Promise.resolve();
    }

    return new Promise<void>((resolve) => {
      this.waitQueue.push(resolve);
    });
  }

  /**
   * Release permit (allows waiting task to proceed)
   */
  release(): void {
    const next = this.waitQueue.shift();
    if (next) {
      next();
    } else {
      this.permits = Math.min(this.maxPermits, this.permits + 1);
    }
  }

  /**
   * Get available permits (for monitoring)
   */
  get available(): number {
    return this.permits;
  }

  /**
   * Get queue size (for monitoring)
   */
  get queueSize(): number {
    return this.waitQueue.length;
  }
}

/**
 * Daemon Metrics
 * Tracks performance and resource usage
 */
export interface DaemonMetricsData {
  totalRequests: number;
  successfulRequests: number;
  failedRequests: number;
  rateLimitedRequests: number;
  avgResponseTime: number;
  maxResponseTime: number;
  queueSize: number;
  activeWorkers: number;
}

export class DaemonMetrics {
  private totalRequests = 0;
  private successfulRequests = 0;
  private failedRequests = 0;
  private rateLimitedRequests = 0;
  private responseTimes: number[] = [];
  private maxResponseTime = 0;

  recordRequest(): void {
    this.totalRequests++;
  }

  recordSuccess(responseTime: number): void {
    this.successfulRequests++;
    this.responseTimes.push(responseTime);
    this.maxResponseTime = Math.max(this.maxResponseTime, responseTime);

    // Keep only last 100 response times to avoid memory growth
    if (this.responseTimes.length > 100) {
      this.responseTimes.shift();
    }
  }

  recordFailure(): void {
    this.failedRequests++;
  }

  recordRateLimited(): void {
    this.rateLimitedRequests++;
  }

  /**
   * Get current metrics snapshot
   */
  getSnapshot(queueSize: number = 0, activeWorkers: number = 0): DaemonMetricsData {
    const avgResponseTime = this.responseTimes.length > 0
      ? this.responseTimes.reduce((a, b) => a + b, 0) / this.responseTimes.length
      : 0;

    return {
      totalRequests: this.totalRequests,
      successfulRequests: this.successfulRequests,
      failedRequests: this.failedRequests,
      rateLimitedRequests: this.rateLimitedRequests,
      avgResponseTime,
      maxResponseTime: this.maxResponseTime,
      queueSize,
      activeWorkers
    };
  }

  /**
   * Reset all metrics
   */
  reset(): void {
    this.totalRequests = 0;
    this.successfulRequests = 0;
    this.failedRequests = 0;
    this.rateLimitedRequests = 0;
    this.responseTimes = [];
    this.maxResponseTime = 0;
  }
}
