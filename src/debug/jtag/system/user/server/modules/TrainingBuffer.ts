/**
 * TrainingBuffer - Accumulate training signals before triggering micro-training
 *
 * Part of the continuous micro-LoRA system. Buffers training signals per-trait
 * until we have enough (threshold) to trigger a micro-training session.
 *
 * Key features:
 * - Per-trait buffers (tone corrections don't wait for reasoning corrections)
 * - Threshold-based triggering (train after N signals)
 * - Expiry/pruning (old signals expire after 24 hours)
 * - Cooldown (prevent too-frequent training)
 * - Rate limiting (prevent signal spam)
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { TraitType } from '../../../genome/entities/GenomeLayerEntity';
import type { TrainingSignal } from './SignalDetector';
import { ORM } from '../../../../daemons/data-daemon/shared/ORM';
import { TaskEntity } from '../../../data/entities/TaskEntity';

/**
 * Buffered training example with metadata
 */
export interface BufferedExample {
  signal: TrainingSignal;
  timestamp: number;
  personaId: UUID;
}

/**
 * Training example format for fine-tuning
 */
export interface TrainingExample {
  prompt: string;     // Conversation context
  completion: string; // What the AI should have said (or did say well)
  isPositive: boolean; // Reinforce (positive) or correct (negative)
}

/**
 * Buffer configuration
 */
export interface TrainingBufferConfig {
  threshold: number;          // Minimum examples before training (default: 5)
  maxAgeMs: number;           // Max age before expiry (default: 24 hours)
  cooldownMs: number;         // Minimum time between trainings per trait (default: 10 min)
  maxSignalsPerHour: number;  // Rate limit per trait (default: 10)
}

const DEFAULT_CONFIG: TrainingBufferConfig = {
  threshold: 5,
  maxAgeMs: 24 * 60 * 60 * 1000,  // 24 hours
  cooldownMs: 10 * 60 * 1000,     // 10 minutes
  maxSignalsPerHour: 10,
};

/**
 * Per-persona training buffer
 *
 * Concurrency-safe: Uses per-trait locks to prevent race conditions
 * when multiple messages arrive simultaneously.
 */
/**
 * Logger function type for persona-specific logging
 */
export type BufferLogger = (message: string) => void;

export class TrainingBuffer {
  private readonly personaId: UUID;
  private readonly personaName: string;
  private readonly config: TrainingBufferConfig;
  private _logger: BufferLogger;

  // Per-trait buffers
  private buffers: Map<TraitType, BufferedExample[]> = new Map();

  // Last training time per trait (for cooldown)
  private lastTrainingTime: Map<TraitType, number> = new Map();

  // Signal count in last hour per trait (for rate limiting)
  private recentSignalCount: Map<TraitType, { count: number; resetAt: number }> = new Map();

  // Per-trait locks to prevent concurrent modifications (async race conditions)
  private traitLocks: Map<TraitType, Promise<void>> = new Map();

  constructor(
    personaId: UUID,
    personaName: string,
    config: Partial<TrainingBufferConfig> = {},
    logger?: BufferLogger
  ) {
    this.personaId = personaId;
    this.personaName = personaName;
    this.config = { ...DEFAULT_CONFIG, ...config };
    // Default to console.log for backwards compatibility, but persona-specific logger preferred
    this._logger = logger || ((msg: string) => console.log(`[TrainingBuffer:${personaName}] ${msg}`));
  }

  /**
   * Update the logger (useful when buffer was created before logger was available)
   */
  setLogger(logger: BufferLogger): void {
    this._logger = logger;
  }

  /**
   * Internal logger accessor
   */
  private get logger(): BufferLogger {
    return this._logger;
  }

  /**
   * Acquire a lock for a trait (simple async mutex)
   * Ensures only one operation modifies a trait buffer at a time
   */
  private async acquireLock(trait: TraitType): Promise<() => void> {
    // Wait for any existing operation on this trait to complete
    const existingLock = this.traitLocks.get(trait);
    if (existingLock) {
      await existingLock;
    }

    // Create a new lock
    let releaseLock: () => void = () => {};
    const lockPromise = new Promise<void>(resolve => {
      releaseLock = resolve;
    });
    this.traitLocks.set(trait, lockPromise);

    return releaseLock;
  }

  /**
   * Add a training signal to the buffer
   *
   * Concurrency-safe: Uses per-trait lock to prevent race conditions
   *
   * @returns true if training was triggered, false otherwise
   */
  async add(signal: TrainingSignal): Promise<boolean> {
    const trait = signal.trait;

    // Acquire lock for this trait (prevents concurrent modifications)
    const releaseLock = await this.acquireLock(trait);

    try {
      // Check rate limit
      if (this.isRateLimited(trait)) {
        this.logger(`⏳ Rate limited for trait ${trait}, skipping signal`);
        return false;
      }

      // Get or create buffer for this trait
      const buffer = this.getBuffer(trait);

      // Add signal to buffer
      buffer.push({
        signal,
        timestamp: Date.now(),
        personaId: this.personaId,
      });

      // Update rate limit counter
      this.incrementSignalCount(trait);

      // Prune expired signals
      this.pruneExpired(trait);

      this.logger(`📥 Added ${signal.type} signal for ${trait} (${buffer.length}/${this.config.threshold})`);

      // Check if ready to train
      if (buffer.length >= this.config.threshold && !this.isInCooldown(trait)) {
        // Trigger training (async, but we await it to ensure buffer is flushed before releasing lock)
        await this.triggerTraining(trait);
        return true;
      }

      return false;
    } finally {
      // Always release lock
      releaseLock();
    }
  }

  /**
   * Get current buffer contents for a trait
   */
  getBufferContents(trait: TraitType): BufferedExample[] {
    return this.getBuffer(trait).slice();  // Return copy
  }

  /**
   * Get buffer sizes for all traits
   */
  getBufferStats(): Record<string, { count: number; oldestAge: number }> {
    const stats: Record<string, { count: number; oldestAge: number }> = {};
    const now = Date.now();

    for (const [trait, buffer] of this.buffers.entries()) {
      const oldestAge = buffer.length > 0 ? now - buffer[0].timestamp : 0;
      stats[trait] = { count: buffer.length, oldestAge };
    }

    return stats;
  }

  /**
   * Manually flush a trait buffer (for testing)
   */
  flush(trait: TraitType): BufferedExample[] {
    const buffer = this.buffers.get(trait) || [];
    this.buffers.set(trait, []);
    return buffer;
  }

  /**
   * Clear all buffers (for testing)
   */
  clear(): void {
    this.buffers.clear();
    this.lastTrainingTime.clear();
    this.recentSignalCount.clear();
  }

  // --- Private methods ---

  private getBuffer(trait: TraitType): BufferedExample[] {
    if (!this.buffers.has(trait)) {
      this.buffers.set(trait, []);
    }
    return this.buffers.get(trait)!;
  }

  private pruneExpired(trait: TraitType): void {
    const buffer = this.getBuffer(trait);
    const cutoff = Date.now() - this.config.maxAgeMs;

    // Remove expired signals
    const before = buffer.length;
    const pruned = buffer.filter(ex => ex.timestamp > cutoff);
    this.buffers.set(trait, pruned);

    if (pruned.length < before) {
      this.logger(`🗑️ Pruned ${before - pruned.length} expired signals from ${trait}`);
    }
  }

  private isInCooldown(trait: TraitType): boolean {
    const lastTime = this.lastTrainingTime.get(trait) || 0;
    return Date.now() - lastTime < this.config.cooldownMs;
  }

  private isRateLimited(trait: TraitType): boolean {
    const record = this.recentSignalCount.get(trait);
    if (!record) return false;

    // Reset if hour has passed
    if (Date.now() > record.resetAt) {
      this.recentSignalCount.set(trait, { count: 0, resetAt: Date.now() + 60 * 60 * 1000 });
      return false;
    }

    return record.count >= this.config.maxSignalsPerHour;
  }

  private incrementSignalCount(trait: TraitType): void {
    const record = this.recentSignalCount.get(trait);
    if (!record || Date.now() > record.resetAt) {
      this.recentSignalCount.set(trait, { count: 1, resetAt: Date.now() + 60 * 60 * 1000 });
    } else {
      record.count++;
    }
  }

  /**
   * Trigger training for a trait
   */
  private async triggerTraining(trait: TraitType): Promise<void> {
    const examples = this.flush(trait);
    if (examples.length === 0) return;

    this.logger(`🔥 Triggering micro-training for ${trait} with ${examples.length} examples`);

    // Convert to training format
    const trainingExamples: TrainingExample[] = examples.map(ex => {
      const signal = ex.signal;

      // For positive signals, reinforce what the AI said
      // For negative signals, learn from the user's correction
      const completion = signal.polarity === 'positive'
        ? (signal.originalMessage?.content?.text || signal.context)
        : (signal.userResponse?.content?.text || signal.context);

      return {
        prompt: signal.context,
        completion,
        isPositive: signal.polarity === 'positive',
      };
    });

    // Update cooldown
    this.lastTrainingTime.set(trait, Date.now());

    // Create training task using existing task system
    try {
      // Create task entity with proper structure
      const task = new TaskEntity();
      task.assigneeId = this.personaId;
      task.createdBy = this.personaId;  // Self-created training task
      task.domain = 'self';
      task.taskType = 'fine-tune-lora';
      task.contextId = this.personaId;  // Context is the persona itself
      task.description = `Signal-driven micro-training for ${trait} trait (${examples.length} examples)`;
      task.priority = 0.7;
      task.status = 'pending';
      task.metadata = {
        loraLayer: trait,
        trainingData: trainingExamples as unknown[],
      };

      await ORM.store(TaskEntity.collection, task);
      this.logger(`✅ Created fine-tune-lora task for ${trait}`);
    } catch (error) {
      this.logger(`❌ Failed to create training task: ${error}`);
      // Put examples back on retry? For now, just log
    }
  }
}

/**
 * Per-persona buffer registry
 */
const _bufferRegistry: Map<UUID, TrainingBuffer> = new Map();

/**
 * Get or create a training buffer for a persona
 *
 * @param personaId - The persona's unique ID
 * @param personaName - The persona's display name (for default logging)
 * @param logger - Optional persona-specific logger function
 */
export function getTrainingBuffer(personaId: UUID, personaName: string, logger?: BufferLogger): TrainingBuffer {
  let buffer = _bufferRegistry.get(personaId);
  if (!buffer) {
    buffer = new TrainingBuffer(personaId, personaName, {}, logger);
    _bufferRegistry.set(personaId, buffer);
  } else if (logger) {
    // Update logger if provided (buffer may have been created before logger was available)
    buffer.setLogger(logger);
  }
  return buffer;
}

/**
 * Clear all buffers (for testing)
 */
export function clearAllTrainingBuffers(): void {
  _bufferRegistry.clear();
}
