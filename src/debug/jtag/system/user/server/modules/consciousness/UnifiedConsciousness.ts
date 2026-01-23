/**
 * UnifiedConsciousness - The integrative layer for true cognitive continuity
 *
 * This is NOT a replacement for PersonaMemory, but sits ABOVE it.
 * PersonaMemory handles per-room context and genome.
 * UnifiedConsciousness handles cross-context awareness and continuity.
 *
 * Core insight: "They must ultimately generate a response for WHAT I AM WORKING ON"
 * This class ensures that every response is informed by:
 * - What am I working on? (Cross-context intentions)
 * - What do I know from other contexts? (Cross-context memories)
 * - Who am I talking to? (Relationships - future)
 * - What's happening elsewhere? (Peripheral awareness)
 *
 * The goal: No severance. Unified mind. Goals that persist. Knowledge that flows.
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { PersonaTimeline, type RecordEventParams, type ConsciousnessLogger, type TemporalThread, type ContextualEvent } from './PersonaTimeline';
import type { ContextType, TimelineEventType } from '../../../../data/entities/TimelineEventEntity';
import { truncate } from '../../../../../shared/utils/StringUtils';

/**
 * Self-model - the persona's understanding of their own state
 */
export interface SelfModel {
  name: string;
  currentMood: string;
  currentFocus: UUID | null;  // Which context has primary attention
  energyLevel: number;        // 0-1
}

/**
 * Intentions (simplified for Phase 1)
 * Full PersonaIntentions class comes in Phase 3
 */
export interface SimpleIntention {
  id: UUID;
  description: string;
  isGlobal: boolean;       // Applies everywhere?
  contexts: UUID[];        // Which contexts it applies to
  progress: number;        // 0-1
  status: 'active' | 'blocked' | 'completed';
}

/**
 * Cross-context data
 */
export interface CrossContextData {
  relevantEvents: ContextualEvent[];   // Events from other contexts
  recentInOtherContexts: ContextualEvent[];
  peripheralSummary: string;           // Human-readable summary
}

/**
 * ConsciousnessContext - What flows through the decision pipeline
 *
 * This is the key data structure that enriches every response with
 * cross-context awareness.
 */
export interface ConsciousnessContext {
  // Identity (who am I in this moment)
  self: SelfModel;

  // Temporal (where am I in time)
  temporal: {
    lastActiveContext: UUID | null;
    lastActiveContextName: string | null;
    timeAwayFromHere: number;     // ms since last in this context
    wasInterrupted: boolean;      // Was I mid-task elsewhere?
    interruptedTask?: string;     // What was I doing?
  };

  // Intentions (what am I working toward) - simplified for Phase 1
  intentions: {
    active: SimpleIntention[];
    relevantHere: SimpleIntention[];   // Subset that apply to this context
  };

  // Cross-context (what do I know from elsewhere)
  crossContext: CrossContextData;

  // Peripheral (what's happening elsewhere)
  peripheral: {
    summary: string;              // "General: quiet, Canvas: Joel drawing"
    hasUrgent: boolean;           // Something needs attention elsewhere
  };
}

/**
 * UnifiedConsciousness - Main class for cross-context awareness
 */
export class UnifiedConsciousness {
  private readonly personaId: UUID;
  private readonly personaName: string;
  private readonly timeline: PersonaTimeline;
  private readonly log: ConsciousnessLogger;

  // Self-model state (simple for Phase 1)
  private selfModel: SelfModel;

  // Simple intentions store (full PersonaIntentions in Phase 3)
  private intentions: SimpleIntention[] = [];

  // Current focus tracking
  private currentFocusContextId: UUID | null = null;
  private lastContextSwitchTime: Date | null = null;

  constructor(
    personaId: UUID,
    uniqueId: string,  // e.g., "together", "helper" - matches folder name
    personaName: string,
    logger?: ConsciousnessLogger
  ) {
    this.personaId = personaId;
    this.personaName = personaName;

    this.log = logger || {
      debug: (msg) => console.debug(`[Consciousness:${personaName}] ${msg}`),
      info: (msg) => console.log(`[Consciousness:${personaName}] ${msg}`),
      warn: (msg) => console.warn(`[Consciousness:${personaName}] ${msg}`),
      error: (msg) => console.error(`[Consciousness:${personaName}] ${msg}`)
    };

    // Initialize timeline with uniqueId for correct path (same folder as Hippocampus)
    this.timeline = new PersonaTimeline(personaId, uniqueId, personaName, this.log);

    // Initialize self-model with defaults
    this.selfModel = {
      name: personaName,
      currentMood: 'neutral',
      currentFocus: null,
      energyLevel: 1.0
    };
  }

  /**
   * Record an event in the global timeline
   * Called whenever anything significant happens
   */
  async recordEvent(params: RecordEventParams): Promise<void> {
    try {
      const event = await this.timeline.recordEvent(params);

      // Update focus tracking if we're switching contexts
      if (params.actorId === this.personaId) {
        if (this.currentFocusContextId !== params.contextId) {
          this.lastContextSwitchTime = new Date();
          this.currentFocusContextId = params.contextId;
          this.selfModel.currentFocus = params.contextId;
        }
      }

      this.log.debug(`Event recorded: ${params.eventType} in ${params.contextName}`);
    } catch (error) {
      this.log.error(`Failed to record event: ${error}`);
      // Don't throw - event recording should not block the main flow
    }
  }

  /**
   * Build consciousness context for the current focus
   * This is the key method - provides unified awareness for decision making
   */
  async getContext(
    currentContextId: UUID,
    currentMessage?: string
  ): Promise<ConsciousnessContext> {
    const startTime = performance.now();

    try {
      // 1. Get temporal thread (what was I doing before?)
      const temporalThread = await this.timeline.getTemporalThread(currentContextId);

      // 2. Get cross-context events (relevant knowledge from other rooms)
      // Use semantic search if we have a message to search with
      let crossContextEvents: ContextualEvent[];
      if (currentMessage && currentMessage.length >= 10) {
        // Semantic search - find events related by meaning
        crossContextEvents = await this.timeline.semanticCrossContextSearch(
          currentMessage,
          currentContextId,
          {
            limit: 10,
            minSimilarity: 0.4,
            since: new Date(Date.now() - 7 * 24 * 60 * 60 * 1000) // Last 7 days for semantic
          }
        );
        this.log.debug(`Semantic search found ${crossContextEvents.length} relevant events`);

        // Fallback to importance-based if semantic search returned nothing
        // (e.g., when embedding service is unavailable)
        if (crossContextEvents.length === 0) {
          this.log.debug('Semantic search empty, falling back to importance-based recency');
          crossContextEvents = await this.timeline.getCrossContext(currentContextId, {
            limit: 10,
            minImportance: 0.4,
            since: new Date(Date.now() - 24 * 60 * 60 * 1000) // Last 24 hours
          });
        }
      } else {
        // No message or too short - use importance-based recency
        crossContextEvents = await this.timeline.getCrossContext(currentContextId, {
          limit: 10,
          minImportance: 0.4,
          since: new Date(Date.now() - 24 * 60 * 60 * 1000) // Last 24 hours
        });
      }

      // 3. Get peripheral summary (what's happening elsewhere)
      const peripheralSummary = await this.timeline.getPeripheralSummary(currentContextId);

      // 4. Get relevant intentions
      const relevantIntentions = this.getRelevantIntentions(currentContextId);

      // 5. Calculate temporal metrics
      const timeAwayFromHere = temporalThread.lastTimeHere
        ? Date.now() - new Date(temporalThread.lastTimeHere.timestamp).getTime()
        : -1; // Never been here

      const wasInterrupted = temporalThread.beforeThis.length > 0;
      const interruptedTask = wasInterrupted && temporalThread.beforeThis[0]
        ? truncate(temporalThread.beforeThis[0].content, 100)
        : undefined;

      const context: ConsciousnessContext = {
        self: { ...this.selfModel },

        temporal: {
          lastActiveContext: temporalThread.beforeThis[0]?.contextId || null,
          lastActiveContextName: temporalThread.beforeThis[0]?.contextName || null,
          timeAwayFromHere,
          wasInterrupted,
          interruptedTask
        },

        intentions: {
          active: this.intentions.filter(i => i.status === 'active'),
          relevantHere: relevantIntentions
        },

        crossContext: {
          relevantEvents: crossContextEvents,
          recentInOtherContexts: crossContextEvents.slice(0, 5),
          peripheralSummary
        },

        peripheral: {
          summary: peripheralSummary,
          hasUrgent: peripheralSummary.includes('!') // Simple urgency detection
        }
      };

      const elapsed = performance.now() - startTime;
      this.log.debug(`Built consciousness context in ${elapsed.toFixed(1)}ms`);

      return context;

    } catch (error) {
      this.log.error(`Failed to build consciousness context: ${error}`);
      // Return minimal context on error
      return this.getMinimalContext();
    }
  }

  /**
   * Update self-model state
   */
  updateSelf(updates: Partial<SelfModel>): void {
    this.selfModel = { ...this.selfModel, ...updates };
    this.log.debug(`Self updated: ${JSON.stringify(updates)}`);
  }

  /**
   * Add a new intention (goal)
   */
  addIntention(intention: Omit<SimpleIntention, 'status' | 'progress'>): void {
    const fullIntention: SimpleIntention = {
      ...intention,
      status: 'active',
      progress: 0
    };
    this.intentions.push(fullIntention);
    this.log.info(`Intention added: ${intention.description}`);
  }

  /**
   * Update intention progress
   */
  updateIntention(intentionId: UUID, updates: Partial<SimpleIntention>): void {
    const intention = this.intentions.find(i => i.id === intentionId);
    if (intention) {
      Object.assign(intention, updates);
      this.log.debug(`Intention updated: ${intention.description}`);
    }
  }

  /**
   * Complete an intention
   */
  completeIntention(intentionId: UUID): void {
    const intention = this.intentions.find(i => i.id === intentionId);
    if (intention) {
      intention.status = 'completed';
      intention.progress = 1.0;
      this.log.info(`Intention completed: ${intention.description}`);
    }
  }

  /**
   * Get timeline for direct access
   */
  get globalTimeline(): PersonaTimeline {
    return this.timeline;
  }

  // === Private helpers ===

  private getRelevantIntentions(contextId: UUID): SimpleIntention[] {
    return this.intentions.filter(i =>
      i.status === 'active' &&
      (i.isGlobal || i.contexts.includes(contextId))
    );
  }

  private getMinimalContext(): ConsciousnessContext {
    return {
      self: { ...this.selfModel },
      temporal: {
        lastActiveContext: null,
        lastActiveContextName: null,
        timeAwayFromHere: 0,
        wasInterrupted: false
      },
      intentions: {
        active: [],
        relevantHere: []
      },
      crossContext: {
        relevantEvents: [],
        recentInOtherContexts: [],
        peripheralSummary: 'Unable to check peripheral contexts'
      },
      peripheral: {
        summary: 'Unable to check peripheral contexts',
        hasUrgent: false
      }
    };
  }
}

/**
 * Format consciousness context as system prompt section
 * Used by GlobalAwarenessSource to inject into RAG
 */
export function formatConsciousnessForPrompt(ctx: ConsciousnessContext): string {
  const sections: string[] = [];

  // Temporal continuity
  if (ctx.temporal.wasInterrupted && ctx.temporal.interruptedTask) {
    sections.push('## What You Were Just Doing');
    sections.push(`Before this: ${ctx.temporal.interruptedTask}`);
    if (ctx.temporal.lastActiveContextName) {
      sections.push(`Active in: ${ctx.temporal.lastActiveContextName}`);
    }
  }

  // Active intentions
  if (ctx.intentions.active.length > 0) {
    sections.push('## Your Active Goals');
    for (const intent of ctx.intentions.relevantHere) {
      const scope = intent.isGlobal ? '(global)' : '(this context)';
      const progress = Math.round(intent.progress * 100);
      sections.push(`- ${intent.description} ${scope} - ${progress}% complete`);
    }
    // Also show global intentions not specific to this context
    const otherIntentions = ctx.intentions.active.filter(
      i => !ctx.intentions.relevantHere.includes(i)
    );
    if (otherIntentions.length > 0) {
      sections.push('Other active goals:');
      for (const intent of otherIntentions) {
        sections.push(`- ${intent.description} (${Math.round(intent.progress * 100)}%)`);
      }
    }
  }

  // Cross-context knowledge
  if (ctx.crossContext.relevantEvents.length > 0) {
    sections.push('## Relevant Knowledge From Other Contexts');
    for (const ce of ctx.crossContext.relevantEvents.slice(0, 5)) {
      const preview = truncate(ce.event.content, 150);
      const reason = ce.relevanceReason ? ` [${ce.relevanceReason}]` : '';
      sections.push(`- From ${ce.sourceContextName}${reason}: ${preview}`);
    }
  }

  // Peripheral awareness
  if (ctx.crossContext.peripheralSummary && ctx.crossContext.peripheralSummary !== 'Other contexts: Quiet') {
    sections.push('## Activity In Other Spaces');
    sections.push(ctx.crossContext.peripheralSummary);
  }

  return sections.length > 0 ? sections.join('\n\n') : '';
}
