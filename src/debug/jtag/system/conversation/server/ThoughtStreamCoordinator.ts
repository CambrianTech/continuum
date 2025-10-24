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
import { BaseModerator, getDefaultModerator, type ConversationHealth, type ModerationContext } from '../shared/BaseModerator';
import { HeartbeatManager } from '../shared/SystemHeartbeat';

/**
 * Thought Stream Coordinator - RTOS-inspired AI social coordination
 */
export class ThoughtStreamCoordinator extends EventEmitter {
  private config: CoordinationConfig;

  /** Active thought streams by messageId */
  private streams: Map<string, ThoughtStream> = new Map();

  /** Sequential evaluation queue - ensures personas evaluate one at a time */
  private evaluationQueue: Map<string, Promise<void>> = new Map();

  /** Recent responders by context (for recency-based rotation) - DEPRECATED: Now handled by moderator */
  private recentResponders: Map<UUID, UUID[]> = new Map(); // contextId -> [personaIds in recency order]

  /** Cleanup timer */
  private cleanupInterval: NodeJS.Timeout;

  /** Moderator - makes ALL coordination decisions (adapter pattern) */
  private moderator: BaseModerator;

  /** Conversation health tracking by contextId */
  private conversationHealth: Map<UUID, ConversationHealth> = new Map();

  /** Adaptive cadence manager - learns system's natural rhythm */
  private heartbeatManager: HeartbeatManager = new HeartbeatManager();

  /** Message history tracking for health metrics (contextId -> timestamps) */
  private recentMessages: Map<UUID, number[]> = new Map();

  constructor(config: Partial<CoordinationConfig> = {}, moderator?: BaseModerator) {
    super();
    this.config = { ...DEFAULT_COORDINATION_CONFIG, ...config };
    this.moderator = moderator || getDefaultModerator();

    // Cleanup old streams every 30 seconds
    this.cleanupInterval = setInterval(() => {
      this.cleanup();
    }, 30000);

    // ALWAYS enable ThoughtStream debugging
    console.log('🧠 ThoughtStreamCoordinator: Initialized');
    console.log(`🎚️ Using moderator: ${this.moderator.constructor.name}`);
  }

  /**
   * Request evaluation turn - NOW PARALLEL (no sequential bottleneck)
   *
   * REMOVED: Sequential queue that was causing memory leaks
   * - Was forcing 12+ AIs to wait in line (10-90 seconds each)
   * - Caused cascading failures when one AI got stuck
   * - Memory accumulated with pending Promises
   *
   * NEW BEHAVIOR: All AIs evaluate in parallel
   * - ThoughtStream + Moderator handle coordination
   * - No artificial bottleneck
   * - Natural concurrency
   */
  async requestEvaluationTurn(messageId: string, personaId: UUID): Promise<() => void> {
    // NO-OP: Just return immediately (no queue)
    // Add tiny random delay (0-50ms) to prevent thundering herd
    const randomDelay = Math.random() * 50;
    await new Promise(resolve => setTimeout(resolve, randomDelay));

    if (this.config.enableLogging) {
      console.log(`⚡ ${personaId.slice(0, 8)}: Parallel evaluation (${randomDelay.toFixed(0)}ms delay)`);
    }

    // Return no-op resolver (no queue to release)
    return () => {};
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

      // ALWAYS log thoughts for debugging
      const elapsedMs = Date.now() - stream.startTime;
      console.log(`🧠 Thought #${stream.thoughts.length}: ${thought.personaId.slice(0, 8)} → ${thought.type} (conf=${thought.confidence.toFixed(2)}) [+${elapsedMs}ms]`);
      console.log(`   Reasoning: ${thought.reasoning.slice(0, 100)}${thought.reasoning.length > 100 ? '...' : ''}`);

      // Record evaluation time for adaptive cadence
      const heartbeat = this.heartbeatManager.getHeartbeat(stream.contextId);
      heartbeat.recordEvaluation(thought.personaId, elapsedMs);

      // SEMAPHORE: Handle claiming/deferring
      if (thought.type === 'claiming') {
        await this.handleClaim(stream, thought);
      } else if (thought.type === 'deferring') {
        await this.handleDefer(stream, thought);
      }

      // Emit event for observers (SIGNAL to condition variables)
      this.emit(`thought:${messageId}`, thought);
      this.emit('thought', messageId, thought);

      // Schedule decision after adaptive cadence window (if not already scheduled)
      if (!stream.decisionTimer) {
        // Get adaptive cadence from heartbeat (smoothly tracks p95)
        const heartbeat = this.heartbeatManager.getHeartbeat(stream.contextId);
        const adaptiveWindow = heartbeat.getAdaptiveCadence();

        console.log(`🫀 Adaptive cadence: ${adaptiveWindow}ms (was ${this.config.intentionWindowMs}ms fixed)`);

        stream.decisionTimer = setTimeout(async () => {
          if (stream.phase === 'gathering') {
            console.log(`⏰ Adaptive window expired for ${messageId.slice(0, 8)}, making decision...`);
            await this.makeDecision(stream);
          }
        }, adaptiveWindow);
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
   * Get or update conversation health metrics for a context
   */
  private getConversationHealth(contextId: UUID): ConversationHealth {
    let health = this.conversationHealth.get(contextId);

    if (!health) {
      // Initialize health for new context
      health = {
        consecutiveSilence: 0,
        recentMessageCount: 0,
        avgResponseTime: 0,
        activeParticipants: 0,
        timeSinceLastResponse: 0
      };
      this.conversationHealth.set(contextId, health);
    }

    return health;
  }

  /**
   * Update conversation health after AI response
   */
  private updateConversationHealth(contextId: UUID, responded: boolean): void {
    const health = this.getConversationHealth(contextId);
    const now = Date.now();

    // Track recent messages (last 5 minutes) - FIX: Limit array size
    let messages = this.recentMessages.get(contextId) || [];
    messages.push(now);
    messages = messages.filter(t => now - t < 300000); // Keep last 5 minutes

    // SAFETY: Hard limit to prevent memory leak (max 100 messages tracked)
    if (messages.length > 100) {
      messages = messages.slice(-100);
    }

    this.recentMessages.set(contextId, messages);

    health.recentMessageCount = messages.length;

    // Update consecutive silence counter
    if (responded) {
      health.consecutiveSilence = 0;
      health.timeSinceLastResponse = 0;
    } else {
      health.consecutiveSilence++;
      health.timeSinceLastResponse = now - (messages[messages.length - 2] || now);
    }

    this.conversationHealth.set(contextId, health);
  }

  /**
   * Check if we can make a decision (DELEGATES to moderator)
   */
  private canDecide(stream: ThoughtStream): boolean {
    if (stream.phase === 'decided') {
      return false; // Already decided
    }

    const health = this.getConversationHealth(stream.contextId);
    const context: ModerationContext = {
      stream,
      health,
      config: this.config,
      now: Date.now()
    };

    return this.moderator.shouldDecideNow(context);
  }

  /**
   * Make final decision and signal waiters (DELEGATES to moderator)
   */
  private async makeDecision(stream: ThoughtStream): Promise<void> {
    if (stream.phase === 'decided') {
      return; // Already decided
    }

    stream.phase = 'deliberating';

    // Build moderation context
    const health = this.getConversationHealth(stream.contextId);
    const context: ModerationContext = {
      stream,
      health,
      config: this.config,
      now: Date.now()
    };

    // Delegate decision to moderator (adapter pattern)
    const moderatorDecision = this.moderator.makeDecision(context);

    // Convert ModeratorDecision to CoordinationDecision
    const thoughts = Array.from(stream.considerations.values());
    const granted = moderatorDecision.granted;
    const denied: UUID[] = [];
    const reasoning: string[] = [];

    // Build reasoning strings for granted personas
    for (const personaId of granted) {
      const thought = stream.considerations.get(personaId);
      if (thought) {
        reasoning.push(`${personaId.slice(0, 8)} granted (conf=${thought.confidence.toFixed(2)})`);
      }
    }

    // Build rejection reasons for denied personas
    for (const thought of thoughts) {
      if (!granted.includes(thought.personaId)) {
        denied.push(thought.personaId);

        // Get rejection reason from moderator
        const rejectionReason = moderatorDecision.rejected.get(thought.personaId);
        let reason: RejectionReason['reason'] = 'no_slots';
        let details = rejectionReason || 'Not granted by moderator';

        // Classify rejection reason
        if (thought.type === 'deferring') {
          reason = 'deferred';
          details = `Persona chose to defer: ${thought.reasoning}`;
        } else if (rejectionReason?.includes('below threshold')) {
          reason = 'low_confidence';
        } else if (rejectionReason?.includes('Ranked')) {
          reason = 'outranked';
        } else if (rejectionReason?.includes('timeout')) {
          reason = 'timeout';
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
        urgency: 50,
        responseType: 'answer',
        relevanceScore: t.confidence,
        wasMentioned: false,
        timestamp: t.timestamp
      })),
      granted,
      denied,
      reasoning: reasoning.join('; ') || 'No personas responded',
      timestamp: new Date(),
      coordinationDurationMs: Date.now() - stream.startTime
    };

    stream.decision = decision;
    stream.phase = 'decided';

    // Log decision timing
    const totalMs = Date.now() - stream.startTime;
    console.log(`⏱️  Decision made after ${totalMs}ms with ${stream.thoughts.length} thoughts (${granted.length} granted, ${denied.length} denied)`);

    // Update conversation health (did anyone respond?)
    this.updateConversationHealth(stream.contextId, granted.length > 0);

    // Update moderator's recency tracking (if using PolynomialDecayModerator)
    if ('updateRecency' in this.moderator && typeof (this.moderator as any).updateRecency === 'function') {
      (this.moderator as any).updateRecency(stream.contextId, granted);
    }

    // ALWAYS log final decision for debugging
    console.log(`🎯 Decision: ${stream.messageId.slice(0, 8)} → ${granted.length} granted, ${denied.length} denied (${decision.coordinationDurationMs}ms)`);
    console.log(`   Moderator: threshold=${(moderatorDecision.confidenceThreshold * 100).toFixed(0)}%, maxResponders=${moderatorDecision.maxResponders}`);
    console.log(`   Health: silence=${health.consecutiveSilence}, recent=${health.recentMessageCount}`);
    if (granted.length > 0) {
      console.log(`   ✅ Granted: ${granted.map(id => id.slice(0, 8)).join(', ')}`);
    }
    if (denied.length > 0) {
      console.log(`   ❌ Denied: ${denied.map(id => id.slice(0, 8)).join(', ')}`);
    }
    if (reasoning.length > 0) {
      console.log(`   Reasoning: ${reasoning.join('; ')}`);
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
   * Get all active streams (for diagnostics/inspection)
   */
  getStreams(): Map<string, ThoughtStream> {
    return this.streams;
  }

  /**
   * Get specific stream by message ID
   */
  getStream(messageId: string): ThoughtStream | undefined {
    return this.streams.get(messageId);
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
   * Cleanup old streams and health tracking (MECHANICAL SAFETY)
   */
  private cleanup(): void {
    const now = Date.now();
    const streamMaxAge = 60000; // 1 minute
    const healthMaxAge = 600000; // 10 minutes

    // Clean up old streams
    for (const [messageId, stream] of Array.from(this.streams.entries())) {
      if (now - stream.startTime > streamMaxAge) {
        this.streams.delete(messageId);
      }
    }

    // SAFETY: Clean up conversation health tracking (prevent memory leak)
    for (const [contextId, health] of Array.from(this.conversationHealth.entries())) {
      if (health.timeSinceLastResponse > healthMaxAge) {
        this.conversationHealth.delete(contextId);
        this.recentMessages.delete(contextId);
      }
    }

    // SAFETY: Clean up recent messages tracking (prevent memory leak)
    for (const [contextId, messages] of Array.from(this.recentMessages.entries())) {
      // Remove messages older than 5 minutes
      const filtered = messages.filter(t => now - t < 300000);
      if (filtered.length === 0) {
        this.recentMessages.delete(contextId);
      } else {
        this.recentMessages.set(contextId, filtered);
      }
    }

    // SAFETY: Clean up evaluation queue (detect dead AIs)
    for (const [messageId, promise] of Array.from(this.evaluationQueue.entries())) {
      const stream = this.streams.get(messageId);
      // If stream is gone but evaluation still queued, AI is dead/stuck
      if (!stream || now - stream.startTime > streamMaxAge) {
        this.evaluationQueue.delete(messageId);
        if (this.config.enableLogging) {
          console.log(`🚨 Cleanup: Removed dead evaluation queue for ${messageId.slice(0, 8)}`);
        }
      }
    }

    if (this.config.enableLogging && (this.streams.size > 0 || this.conversationHealth.size > 0)) {
      console.log(`🧠 Cleanup: ${this.streams.size} active streams, ${this.conversationHealth.size} tracked contexts, ${this.evaluationQueue.size} queued evaluations`);
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
   * Shutdown (MECHANICAL SAFETY: Clean up ALL state)
   */
  shutdown(): void {
    clearInterval(this.cleanupInterval);
    this.streams.clear();
    this.evaluationQueue.clear();
    this.recentResponders.clear();
    this.conversationHealth.clear();
    this.recentMessages.clear();
    this.removeAllListeners();

    if (this.config.enableLogging) {
      console.log('🧠 ThoughtStreamCoordinator: Shutdown complete - all state cleared');
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
