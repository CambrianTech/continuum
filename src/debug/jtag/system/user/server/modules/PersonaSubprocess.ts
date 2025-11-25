/**
 * PersonaSubprocess - Base class for all persona background processes
 *
 * Inspired by cbar's QueueThread<T> pattern:
 * - Base class handles ALL threading/queue logic
 * - Implementations ONLY override handleTask()
 * - Pass entire persona for direct property access
 * - Priority-based adaptive timing
 * - Non-blocking queue operations
 * - Built-in per-subprocess logging (this.log())
 *
 * Usage:
 * ```typescript
 * class MyWorker extends PersonaSubprocess<MyTask> {
 *   protected async handleTask(task: MyTask): Promise<boolean> {
 *     this.log('Processing task...');
 *     // Access persona directly: this.persona.inbox, this.persona.state
 *     return true;
 *   }
 * }
 * ```
 */

import type { PersonaUser } from '../PersonaUser';

export type SubprocessPriority = 'highest' | 'high' | 'moderate' | 'default' | 'low' | 'lowest';

export abstract class PersonaSubprocess<T = void> {
  protected readonly persona: PersonaUser;
  protected readonly priority: SubprocessPriority;
  protected readonly name: string;
  protected running: boolean = false;

  private queue: T[] = [];
  private wakeupSignal: boolean = false;
  private hasRun: boolean = false;
  private readonly maxQueueSize: number;

  /**
   * @param persona - Full persona object (like cbar's parent pointer)
   * @param priority - Thread priority (affects wait time)
   * @param maxQueueSize - Maximum queue depth (older items dropped)
   */
  constructor(
    persona: PersonaUser,
    options: {
      priority?: SubprocessPriority;
      maxQueueSize?: number;
      name?: string;
    } = {}
  ) {
    this.persona = persona;
    this.priority = options.priority ?? 'default';
    this.maxQueueSize = options.maxQueueSize ?? 100;
    this.name = options.name ?? this.constructor.name;
  }

  /**
   * Start the subprocess (non-blocking)
   */
  async start(): Promise<void> {
    if (this.running) {
      console.warn(`⚠️ [${this.name}] Already running`);
      return;
    }

    this.running = true;
    console.log(`▶️ [${this.name}] Started (priority: ${this.priority})`);

    // Auto-log start event
    this.log(`Started (priority: ${this.priority})`);

    // Start service loop (non-blocking)
    setImmediate(() => this.serviceLoop());
  }

  /**
   * Stop the subprocess
   */
  async stop(): Promise<void> {
    if (!this.running) return;

    this.running = false;
    console.log(`⏹️ [${this.name}] Stopping...`);

    // Auto-log stop event
    this.log('Stopped');
  }

  /**
   * Add task to queue (non-blocking)
   * Like cbar's addItem()
   */
  enqueue(task: T): void {
    if (!this.running) return;

    // Drop oldest if queue full (like cbar)
    while (this.queue.length >= this.maxQueueSize) {
      this.queue.shift();
    }

    this.queue.push(task);

    // Immediate wakeup for high priority or manual trigger
    if (this.priority === 'high' || this.priority === 'highest' || this.wakeupSignal || !this.hasRun) {
      this.wakeupSignal = false;
      this.hasRun = true;
      // Signal will be processed in next iteration
    }
  }

  /**
   * Manual wakeup (like cbar's wakeup())
   */
  wakeup(): void {
    this.wakeupSignal = true;
  }

  /**
   * Flush queue
   */
  flush(): void {
    this.queue = [];
  }

  /**
   * Get current queue depth
   */
  getQueueSize(): number {
    return this.queue.length;
  }

  /**
   * Get subprocess status
   */
  getStatus(): {
    name: string;
    running: boolean;
    priority: SubprocessPriority;
    queueSize: number;
    maxQueueSize: number;
  } {
    return {
      name: this.name,
      running: this.running,
      priority: this.priority,
      queueSize: this.queue.length,
      maxQueueSize: this.maxQueueSize
    };
  }

  // ==================== Private Methods ====================

  /**
   * Main service loop (base class handles ALL logic)
   * Like cbar's QueueThread::run()
   */
  private async serviceLoop(): Promise<void> {
    while (this.running) {
      try {
        // Process queue if not empty
        if (this.queue.length > 0) {
          const task = this.queue.shift()!;

          // Call implementation (only thing subclasses override)
          await this.handleTask(task);
        } else {
          // Call tick for periodic work (optional)
          await this.tick();
        }

        // Priority-based wait time (like cbar)
        const waitTime = this.getWaitTime();
        await this.sleep(waitTime);
      } catch (error) {
        console.error(`❌ [${this.name}] Error in service loop:`, error);
        // Back off on error
        await this.sleep(1000);
      }
    }

    console.log(`⏹️ [${this.name}] Stopped`);
  }

  /**
   * Priority-based wait time (like cbar's timedWait)
   *
   * Highest: 10ms
   * High: 50ms
   * Moderate: 100ms
   * Default: 200ms
   * Low: 500ms
   * Lowest: 1000ms
   */
  private getWaitTime(): number {
    switch (this.priority) {
      case 'highest': return 10;
      case 'high': return 50;
      case 'moderate': return 100;
      case 'default': return 200;
      case 'low': return 500;
      case 'lowest': return 1000;
    }
  }

  /**
   * Sleep (non-blocking)
   */
  private async sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Log to subprocess-specific file (non-blocking, queued)
   *
   * Each subprocess gets its own log file (e.g., logs/hippocampus.log)
   * to avoid flooding the main log.
   *
   * Automatically includes:
   * - Timestamp (ISO 8601)
   * - Persona identification ([@name])
   * - Subprocess name
   *
   * Usage: this.log('Tick started')
   * Output: [2025-11-22T10:30:45.123Z] [@Grok] [Hippocampus] Tick started
   *
   * NOTE: Uses PersonaLogger queue - non-blocking, batched writes
   */
  protected log(message: string): void {
    const logLine = this.formatLogLine(message);

    // Enqueue to PersonaLogger (non-blocking, instant return)
    // PersonaLogger will batch and flush asynchronously
    if (this.persona.logger) {
      const fileName = `${this.name.toLowerCase()}.log`;
      this.persona.logger.enqueueLog(fileName, logLine);
    } else {
      // Fallback to console if logger not available (shouldn't happen)
      console.log(`[${this.name}] ${logLine.trim()}`);
    }
  }

  /**
   * Format a log line with consistent structure
   *
   * Format: [timestamp] [@persona] [subprocess] message
   */
  private formatLogLine(message: string): string {
    const timestamp = new Date().toISOString();
    const personaName = this.persona.entity.displayName;
    return `[${timestamp}] [@${personaName}] [${this.name}] ${message}\n`;
  }

  // ==================== Abstract Methods ====================

  /**
   * Handle a task from the queue
   *
   * THIS IS THE ONLY METHOD SUBCLASSES OVERRIDE
   *
   * @param task - Task to process
   * @returns true if successful, false otherwise
   */
  protected abstract handleTask(task: T): Promise<boolean>;

  /**
   * Optional: Periodic tick when queue is empty
   *
   * Override this for continuous processing (like memory consolidation checks)
   */
  protected async tick(): Promise<void> {
    // Default: do nothing
  }
}

/**
 * Subprocess without a queue (for continuous processing)
 *
 * Like memory consolidation - always checking, no explicit tasks
 */
export abstract class PersonaContinuousSubprocess extends PersonaSubprocess<void> {
  constructor(persona: PersonaUser, options: Omit<ConstructorParameters<typeof PersonaSubprocess>[1], 'maxQueueSize'> = {}) {
    super(persona, { ...options, maxQueueSize: 0 });
  }

  // No task handling, just continuous ticking
  protected async handleTask(_task: void): Promise<boolean> {
    return true;
  }

  /**
   * Continuous work - called every cycle
   */
  protected abstract tick(): Promise<void>;
}
