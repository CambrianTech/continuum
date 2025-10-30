/**
 * TrainingDataAccumulator - In-memory buffer for continuous learning
 *
 * Accumulates training examples during recipe execution:
 * 1. genome/capture-interaction stores input/output pairs
 * 2. genome/capture-feedback attaches human/AI feedback
 * 3. When batch threshold reached, triggers genome/train
 *
 * PHASE 7.4 - Foundation for recipe-embedded learning
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * Training example with input/output and optional feedback
 */
export interface TrainingExample {
  id: string;                          // Unique ID for this example
  domain: string;                      // Learning domain (conversation, code, etc.)
  roleId: string;                      // Which role produced this example
  personaId?: UUID;                    // PersonaUser who produced this

  // IO pair
  input: string;                       // Context/prompt
  output: string;                      // Generated response
  expectedOutput?: string;             // Gold standard (if available)

  // Quality signals
  feedback?: {
    source: 'human' | 'ai' | 'system';
    rating?: number;                   // 0.0-1.0 quality score
    comments?: string;
    corrections?: string;              // Suggested improvements
  };

  // Metadata
  timestamp: Date;
  contextMetadata?: Record<string, unknown>;
}

/**
 * Interaction capture (genome/capture-interaction command params)
 */
export interface InteractionCapture {
  roleId: string;
  personaId?: UUID;
  domain: string;
  input: string;
  output: string;
  expectedOutput?: string;
  contextMetadata?: Record<string, unknown>;
}

/**
 * Feedback capture (genome/capture-feedback command params)
 */
export interface FeedbackCapture {
  interactionId: string;
  source: 'human' | 'ai' | 'system';
  rating?: number;
  comments?: string;
  corrections?: string;
}

/**
 * Training data accumulator - RAM-based buffer per PersonaUser
 *
 * Stores examples by domain until batch threshold reached,
 * then triggers fine-tuning via genome/train command.
 */
export class TrainingDataAccumulator {
  private domainBuffers: Map<string, TrainingExample[]> = new Map();
  private batchThresholds: Map<string, number> = new Map();
  private interactionIndex: Map<string, TrainingExample> = new Map();
  private nextExampleId: number = 0;

  // Default thresholds
  private readonly DEFAULT_BATCH_SIZE = 50;
  private readonly MIN_BATCH_SIZE = 10;
  private readonly MAX_BATCH_SIZE = 1000;

  constructor(
    private personaId: UUID,
    private displayName: string
  ) {
    console.log(`🧬 ${displayName}: TrainingDataAccumulator initialized`);
  }

  /**
   * Capture interaction (input/output pair) from recipe execution
   */
  async captureInteraction(capture: InteractionCapture): Promise<string> {
    const exampleId = `${this.personaId}-${this.nextExampleId++}`;

    const example: TrainingExample = {
      id: exampleId,
      domain: capture.domain,
      roleId: capture.roleId,
      personaId: capture.personaId || this.personaId,
      input: capture.input,
      output: capture.output,
      expectedOutput: capture.expectedOutput,
      timestamp: new Date(),
      contextMetadata: capture.contextMetadata
    };

    // Store in domain buffer
    if (!this.domainBuffers.has(capture.domain)) {
      this.domainBuffers.set(capture.domain, []);
    }
    this.domainBuffers.get(capture.domain)!.push(example);

    // Index for feedback attachment
    this.interactionIndex.set(exampleId, example);

    const bufferSize = this.domainBuffers.get(capture.domain)!.length;
    console.log(`📝 ${this.displayName}: Captured ${capture.domain} example (buffer: ${bufferSize}/${this.getBatchThreshold(capture.domain)})`);

    return exampleId;
  }

  /**
   * Attach feedback to most recent interaction
   */
  async captureFeedback(feedback: FeedbackCapture): Promise<void> {
    const example = this.interactionIndex.get(feedback.interactionId);

    if (!example) {
      console.warn(`⚠️ ${this.displayName}: Feedback for unknown interaction ${feedback.interactionId}`);
      return;
    }

    example.feedback = {
      source: feedback.source,
      rating: feedback.rating,
      comments: feedback.comments,
      corrections: feedback.corrections
    };

    console.log(`💬 ${this.displayName}: Attached ${feedback.source} feedback to ${example.domain} example`);
  }

  /**
   * Check if domain has enough examples to trigger micro-tuning
   */
  shouldMicroTune(domain: string): boolean {
    const buffer = this.domainBuffers.get(domain);
    if (!buffer) return false;

    const threshold = this.getBatchThreshold(domain);
    return buffer.length >= threshold;
  }

  /**
   * Get current buffer size for domain
   */
  getBufferSize(domain: string): number {
    return this.domainBuffers.get(domain)?.length || 0;
  }

  /**
   * Get batch threshold for domain
   */
  getBatchThreshold(domain: string): number {
    return this.batchThresholds.get(domain) || this.DEFAULT_BATCH_SIZE;
  }

  /**
   * Set batch threshold for domain
   */
  setBatchThreshold(domain: string, threshold: number): void {
    const clamped = Math.max(this.MIN_BATCH_SIZE, Math.min(threshold, this.MAX_BATCH_SIZE));
    this.batchThresholds.set(domain, clamped);
    console.log(`🎚️ ${this.displayName}: Set ${domain} batch threshold to ${clamped}`);
  }

  /**
   * Consume training data for domain (returns and clears buffer)
   */
  async consumeTrainingData(domain: string): Promise<TrainingExample[]> {
    const buffer = this.domainBuffers.get(domain);
    if (!buffer || buffer.length === 0) {
      return [];
    }

    // Clear buffer and return examples
    const examples = [...buffer];
    this.domainBuffers.set(domain, []);

    // Clear interaction index for consumed examples
    for (const example of examples) {
      this.interactionIndex.delete(example.id);
    }

    console.log(`🔄 ${this.displayName}: Consumed ${examples.length} ${domain} examples for training`);

    return examples;
  }

  /**
   * Get all domains with accumulated examples
   */
  getDomains(): string[] {
    return Array.from(this.domainBuffers.keys()).filter(domain =>
      this.domainBuffers.get(domain)!.length > 0
    );
  }

  /**
   * Get stats for all domains
   */
  getStats(): Record<string, { count: number; threshold: number; ready: boolean }> {
    const stats: Record<string, { count: number; threshold: number; ready: boolean }> = {};

    for (const domain of this.domainBuffers.keys()) {
      const count = this.getBufferSize(domain);
      const threshold = this.getBatchThreshold(domain);
      stats[domain] = {
        count,
        threshold,
        ready: count >= threshold
      };
    }

    return stats;
  }

  /**
   * Clear all buffers (for testing)
   */
  clearAll(): void {
    this.domainBuffers.clear();
    this.interactionIndex.clear();
    console.log(`🧹 ${this.displayName}: Cleared all training buffers`);
  }
}
