/**
 * Generator Logger
 * 
 * Clean logging utility for the generator system with proper formatting
 * and configurable output levels.
 */

import type { GeneratorLogger, LogLevel } from '../types/GeneratorTypes';

// ============================================================================
// Console Logger Implementation
// ============================================================================

export class ConsoleLogger implements GeneratorLogger {
  private level: LogLevel;
  private prefix: string;

  constructor(level: LogLevel = 'info', prefix = '🏭 Generator') {
    this.level = level;
    this.prefix = prefix;
  }

  debug(message: string, ...args: unknown[]): void {
    if (this.shouldLog('debug')) {
      console.debug(`${this.prefix} 🔍 ${message}`, ...args);
    }
  }

  info(message: string, ...args: unknown[]): void {
    if (this.shouldLog('info')) {
      console.info(`${this.prefix} ℹ️  ${message}`, ...args);
    }
  }

  warn(message: string, ...args: unknown[]): void {
    if (this.shouldLog('warn')) {
      console.warn(`${this.prefix} ⚠️  ${message}`, ...args);
    }
  }

  error(message: string, ...args: unknown[]): void {
    if (this.shouldLog('error')) {
      console.error(`${this.prefix} ❌ ${message}`, ...args);
    }
  }

  private shouldLog(messageLevel: LogLevel): boolean {
    const levels: Record<LogLevel, number> = {
      debug: 0,
      info: 1,
      warn: 2,
      error: 3
    };
    
    return levels[messageLevel] >= levels[this.level];
  }

  /**
   * Set the minimum log level
   */
  setLevel(level: LogLevel): void {
    this.level = level;
  }

  /**
   * Create a child logger with additional prefix
   */
  child(childPrefix: string): ConsoleLogger {
    return new ConsoleLogger(this.level, `${this.prefix} ${childPrefix}`);
  }
}

// ============================================================================
// Silent Logger (for testing/dry-run)
// ============================================================================

export class SilentLogger implements GeneratorLogger {
  debug(): void {}
  info(): void {}
  warn(): void {}
  error(): void {}
}

// ============================================================================
// Progress Tracker
// ============================================================================

export class ProgressTracker {
  private logger: GeneratorLogger;
  private startTime: number;
  private steps: { name: string; completed: boolean; duration?: number }[] = [];

  constructor(logger: GeneratorLogger) {
    this.logger = logger;
    this.startTime = Date.now();
  }

  /**
   * Add a step to track
   */
  addStep(name: string): void {
    this.steps.push({ name, completed: false });
  }

  /**
   * Mark a step as completed
   */
  completeStep(name: string): void {
    const step = this.steps.find(s => s.name === name);
    if (step) {
      step.completed = true;
      step.duration = Date.now() - this.startTime;
      this.logger.info(`✅ ${name} (${step.duration}ms)`);
    }
  }

  /**
   * Log progress summary
   */
  logSummary(): void {
    const completed = this.steps.filter(s => s.completed).length;
    const total = this.steps.length;
    const totalTime = Date.now() - this.startTime;

    this.logger.info(`📊 Progress: ${completed}/${total} steps completed in ${totalTime}ms`);
    
    for (const step of this.steps) {
      const status = step.completed ? '✅' : '❌';
      const duration = step.duration ? ` (${step.duration}ms)` : '';
      this.logger.info(`   ${status} ${step.name}${duration}`);
    }
  }

  /**
   * Check if all steps are completed
   */
  isComplete(): boolean {
    return this.steps.every(s => s.completed);
  }
}

// ============================================================================
// Statistics Collector
// ============================================================================

export class GeneratorStats {
  private stats: Record<string, number> = {};
  private logger: GeneratorLogger;

  constructor(logger: GeneratorLogger) {
    this.logger = logger;
  }

  /**
   * Increment a counter
   */
  increment(key: string, amount = 1): void {
    this.stats[key] = (this.stats[key] || 0) + amount;
  }

  /**
   * Set a value
   */
  set(key: string, value: number): void {
    this.stats[key] = value;
  }

  /**
   * Get a value
   */
  get(key: string): number {
    return this.stats[key] || 0;
  }

  /**
   * Log all statistics
   */
  logSummary(): void {
    this.logger.info('📈 Generation Statistics:');
    for (const [key, value] of Object.entries(this.stats)) {
      this.logger.info(`   ${key}: ${value}`);
    }
  }

  /**
   * Get all stats as object
   */
  getAll(): Record<string, number> {
    return { ...this.stats };
  }

  /**
   * Reset all statistics
   */
  reset(): void {
    this.stats = {};
  }
}