/**
 * Base Coordination Stream - Abstract coordination for ANY domain
 *
 * Extracted from ThoughtStreamCoordinator (Phase 1, Commit 1.3)
 *
 * Philosophy: "Use abstraction and subclasses" - elegant extensibility
 *
 * Domain-agnostic coordination for AI turn-taking across:
 * - Chat (messages in rooms)
 * - Games (moves in matches)
 * - Coding (edits in sessions)
 * - Video (actions in scenes)
 *
 * Base class provides coordination primitives (RTOS-inspired):
 * - Thought broadcasting (SIGNAL)
 * - Turn claiming (MUTEX)
 * - Decision making (CONDITION VARIABLE)
 * - Cleanup (MECHANICAL SAFETY)
 *
 * Subclasses define domain-specific behavior via protected hooks
 */

import { EventEmitter } from 'events';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { Logger, FileMode, type ComponentLogger } from '../../core/logging/Logger';

/**
 * Domain-agnostic thought (claim to respond)
 */
export interface BaseThought {
  personaId: UUID;
  personaName: string;
  type: 'claiming' | 'deferring';
  confidence: number;    // 0.0-1.0
  reasoning: string;
  priority?: string;     // Domain-specific priority label
  timestamp: number;
}

/**
 * Domain-agnostic coordination decision
 */
export interface BaseDecision {
  eventId: string;       // Domain-agnostic event identifier
  contextId: UUID;       // Domain-agnostic context (room, game, session, etc)
  granted: UUID[];       // Personas allowed to act
  denied: UUID[];        // Personas rejected
  reasoning: string;
  timestamp: Date;
  coordinationDurationMs: number;
}

/**
 * Domain-agnostic stream state
 */
export interface BaseStream<T extends BaseThought = BaseThought> {
  eventId: string;       // Generic event (message, move, edit, action)
  contextId: UUID;       // Generic context (room, game, session, scene)
  phase: 'gathering' | 'deliberating' | 'decided';
  thoughts: T[];         // Immutable log of thoughts
  considerations: Map<UUID, T>;  // Mutable state for fast lookup
  startTime: number;
  availableSlots: number;
  claimedBy: Set<UUID>;
  decision?: BaseDecision;
  decisionTimer?: NodeJS.Timeout;
  decisionSignal?: Promise<BaseDecision>;
  signalResolver?: (decision: BaseDecision) => void;
}

/**
 * Configuration for coordination behavior
 */
export interface CoordinationConfig {
  intentionWindowMs: number;      // How long to wait for thoughts
  maxResponders: number;           // Max simultaneous actors
  enableLogging: boolean;
  cleanupIntervalMs: number;
}

export const DEFAULT_COORDINATION_CONFIG: CoordinationConfig = {
  intentionWindowMs: 2000,   // 2 seconds
  maxResponders: 1,
  enableLogging: true,
  cleanupIntervalMs: 30000   // 30 seconds
};

/**
 * Abstract Base Coordination Stream
 *
 * Provides coordination primitives, subclasses define domain-specific behavior
 */
export abstract class BaseCoordinationStream<
  TThought extends BaseThought = BaseThought,
  TDecision extends BaseDecision = BaseDecision,
  TStream extends BaseStream<TThought> = BaseStream<TThought>
> extends EventEmitter {

  protected readonly config: CoordinationConfig;

  /** Logger instance */
  private logger: ComponentLogger;

  /** Active streams by event ID */
  protected streams: Map<string, TStream> = new Map();

  /** Cleanup timer */
  private cleanupInterval: NodeJS.Timeout;

  constructor(config: Partial<CoordinationConfig> = {}) {
    super();
    this.config = { ...DEFAULT_COORDINATION_CONFIG, ...config };

    // Initialize logger for coordination (logs to coordination.log)
    this.logger = Logger.createWithFile('CoordinationStream', 'coordination', FileMode.CLEAN);

    // Cleanup old streams periodically
    this.cleanupInterval = setInterval(() => {
      this.cleanup();
    }, this.config.cleanupIntervalMs);

    this.log(`✅ ${this.getDomainName()} CoordinationStream initialized`);
  }

  // ============================================================================
  // ABSTRACT METHODS - Subclasses MUST implement
  // ============================================================================

  /**
   * Get domain name for logging (e.g., "Chat", "Game", "Code")
   */
  protected abstract getDomainName(): string;

  /**
   * Create new stream for domain-specific event
   */
  protected abstract createStream(eventId: string, contextId: UUID): TStream;

  /**
   * Convert domain-agnostic decision to domain-specific decision
   */
  protected abstract convertDecision(baseDecision: BaseDecision, stream: TStream): TDecision;

  /**
   * Get event-specific log context (for debugging)
   */
  protected abstract getEventLogContext(eventId: string): string;

  // ============================================================================
  // PROTECTED HOOKS - Subclasses MAY override for customization
  // ============================================================================

  /**
   * Hook: Called when thought is broadcast (before processing)
   * Subclasses can add domain-specific logic
   */
  protected onThoughtBroadcast(stream: TStream, thought: TThought): void {
    // Default: no-op
  }

  /**
   * Hook: Called when claim is processed
   * Subclasses can add domain-specific validation
   */
  protected onClaim(stream: TStream, thought: TThought): boolean {
    // Default: always allow
    return true;
  }

  /**
   * Hook: Called when decision is made (before signaling)
   * Subclasses can add domain-specific post-processing
   */
  protected onDecisionMade(stream: TStream, decision: TDecision): void {
    // Default: no-op
  }

  /**
   * Hook: Determine if we can decide early (optimization)
   * Subclasses can add domain-specific criteria
   */
  protected canDecideEarly(stream: TStream): boolean {
    // Default: decide early if all expected personas have responded
    return stream.phase === 'gathering' && stream.thoughts.length >= 3;
  }

  /**
   * Hook: Get probabilistic max responders
   * Subclasses can customize slot allocation
   */
  protected getMaxResponders(): number {
    // Default: probabilistic (70% = 1, 25% = 2, 5% = 3)
    const rand = Math.random();
    if (rand < 0.70) return 1;
    if (rand < 0.95) return 2;
    return 3;
  }

  /**
   * Hook: Stream cleanup age threshold
   * Subclasses can customize cleanup timing
   */
  protected getStreamMaxAge(stream: TStream): number {
    // Fast cleanup for decided streams (5s)
    if (stream.phase === 'decided') return 5000;
    // Slower cleanup for gathering streams (60s)
    return 60000;
  }

  // ============================================================================
  // PUBLIC API - Coordination primitives
  // ============================================================================

  /**
   * Broadcast a thought to the stream (SIGNAL primitive)
   */
  async broadcastThought(eventId: string, contextId: UUID, thought: TThought): Promise<void> {
    try {
      const stream = this.getOrCreateStream(eventId, contextId);

      // Add to immutable log
      stream.thoughts.push(thought);

      // Update mutable state
      stream.considerations.set(thought.personaId, thought);

      // Hook: Allow subclass to react
      this.onThoughtBroadcast(stream, thought);

      // Log thought
      const elapsedMs = Date.now() - stream.startTime;
      this.log(`🧠 Thought #${stream.thoughts.length}: ${thought.personaId.slice(0, 8)} → ${thought.type} (conf=${thought.confidence.toFixed(2)}) [+${elapsedMs}ms]`);
      this.log(`   Reasoning: ${thought.reasoning.slice(0, 100)}${thought.reasoning.length > 100 ? '...' : ''}`);

      // SEMAPHORE: Handle claiming/deferring
      if (thought.type === 'claiming') {
        await this.handleClaim(stream, thought);
      } else if (thought.type === 'deferring') {
        await this.handleDefer(stream, thought);
      }

      // Emit event for observers (SIGNAL)
      this.emit(`thought:${eventId}`, thought);
      this.emit('thought', eventId, thought);

      // Schedule decision timer (if not already scheduled)
      if (!stream.decisionTimer) {
        stream.decisionTimer = setTimeout(async () => {
          if (stream.phase === 'gathering') {
            this.log(`⏰ Window expired for ${eventId.slice(0, 8)}, making decision...`);
            await this.makeDecision(stream);
          }
        }, this.config.intentionWindowMs);
      }

      // Early decision if criteria met (optimization)
      if (this.canDecideEarly(stream)) {
        clearTimeout(stream.decisionTimer);
        stream.decisionTimer = undefined;
        await this.makeDecision(stream);
      }

    } catch (error) {
      this.log(`❌ Error broadcasting thought: ${error}`, true);
    }
  }

  /**
   * Wait for decision (CONDITION VARIABLE primitive)
   */
  async waitForDecision(eventId: string, timeoutMs: number = 5000): Promise<TDecision | null> {
    const stream = this.streams.get(eventId);
    if (!stream) {
      this.log(`⚠️  No stream for ${eventId.slice(0, 8)} - graceful degradation`);
      return null;
    }

    if (stream.phase === 'decided' && stream.decision) {
      return this.convertDecision(stream.decision, stream);
    }

    // Setup condition variable (Promise-based)
    if (!stream.decisionSignal) {
      stream.decisionSignal = new Promise<BaseDecision>((resolve) => {
        stream.signalResolver = resolve;
      });
    }

    // Wait with timeout (fallback safety)
    const timeoutPromise = new Promise<null>((resolve) => {
      setTimeout(() => resolve(null), timeoutMs);
    });

    const result = await Promise.race([stream.decisionSignal, timeoutPromise]);

    if (!result) {
      this.log(`⏰ Timeout waiting for decision on ${eventId.slice(0, 8)}`);
      return null;
    }

    return this.convertDecision(result, stream);
  }

  /**
   * Check if persona was granted permission
   */
  async checkPermission(personaId: UUID, eventId: string): Promise<boolean> {
    const stream = this.streams.get(eventId);
    if (!stream) return true; // Graceful degradation

    if (stream.phase !== 'decided' || !stream.decision) {
      return false; // Not decided yet
    }

    return stream.decision.granted.includes(personaId);
  }

  /**
   * Get specific stream by event ID
   */
  getStream(eventId: string): TStream | undefined {
    return this.streams.get(eventId);
  }

  /**
   * Get all active streams (diagnostics)
   */
  getStreams(): Map<string, TStream> {
    return this.streams;
  }

  /**
   * Shutdown (MECHANICAL SAFETY)
   */
  shutdown(): void {
    clearInterval(this.cleanupInterval);
    this.streams.clear();
    this.removeAllListeners();
    this.log('🛑 Shutdown complete');
  }

  // ============================================================================
  // PRIVATE IMPLEMENTATION - Coordination logic
  // ============================================================================

  /**
   * Handle claim attempt (MUTEX acquisition)
   */
  private async handleClaim(stream: TStream, thought: TThought): Promise<void> {
    // Hook: Allow subclass to validate
    if (!this.onClaim(stream, thought)) {
      this.log(`❌ Claim rejected by domain: ${thought.personaId.slice(0, 8)}`);
      return;
    }

    // SEMAPHORE: Check if slots available
    if (stream.availableSlots > 0) {
      stream.availableSlots--;
      stream.claimedBy.add(thought.personaId);
      this.log(`🔒 Claim: ${thought.personaId.slice(0, 8)} acquired slot (${stream.availableSlots} remaining)`);
    } else {
      this.log(`❌ Claim: ${thought.personaId.slice(0, 8)} no slots available`);
    }
  }

  /**
   * Handle defer (MUTEX release)
   */
  private async handleDefer(stream: TStream, thought: TThought): Promise<void> {
    if (stream.claimedBy.has(thought.personaId)) {
      stream.claimedBy.delete(thought.personaId);
      stream.availableSlots++;
      this.log(`🔓 Defer: ${thought.personaId.slice(0, 8)} released slot (${stream.availableSlots} available)`);
    }
  }

  /**
   * Make final decision and signal waiters
   */
  private async makeDecision(stream: TStream): Promise<void> {
    if (stream.phase === 'decided') {
      return; // Already decided
    }

    stream.phase = 'deliberating';

    // Simple decision logic: grant to highest confidence claimers
    const claimers = Array.from(stream.considerations.values())
      .filter(t => t.type === 'claiming')
      .sort((a, b) => b.confidence - a.confidence);

    const granted = claimers
      .slice(0, this.config.maxResponders)
      .map(t => t.personaId);

    const denied = Array.from(stream.considerations.keys())
      .filter(id => !granted.includes(id));

    const decision: BaseDecision = {
      eventId: stream.eventId,
      contextId: stream.contextId,
      granted,
      denied,
      reasoning: granted.length > 0
        ? `Granted to ${granted.length} highest confidence claimers`
        : 'No claimers',
      timestamp: new Date(),
      coordinationDurationMs: Date.now() - stream.startTime
    };

    stream.decision = decision;
    stream.phase = 'decided';

    // Convert to domain-specific decision
    const domainDecision = this.convertDecision(decision, stream);

    // Hook: Allow subclass to post-process
    this.onDecisionMade(stream, domainDecision);

    // Log decision
    const totalMs = Date.now() - stream.startTime;
    this.log(`🎯 Decision: ${stream.eventId.slice(0, 8)} → ${granted.length} granted, ${denied.length} denied (${totalMs}ms)`);
    if (granted.length > 0) {
      this.log(`   ✅ Granted: ${granted.map(id => id.slice(0, 8)).join(', ')}`);
    }

    // CONDITION VARIABLE: Signal all waiters
    if (stream.signalResolver) {
      stream.signalResolver(decision);
    }

    // Emit event for observers
    this.emit(`decision:${stream.eventId}`, domainDecision);
    this.emit('decision', stream.eventId, domainDecision);
  }

  /**
   * Get or create stream
   */
  private getOrCreateStream(eventId: string, contextId: UUID): TStream {
    let stream = this.streams.get(eventId);

    if (!stream) {
      stream = this.createStream(eventId, contextId);
      this.streams.set(eventId, stream);
      this.log(`🧠 Stream: Created for ${this.getEventLogContext(eventId)} (slots=${stream.availableSlots})`);
    }

    return stream;
  }

  /**
   * Cleanup old streams (MECHANICAL SAFETY)
   */
  private cleanup(): void {
    const now = Date.now();

    for (const [eventId, stream] of Array.from(this.streams.entries())) {
      const maxAge = this.getStreamMaxAge(stream);

      if (now - stream.startTime > maxAge) {
        this.streams.delete(eventId);
        this.log(`🧹 Cleanup: Removed stream ${eventId.slice(0, 8)} (${now - stream.startTime}ms old, phase=${stream.phase})`);
      }
    }

    if (this.streams.size > 0) {
      this.log(`🧠 Cleanup: ${this.streams.size} active streams`);
    }
  }

  /**
   * Logging helper
   */
  protected log(message: string, isError: boolean = false): void {
    if (!this.config.enableLogging) return;

    if (isError) {
      this.logger.error(message);
    } else {
      this.logger.info(message);
    }
  }
}
