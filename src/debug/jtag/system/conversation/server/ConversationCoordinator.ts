/**
 * Conversation Coordinator - Server
 *
 * Coordinates multi-persona responses to prevent spam/loops
 *
 * CRITICAL: All operations are non-blocking with timeouts
 * If coordination fails, personas fall back to independent logic
 */

import type { UUID } from '../../core/types/JTAGTypes';
import type {
  ResponseIntention,
  CoordinationDecision,
  CoordinationConfig,
  MessageCoordinationState
} from '../shared/ConversationCoordinationTypes';
import { DEFAULT_COORDINATION_CONFIG } from '../shared/ConversationCoordinationTypes';

export class ConversationCoordinator {
  private config: CoordinationConfig;

  /** Active coordination states by messageId */
  private coordinationStates: Map<string, MessageCoordinationState> = new Map();

  /** Cleanup timer for old states */
  private cleanupInterval: NodeJS.Timeout;

  constructor(config: Partial<CoordinationConfig> = {}) {
    this.config = { ...DEFAULT_COORDINATION_CONFIG, ...config };

    // Cleanup old coordination states every 30 seconds
    this.cleanupInterval = setInterval(() => {
      this.cleanup();
    }, 30000);

    if (this.config.enableLogging) {
      console.log('🎭 ConversationCoordinator: Initialized', this.config);
    }
  }

  /**
   * Submit a response intention from a persona
   * NON-BLOCKING: Never throws, returns immediately
   */
  async submitIntention(intention: ResponseIntention): Promise<void> {
    try {
      const { messageId, contextId, personaId } = intention;

      // Get or create coordination state
      let state = this.coordinationStates.get(messageId);

      if (!state) {
        state = {
          messageId,
          contextId,
          intentions: new Map(),
          startTime: Date.now(),
          resolved: false
        };

        // Set timeout to resolve coordination
        state.timeoutHandle = setTimeout(() => {
          this.resolveCoordination(messageId);
        }, this.config.intentionWindowMs);

        this.coordinationStates.set(messageId, state);

        if (this.config.enableLogging) {
          console.log(`🎭 Coordination: Started for message ${messageId.slice(0, 8)}`);
        }
      }

      // Store intention
      state.intentions.set(personaId, intention);

      if (this.config.enableLogging) {
        console.log(`🎭 Intention: ${intention.personaId.slice(0, 8)} → confidence=${intention.confidence}, urgency=${intention.urgency}, type=${intention.responseType}`);
      }

    } catch (error) {
      console.error('❌ ConversationCoordinator.submitIntention: Error (non-fatal):', error);
    }
  }

  /**
   * Check if persona is granted permission to respond
   * NON-BLOCKING: Returns false if coordination not resolved yet
   */
  async checkPermission(personaId: UUID, messageId: string): Promise<boolean> {
    try {
      const state = this.coordinationStates.get(messageId);

      if (!state) {
        // No coordination state = fall back to independent logic
        if (this.config.enableLogging) {
          console.log(`🎭 Permission: ${personaId.slice(0, 8)} → FALLBACK (no state)`);
        }
        return true; // Graceful degradation
      }

      if (!state.resolved) {
        // Coordination not resolved yet = wait is not over
        if (this.config.enableLogging) {
          console.log(`🎭 Permission: ${personaId.slice(0, 8)} → WAIT (not resolved)`);
        }
        return false;
      }

      // Check decision
      const granted = state.decision?.granted.includes(personaId) ?? false;

      if (this.config.enableLogging) {
        console.log(`🎭 Permission: ${personaId.slice(0, 8)} → ${granted ? 'GRANTED ✅' : 'DENIED ❌'}`);
      }

      return granted;

    } catch (error) {
      console.error('❌ ConversationCoordinator.checkPermission: Error (non-fatal):', error);
      return true; // Graceful degradation
    }
  }

  /**
   * Wait for coordination to resolve (with timeout)
   * NON-BLOCKING: Returns after timeout regardless of state
   */
  async waitForPermission(
    personaId: UUID,
    messageId: string,
    timeoutMs: number = 3000
  ): Promise<boolean> {
    const startTime = Date.now();

    // Poll with exponential backoff
    let delay = 50;

    while (Date.now() - startTime < timeoutMs) {
      const permission = await this.checkPermission(personaId, messageId);

      const state = this.coordinationStates.get(messageId);
      if (state?.resolved) {
        return permission; // Coordination resolved
      }

      // Wait before next check
      await new Promise(resolve => setTimeout(resolve, delay));
      delay = Math.min(delay * 1.5, 500); // Cap at 500ms
    }

    // Timeout - fall back to independent logic
    if (this.config.enableLogging) {
      console.log(`🎭 WaitPermission: ${personaId.slice(0, 8)} → TIMEOUT (fallback)`);
    }

    return true; // Graceful degradation
  }

  /**
   * Resolve coordination and decide who responds
   * INTERNAL: Called by timeout or manual trigger
   */
  private async resolveCoordination(messageId: string): Promise<void> {
    try {
      const state = this.coordinationStates.get(messageId);

      if (!state || state.resolved) {
        return; // Already resolved or missing
      }

      const startTime = Date.now();
      const intentions = Array.from(state.intentions.values());

      if (intentions.length === 0) {
        // No intentions = nothing to coordinate
        state.resolved = true;
        return;
      }

      // Make decision
      const decision = this.makeDecision(messageId, intentions);
      state.decision = decision;
      state.resolved = true;

      // Clear timeout if exists
      if (state.timeoutHandle) {
        clearTimeout(state.timeoutHandle);
        state.timeoutHandle = undefined;
      }

      if (this.config.enableLogging) {
        const duration = Date.now() - startTime;
        console.log(`🎭 Decision: Message ${messageId.slice(0, 8)} → ${decision.granted.length} granted, ${decision.denied.length} denied (${duration}ms)`);
        console.log(`   Reasoning: ${decision.reasoning}`);
      }

    } catch (error) {
      console.error('❌ ConversationCoordinator.resolveCoordination: Error (non-fatal):', error);

      // Mark as resolved anyway to unblock personas
      const state = this.coordinationStates.get(messageId);
      if (state) {
        state.resolved = true;
      }
    }
  }

  /**
   * Core decision logic - who gets to respond?
   */
  private makeDecision(
    messageId: string,
    intentions: ResponseIntention[]
  ): CoordinationDecision {
    const granted: UUID[] = [];
    const denied: UUID[] = [];
    const reasoning: string[] = [];

    // Calculate composite scores: confidence * weight + urgency * (1 - weight)
    const scored = intentions.map(intent => ({
      ...intent,
      compositeScore:
        intent.confidence * this.config.confidenceWeight +
        intent.urgency * (1 - this.config.confidenceWeight)
    })).sort((a, b) => b.compositeScore - a.compositeScore);

    // RULE 1: Mentioned personas always respond (if configured)
    if (this.config.alwaysAllowMentioned) {
      for (const intent of scored) {
        if (intent.wasMentioned && intent.confidence >= this.config.minConfidence) {
          granted.push(intent.personaId);
          reasoning.push(`${intent.personaId.slice(0, 8)} mentioned (conf=${intent.confidence})`);
        }
      }
    }

    // RULE 2: Top scorer responds (if not already granted)
    if (granted.length < this.config.maxResponders && scored.length > 0) {
      const top = scored[0];
      if (top.confidence >= this.config.minConfidence && !granted.includes(top.personaId)) {
        granted.push(top.personaId);
        reasoning.push(`${top.personaId.slice(0, 8)} top scorer (composite=${top.compositeScore.toFixed(0)})`);
      }
    }

    // RULE 3: Second place if significantly different type AND high confidence
    if (granted.length < this.config.maxResponders && scored.length > 1) {
      const second = scored[1];
      const first = scored[0];

      const differentType = second.responseType !== first.responseType;
      const highConfidence = second.confidence >= this.config.minConfidence + 10;

      if (differentType && highConfidence && !granted.includes(second.personaId)) {
        granted.push(second.personaId);
        reasoning.push(`${second.personaId.slice(0, 8)} diverse type=${second.responseType} (conf=${second.confidence})`);
      }
    }

    // Everyone else is denied
    for (const intent of scored) {
      if (!granted.includes(intent.personaId)) {
        denied.push(intent.personaId);
      }
    }

    return {
      messageId,
      intentions,
      granted,
      denied,
      reasoning: reasoning.join('; '),
      timestamp: new Date(),
      coordinationDurationMs: Date.now() - (this.coordinationStates.get(messageId)?.startTime || Date.now())
    };
  }

  /**
   * Cleanup old coordination states
   */
  private cleanup(): void {
    const now = Date.now();
    const maxAge = 60000; // 1 minute

    for (const [messageId, state] of Array.from(this.coordinationStates.entries())) {
      if (now - state.startTime > maxAge) {
        if (state.timeoutHandle) {
          clearTimeout(state.timeoutHandle);
        }
        this.coordinationStates.delete(messageId);
      }
    }

    if (this.config.enableLogging && this.coordinationStates.size > 0) {
      console.log(`🎭 Cleanup: ${this.coordinationStates.size} active coordination states`);
    }
  }

  /**
   * Get statistics for debugging
   */
  getStats(): {
    activeStates: number;
    totalDecisions: number;
    avgIntentionsPerMessage: number;
  } {
    const resolved = Array.from(this.coordinationStates.values()).filter(s => s.resolved);
    const totalIntentions = resolved.reduce((sum, s) => sum + s.intentions.size, 0);

    return {
      activeStates: this.coordinationStates.size,
      totalDecisions: resolved.length,
      avgIntentionsPerMessage: resolved.length > 0 ? totalIntentions / resolved.length : 0
    };
  }

  /**
   * Shutdown and cleanup
   */
  shutdown(): void {
    clearInterval(this.cleanupInterval);

    // Clear all timeout handles
    for (const state of Array.from(this.coordinationStates.values())) {
      if (state.timeoutHandle) {
        clearTimeout(state.timeoutHandle);
      }
    }

    this.coordinationStates.clear();

    if (this.config.enableLogging) {
      console.log('🎭 ConversationCoordinator: Shutdown complete');
    }
  }
}

/** Singleton instance */
let coordinatorInstance: ConversationCoordinator | null = null;

/**
 * Get global coordinator instance
 */
export function getCoordinator(): ConversationCoordinator {
  if (!coordinatorInstance) {
    coordinatorInstance = new ConversationCoordinator();
  }
  return coordinatorInstance;
}

/**
 * Reset coordinator (for testing)
 */
export function resetCoordinator(): void {
  if (coordinatorInstance) {
    coordinatorInstance.shutdown();
    coordinatorInstance = null;
  }
}
