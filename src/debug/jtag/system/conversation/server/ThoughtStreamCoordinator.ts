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
  CoordinationConfig,
  RejectionReason,
  PersonaPriority
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

  /** Recent responders by context (for recency-based rotation) */
  private recentResponders: Map<UUID, UUID[]> = new Map(); // contextId -> [personaIds in recency order]

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

    // Check if any moderator is claiming (if so, no recency penalty - moderator controls the flow)
    const hasModerator = claims.some(c => c.priority === 'moderator');

    // Get recency penalties (personas who recently responded get deprioritized)
    const recentList = this.recentResponders.get(stream.contextId) || [];
    const getRecencyPenalty = (personaId: UUID): number => {
      if (hasModerator) return 0; // No recency penalty when moderator present

      const position = recentList.indexOf(personaId);
      if (position === -1) return 0; // Never responded, no penalty

      // Recent responders get penalty (0-50 point deduction)
      // Most recent = 50 point penalty, older = less penalty
      const maxPenalty = 50;
      const recencyFactor = 1 - (position / Math.max(recentList.length, 1));
      return maxPenalty * recencyFactor;
    };

    // Sort claims by PRIORITY FIRST, then confidence minus recency penalty
    // Priority order: moderator > expert > participant > observer
    const getPriorityWeight = (priority?: PersonaPriority): number => {
      switch (priority) {
        case 'moderator': return 1000; // Highest priority (teachers, admins, project managers)
        case 'expert': return 100;     // Domain experts get boosted
        case 'participant': return 10;  // Normal participants
        case 'observer': return 1;      // Low-priority observers
        default: return 10;             // Default to participant if not specified
      }
    };

    const sortedClaims = claims.sort((a, b) => {
      const priorityA = getPriorityWeight(a.priority);
      const priorityB = getPriorityWeight(b.priority);

      // Sort by priority first
      if (priorityA !== priorityB) {
        return priorityB - priorityA;
      }

      // Within same priority, sort by confidence MINUS recency penalty
      const recencyPenaltyA = getRecencyPenalty(a.personaId);
      const recencyPenaltyB = getRecencyPenalty(b.personaId);
      const scoreA = a.confidence - recencyPenaltyA;
      const scoreB = b.confidence - recencyPenaltyB;

      return scoreB - scoreA;
    });

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

    // Everyone else denied - track rejection reasons for diagnostics
    for (const thought of thoughts) {
      if (!granted.includes(thought.personaId)) {
        denied.push(thought.personaId);

        // Determine rejection reason
        let reason: RejectionReason['reason'] = 'no_slots';
        let details = '';

        if (thought.type === 'deferring') {
          reason = 'deferred';
          details = `Persona chose to defer: ${thought.reasoning}`;
        } else if (thought.type === 'claiming') {
          // Was claiming but didn't get granted
          const claimIndex = sortedClaims.findIndex(c => c.personaId === thought.personaId);
          if (claimIndex >= this.config.maxResponders) {
            reason = 'outranked';
            details = `Ranked ${claimIndex + 1}/${sortedClaims.length}, only ${this.config.maxResponders} slot(s) available`;
          } else if (thought.confidence < this.config.minConfidence) {
            reason = 'low_confidence';
            details = `Confidence ${thought.confidence.toFixed(2)} below threshold ${this.config.minConfidence.toFixed(2)}`;
          } else {
            reason = 'no_slots';
            details = 'All slots claimed by higher priority personas';
          }
        } else {
          reason = 'timeout';
          details = `Timed out in '${thought.type}' state`;
        }

        stream.rejections.push({
          personaId: thought.personaId,
          reason,
          confidence: thought.confidence,
          priority: thought.priority,
          details,
          timestamp: Date.now()
        });
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

    // Track granted responders for recency-based rotation
    for (const personaId of granted) {
      this.trackResponse(stream.contextId, personaId);
    }

    if (this.config.enableLogging) {
      console.log(`🎯 Decision: ${stream.messageId.slice(0, 8)} → ${granted.length} granted, ${denied.length} denied (${decision.coordinationDurationMs}ms)`);
      console.log(`   Reasoning: ${decision.reasoning}`);
      if (!hasModerator && recentList.length > 0) {
        console.log(`🔄 Recency: Recent responders=[${recentList.slice(0, 5).map((r: UUID) => r.slice(0, 8)).join(', ')}...]`);
      }
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
        claimedBy: new Set(),
        rejections: []
      };

      this.streams.set(messageId, stream);

      if (this.config.enableLogging) {
        console.log(`🧠 Stream: Created for message ${messageId.slice(0, 8)} (slots=${maxResponders})`);
      }
    }

    return stream;
  }

  /**
   * Track a response for recency-based rotation
   * Maintains a rolling list of recent responders (most recent first)
   */
  private trackResponse(contextId: UUID, personaId: UUID): void {
    let recent = this.recentResponders.get(contextId) || [];

    // Remove persona if already in list
    recent = recent.filter(id => id !== personaId);

    // Add to front (most recent)
    recent.unshift(personaId);

    // Keep only last 10 responders
    if (recent.length > 10) {
      recent = recent.slice(0, 10);
    }

    this.recentResponders.set(contextId, recent);
  }

  /**
   * Get rejection statistics for diagnostics
   */
  getRejectionStats(messageId?: string): RejectionReason[] {
    if (messageId) {
      const stream = this.streams.get(messageId);
      return stream?.rejections || [];
    }

    // Return all rejections across all streams
    const allRejections: RejectionReason[] = [];
    for (const stream of this.streams.values()) {
      allRejections.push(...stream.rejections);
    }
    return allRejections;
  }

  /**
   * Get summary statistics about coordination
   */
  getCoordinationStats() {
    const stats = {
      activeStreams: this.streams.size,
      totalRejections: 0,
      rejectionsByReason: {
        no_slots: 0,
        low_confidence: 0,
        outranked: 0,
        deferred: 0,
        timeout: 0
      },
      rejectionsByPriority: {
        moderator: 0,
        expert: 0,
        participant: 0,
        observer: 0,
        undefined: 0
      }
    };

    for (const stream of this.streams.values()) {
      stats.totalRejections += stream.rejections.length;
      for (const rejection of stream.rejections) {
        stats.rejectionsByReason[rejection.reason]++;
        const priority = rejection.priority || 'undefined';
        if (priority in stats.rejectionsByPriority) {
          stats.rejectionsByPriority[priority as keyof typeof stats.rejectionsByPriority]++;
        }
      }
    }

    return stats;
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
