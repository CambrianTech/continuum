/**
 * Hippocampus - Bidirectional Memory Consolidation
 *
 * Inspired by the biological hippocampus:
 * - CONSOLIDATE: Working memory → Pattern detection → Long-term storage
 * - ACTIVATE: Long-term storage → Retrieve patterns → Working memory
 *
 * Like U-Net or virtual memory paging:
 * - Compress/decompress
 * - Page in/out of working memory
 * - LRU eviction when full
 *
 * This is the MINIMAL first version - just proves threading works
 * Future: Add actual consolidation logic after validation
 */

import { PersonaContinuousSubprocess } from '../../PersonaSubprocess';
import type { PersonaUser } from '../../../PersonaUser';

/**
 * Snapshot of persona state at tick time
 * Used for logging and future consolidation decisions
 */
interface PersonaStateSnapshot {
  readonly inboxSize: number;
  readonly energy: number;
  readonly attention: number;
}

/**
 * Memory consolidation metrics
 * Phase 2: Will track consolidation operations
 */
interface ConsolidationMetrics {
  readonly tickCount: number;
  readonly lastConsolidation: Date | null;
  readonly patternsDetected: number;
  readonly patternsStored: number;
}

/**
 * Hippocampus - Continuous memory consolidation subprocess
 *
 * Phase 1 (CURRENT): Minimal logging-only version
 * - Just tick and log
 * - Prove threading works
 * - No actual consolidation yet
 *
 * Phase 2 (FUTURE): Add consolidation logic
 * - Pattern detection (cosine similarity)
 * - Compress to long-term storage
 * - Activate patterns back to working memory
 */
export class Hippocampus extends PersonaContinuousSubprocess {
  private metrics: ConsolidationMetrics;

  constructor(persona: PersonaUser) {
    super(persona, {
      priority: 'low', // Low priority - don't interfere with response times
      name: 'Hippocampus'
    });

    this.metrics = {
      tickCount: 0,
      lastConsolidation: null,
      patternsDetected: 0,
      patternsStored: 0
    };
  }

  /**
   * Capture current persona state snapshot
   */
  private captureStateSnapshot(): PersonaStateSnapshot {
    const state = this.persona.personaState.getState();
    return {
      inboxSize: this.persona.inbox.getSize(),
      energy: state.energy,
      attention: state.attention
    };
  }

  /**
   * Log state snapshot in structured format
   */
  private logStateSnapshot(snapshot: PersonaStateSnapshot): void {
    this.log(`Inbox: ${snapshot.inboxSize} items`);
    this.log(`State: energy=${snapshot.energy.toFixed(2)}, attention=${snapshot.attention.toFixed(2)}`);
  }

  /**
   * Continuous tick - called every cycle (based on priority)
   *
   * Phase 1: Just log basic state
   * Phase 2: Add pattern detection and consolidation
   */
  protected async tick(): Promise<void> {
    this.metrics = {
      ...this.metrics,
      tickCount: this.metrics.tickCount + 1
    };

    this.log(`Tick #${this.metrics.tickCount} started`);

    const snapshot: PersonaStateSnapshot = this.captureStateSnapshot();
    this.logStateSnapshot(snapshot);

    // Phase 2: Add consolidation logic here
    // - Check if consolidation needed (based on snapshot)
    // - Detect patterns in working memory
    // - Compress to long-term storage
    // - Update metrics

    this.log('Tick complete');
  }

  /**
   * Get current consolidation metrics
   */
  public getMetrics(): Readonly<ConsolidationMetrics> {
    return this.metrics;
  }
}
