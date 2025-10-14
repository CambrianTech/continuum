/**
 * Thought Stream Coordinator - Event-Driven RTOS-Style Architecture
 *
 * Classic concurrency primitives applied to AI social behavior:
 * - mutex → claiming exclusive response right
 * - semaphore → limited response slots (maxResponders)
 * - signal → broadcasting thoughts to stream
 * - condition variable → waiting for decision
 *
 * CRITICAL: All operations are event-driven, no polling/timeouts
 * Personas observe the thought stream and react naturally
 */

import { EventEmitter } from 'events';
import type { UUID } from '../../core/types/JTAGTypes';
import type {
  Thought,
  ThoughtType,
  ThoughtStream,
  CoordinationDecision,
  CoordinationConfig
} from '../shared/ConversationCoordinationTypes';
import { DEFAULT_COORDINATION_CONFIG } from '../shared/ConversationCoordinationTypes';

/**
 * Thought Stream Coordinator - RTOS-inspired AI social coordination
 */
export class ThoughtStreamCoordinator extends EventEmitter {
  private config: CoordinationConfig;

  /** Active thought streams by messageId */
  private streams: Map<string, ThoughtStream> = new Map();

  /** Sequential evaluation queue - ensures personas evaluate one at a time */
  private evaluationQueue: Map<string, Promise<void>> = new Map();

  /** Cleanup timer */
  private cleanupInterval: NodeJS.Timeout;

  constructor(config: Partial<CoordinationConfig> = {}) {
    super();
    this.config = { ...DEFAULT_COORDINATION_CONFIG, ...config };

    // Cleanup old streams every 30 seconds
    this.cleanupInterval = setInterval(() => {
      this.cleanup();
    }, 30000);

    if (this.config.enableLogging) {
      console.log('🧠 ThoughtStreamCoordinator: Initialized', this.config);
    }
  }

  /**
   * Request evaluation turn (ensures sequential evaluation with random ordering)
   * Brain-like: One persona thinks at a time, but order is randomized for fairness
   *
   * Usage:
   *   const releaseTurn = await coordinator.requestEvaluationTurn(messageId, personaId);
   *   try {
   *     await evaluateAndRespond();
   *   } finally {
   *     releaseTurn();
   *   }
   */
  async requestEvaluationTurn(messageId: string, personaId: UUID): Promise<() => void> {
    // Wait for any previous evaluation on this message to complete
    const existingEvaluation = this.evaluationQueue.get(messageId);
    if (existingEvaluation) {
      await existingEvaluation;
    }

    // Create promise for this evaluation (others will wait for it)
    let resolveEvaluation: () => void;
    const evaluationPromise = new Promise<void>((resolve) => {
      resolveEvaluation = resolve;
    });

    this.evaluationQueue.set(messageId, evaluationPromise);

    // Add small random delay (10-100ms) to simulate neural timing variance
    const randomDelay = Math.random() * 90 + 10;
    await new Promise(resolve => setTimeout(resolve, randomDelay));

    if (this.config.enableLogging) {
      console.log(`🎲 ${personaId.slice(0, 8)}: Got evaluation turn (after ${randomDelay.toFixed(0)}ms delay)`);
    }

    // Return resolver so caller can release the turn when done
    return resolveEvaluation!;
  }

  /**
   * Broadcast a thought to the stream (SIGNAL primitive)
   * Event-driven: immediately notifies all observers
   */
  async broadcastThought(messageId: string, thought: Thought): Promise<void> {
    try {
      const stream = this.getOrCreateStream(messageId, thought.personaId);

      // Add to immutable log
      stream.thoughts.push(thought);

      // Update mutable state
      stream.considerations.set(thought.personaId, thought);

      if (this.config.enableLogging) {
        console.log(`🧠 Thought: ${thought.personaId.slice(0, 8)} → ${thought.type} (conf=${thought.confidence})`);
        console.log(`   Reasoning: ${thought.reasoning}`);
      }

      // SEMAPHORE: Handle claiming/deferring
      if (thought.type === 'claiming') {
        await this.handleClaim(stream, thought);
      } else if (thought.type === 'deferring') {
        await this.handleDefer(stream, thought);
      }

      // Emit event for observers (SIGNAL to condition variables)
      this.emit(`thought:${messageId}`, thought);
      this.emit('thought', messageId, thought);

      // Schedule decision after intention window (if not already scheduled)
      if (!stream.decisionTimer) {
        stream.decisionTimer = setTimeout(async () => {
          if (stream.phase === 'gathering') {
            if (this.config.enableLogging) {
              console.log(`⏰ Intention window expired for ${messageId.slice(0, 8)}, making decision...`);
            }
            await this.makeDecision(stream);
          }
        }, this.config.intentionWindowMs);
      }

      // Early decision only if ALL personas have responded (optimization)
      if (this.canDecide(stream)) {
        clearTimeout(stream.decisionTimer);
        stream.decisionTimer = undefined;
        await this.makeDecision(stream);
      }

    } catch (error) {
      console.error('❌ ThoughtStreamCoordinator.broadcastThought: Error (non-fatal):', error);
    }
  }

  /**
   * Handle claim attempt (MUTEX acquisition)
   */
  private async handleClaim(stream: ThoughtStream, thought: Thought): Promise<void> {
    // SEMAPHORE: Check if slots available
    if (stream.availableSlots > 0) {
      stream.availableSlots--;
      stream.claimedBy.add(thought.personaId);

      if (this.config.enableLogging) {
        console.log(`🔒 Claim: ${thought.personaId.slice(0, 8)} acquired slot (${stream.availableSlots} remaining)`);
      }
    } else {
      if (this.config.enableLogging) {
        console.log(`❌ Claim: ${thought.personaId.slice(0, 8)} no slots available`);
      }
    }
  }

  /**
   * Handle defer (MUTEX release)
   */
  private async handleDefer(stream: ThoughtStream, thought: Thought): Promise<void> {
    if (stream.claimedBy.has(thought.personaId)) {
      stream.claimedBy.delete(thought.personaId);
      stream.availableSlots++;

      if (this.config.enableLogging) {
        console.log(`🔓 Defer: ${thought.personaId.slice(0, 8)} released slot (${stream.availableSlots} available)`);
      }
    }
  }

  /**
   * Wait for decision (CONDITION VARIABLE primitive)
   * Async/await replaces polling - much cleaner!
   */
  async waitForDecision(messageId: string, timeoutMs: number = 5000): Promise<CoordinationDecision | null> {
    const stream = this.streams.get(messageId);
    if (!stream) {
      if (this.config.enableLogging) {
        console.log(`🧠 WaitForDecision: No stream for ${messageId.slice(0, 8)} - fallback`);
      }
      return null; // Graceful degradation
    }

    if (stream.phase === 'decided' && stream.decision) {
      return stream.decision; // Already decided
    }

    // Setup condition variable (Promise-based)
    if (!stream.decisionSignal) {
      stream.decisionSignal = new Promise<CoordinationDecision>((resolve) => {
        stream.signalResolver = resolve;
      });
    }

    // Wait with timeout (fallback safety)
    const timeoutPromise = new Promise<null>((resolve) => {
      setTimeout(() => resolve(null), timeoutMs);
    });

    const result = await Promise.race([stream.decisionSignal, timeoutPromise]);

    if (!result && this.config.enableLogging) {
      console.log(`⏰ WaitForDecision: Timeout for ${messageId.slice(0, 8)} - fallback`);
    }

    return result;
  }

  /**
   * Check if we can make a decision (HEURISTICS)
   */
  private canDecide(stream: ThoughtStream): boolean {
    if (stream.phase === 'decided') {
      return false; // Already decided
    }

    const thoughts = Array.from(stream.considerations.values());
    const claims = thoughts.filter(t => t.type === 'claiming');
    const deferrals = thoughts.filter(t => t.type === 'deferring');

    // RULE 1: Clear winner - one claim with very high confidence
    if (claims.length === 1 && claims[0].confidence > 90) {
      if (this.config.enableLogging) {
        console.log(`🎯 CanDecide: Clear winner (conf=${claims[0].confidence})`);
      }
      return true;
    }

    // RULE 2: All slots claimed
    if (stream.availableSlots === 0 && stream.claimedBy.size > 0) {
      if (this.config.enableLogging) {
        console.log(`🎯 CanDecide: All slots claimed (${stream.claimedBy.size})`);
      }
      return true;
    }

    // RULE 3: Everyone has decided (claim or defer)
    const totalPersonas = thoughts.length;
    if (totalPersonas > 0 && claims.length + deferrals.length === totalPersonas) {
      if (this.config.enableLogging) {
        console.log(`🎯 CanDecide: Everyone decided (${claims.length} claims, ${deferrals.length} deferrals)`);
      }
      return true;
    }

    // RULE 4: Timeout fallback - 3 seconds elapsed with at least one claim
    const elapsed = Date.now() - stream.startTime;
    if (elapsed > 3000 && claims.length > 0) {
      if (this.config.enableLogging) {
        console.log(`⏰ CanDecide: Timeout fallback (${elapsed}ms, ${claims.length} claims)`);
      }
      return true;
    }

    return false;
  }

  /**
   * Make final decision and signal waiters (CONDITION VARIABLE broadcast)
   */
  private async makeDecision(stream: ThoughtStream): Promise<void> {
    if (stream.phase === 'decided') {
      return; // Already decided
    }

    stream.phase = 'deliberating';

    const thoughts = Array.from(stream.considerations.values());
    const claims = thoughts.filter(t => t.type === 'claiming');

    // Sort claims by confidence
    const sortedClaims = claims.sort((a, b) => b.confidence - a.confidence);

    // Grant slots to top claimers (up to maxResponders)
    const granted: UUID[] = [];
    const denied: UUID[] = [];
    const reasoning: string[] = [];

    // FIX 1: If only one claimant, ALWAYS grant (regardless of confidence)
    if (sortedClaims.length === 1) {
      const claim = sortedClaims[0];
      granted.push(claim.personaId);
      reasoning.push(`${claim.personaId.slice(0, 8)} is only claimant (conf=${claim.confidence.toFixed(2)}) - auto-granted`);

      if (this.config.enableLogging) {
        console.log(`✅ Decision: Only one claimant, auto-granting ${claim.personaId.slice(0, 8)}`);
      }
    }
    // FIX 2: If NO claimants but some personas evaluated, lower the bar and check deferrals
    else if (sortedClaims.length === 0) {
      if (this.config.enableLogging) {
        console.log(`⚠️  Decision: No claimants - all personas deferred or went silent`);
      }
      reasoning.push('No personas claimed - all went silent');
    }
    // Normal case: Multiple claimants
    else {
      // FIX 3: Dynamic minConfidence based on context
      // - If many high-confidence claims: use normal threshold
      // - If all low confidence: lower the threshold
      const avgConfidence = sortedClaims.reduce((sum, c) => sum + c.confidence, 0) / sortedClaims.length;
      const dynamicMinConfidence = avgConfidence < 0.4 ? 0.2 : this.config.minConfidence;

      if (this.config.enableLogging && dynamicMinConfidence !== this.config.minConfidence) {
        console.log(`🎚️  Decision: Lowered minConfidence ${this.config.minConfidence} → ${dynamicMinConfidence} (avg conf=${avgConfidence.toFixed(2)})`);
      }

      for (let i = 0; i < Math.min(sortedClaims.length, this.config.maxResponders); i++) {
        const claim = sortedClaims[i];
        if (claim.confidence >= dynamicMinConfidence) {
          granted.push(claim.personaId);
          reasoning.push(`${claim.personaId.slice(0, 8)} claimed (conf=${claim.confidence.toFixed(2)})`);
        } else {
          reasoning.push(`${claim.personaId.slice(0, 8)} below threshold (conf=${claim.confidence.toFixed(2)} < ${dynamicMinConfidence.toFixed(2)})`);
        }
      }
    }

    // Everyone else denied
    for (const thought of thoughts) {
      if (!granted.includes(thought.personaId)) {
        denied.push(thought.personaId);
      }
    }

    const decision: CoordinationDecision = {
      messageId: stream.messageId,
      intentions: thoughts.map(t => ({
        personaId: t.personaId,
        contextId: stream.contextId,
        messageId: stream.messageId,
        confidence: t.confidence,
        urgency: 50, // TODO: Add urgency to Thought
        responseType: 'answer', // TODO: Map from ThoughtType
        relevanceScore: t.confidence,
        wasMentioned: false, // TODO: Track this
        timestamp: t.timestamp
      })),
      granted,
      denied,
      reasoning: reasoning.join('; '),
      timestamp: new Date(),
      coordinationDurationMs: Date.now() - stream.startTime
    };

    stream.decision = decision;
    stream.phase = 'decided';

    if (this.config.enableLogging) {
      console.log(`🎯 Decision: ${stream.messageId.slice(0, 8)} → ${granted.length} granted, ${denied.length} denied (${decision.coordinationDurationMs}ms)`);
      console.log(`   Reasoning: ${decision.reasoning}`);
    }

    // CONDITION VARIABLE: Signal all waiters
    if (stream.signalResolver) {
      stream.signalResolver(decision);
    }

    // Emit event for observers
    this.emit(`decision:${stream.messageId}`, decision);
    this.emit('decision', stream.messageId, decision);
  }

  /**
   * Check if persona was granted permission
   */
  async checkPermission(personaId: UUID, messageId: string): Promise<boolean> {
    const stream = this.streams.get(messageId);
    if (!stream) {
      return true; // Graceful degradation
    }

    if (stream.phase !== 'decided' || !stream.decision) {
      return false; // Not decided yet
    }

    return stream.decision.granted.includes(personaId);
  }

  /**
   * Get probabilistic maxResponders (per-message, not fixed)
   * - 70% chance: 1 responder (focused)
   * - 25% chance: 2 responders (discussion)
   * - 5% chance: 3 responders (lively debate)
   */
  private getProbabilisticMaxResponders(): number {
    const rand = Math.random();
    if (rand < 0.70) return 1;  // 70% - focused
    if (rand < 0.95) return 2;  // 25% - discussion
    return 3;                    // 5% - lively
  }

  /**
   * Get or create thought stream (with dynamic slot allocation)
   */
  private getOrCreateStream(messageId: string, contextId: UUID): ThoughtStream {
    let stream = this.streams.get(messageId);

    if (!stream) {
      // Probabilistic slot allocation per message
      const maxResponders = this.getProbabilisticMaxResponders();

      stream = {
        messageId,
        contextId,
        phase: 'gathering',
        thoughts: [],
        considerations: new Map(),
        startTime: Date.now(),
        availableSlots: maxResponders,
        claimedBy: new Set()
      };

      this.streams.set(messageId, stream);

      if (this.config.enableLogging) {
        console.log(`🧠 Stream: Created for message ${messageId.slice(0, 8)} (slots=${maxResponders})`);
      }
    }

    return stream;
  }

  /**
   * Cleanup old streams
   */
  private cleanup(): void {
    const now = Date.now();
    const maxAge = 60000; // 1 minute

    for (const [messageId, stream] of Array.from(this.streams.entries())) {
      if (now - stream.startTime > maxAge) {
        this.streams.delete(messageId);
      }
    }

    if (this.config.enableLogging && this.streams.size > 0) {
      console.log(`🧠 Cleanup: ${this.streams.size} active streams`);
    }
  }

  /**
   * Get statistics
   */
  getStats(): {
    activeStreams: number;
    totalDecisions: number;
    avgThoughtsPerMessage: number;
  } {
    const decided = Array.from(this.streams.values()).filter(s => s.phase === 'decided');
    const totalThoughts = decided.reduce((sum, s) => sum + s.thoughts.length, 0);

    return {
      activeStreams: this.streams.size,
      totalDecisions: decided.length,
      avgThoughtsPerMessage: decided.length > 0 ? totalThoughts / decided.length : 0
    };
  }

  /**
   * Shutdown
   */
  shutdown(): void {
    clearInterval(this.cleanupInterval);
    this.streams.clear();
    this.removeAllListeners();

    if (this.config.enableLogging) {
      console.log('🧠 ThoughtStreamCoordinator: Shutdown complete');
    }
  }
}

/** Singleton instance */
let coordinatorInstance: ThoughtStreamCoordinator | null = null;

/**
 * Get global coordinator instance
 */
export function getThoughtStreamCoordinator(): ThoughtStreamCoordinator {
  if (!coordinatorInstance) {
    coordinatorInstance = new ThoughtStreamCoordinator();
  }
  return coordinatorInstance;
}

/**
 * Reset coordinator (for testing)
 */
export function resetThoughtStreamCoordinator(): void {
  if (coordinatorInstance) {
    coordinatorInstance.shutdown();
    coordinatorInstance = null;
  }
}
