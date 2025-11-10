/**
 * PersonaCentralNervousSystem
 *
 * Thin orchestration layer over existing PersonaUser modules.
 * Does NOT replace existing code - just coordinates it for multi-domain capability.
 *
 * Current behavior: Identical to PersonaUser.serviceInbox()
 * Future behavior: Multi-domain attention allocation
 */

import type { CNSConfig } from './CNSTypes';
import type { CognitiveContext } from '../cognitive-schedulers/ICognitiveScheduler';
import { ActivityDomain } from '../cognitive-schedulers/ICognitiveScheduler';

export class PersonaCentralNervousSystem {
  private readonly config: CNSConfig;

  constructor(config: CNSConfig) {
    this.config = config;
    this.log(`Initialized CNS with ${config.scheduler.name} scheduler`);
    this.log(`Enabled domains: ${config.enabledDomains.join(', ')}`);
    this.log(`Background threads: ${config.allowBackgroundThreads ? 'enabled' : 'disabled'}`);
  }

  /**
   * Single service cycle - replaces PersonaUser.serviceInbox()
   *
   * Phase 1: Identical behavior to existing code
   * Phase 2+: Multi-domain support
   */
  async serviceCycle(): Promise<void> {
    // STEP 0a: Poll task database for pending tasks assigned to this persona
    await this.config.pollTasks();

    // STEP 0b: Generate self-tasks for autonomous work creation
    await this.config.generateSelfTasks();

    // STEP 1: Wait for work (signal-based, delegates to inbox)
    const cadence = this.config.personaState.getCadence();
    const hasWork = await this.config.inbox.waitForWork(cadence);

    if (!hasWork) {
      // Timeout - rest to recover energy (existing behavior)
      await this.config.personaState.rest(cadence);
      return;
    }

    // STEP 2: Build context for scheduler decision
    const context = this.buildCognitiveContext();

    // STEP 3: Check if we should service chat domain
    const shouldServiceChat = await this.config.scheduler.shouldServiceDomain(
      ActivityDomain.CHAT,
      context
    );

    if (!shouldServiceChat) {
      this.log('Scheduler decided not to service chat');
      return;
    }

    // STEP 4: Service chat domain (for now, this is all we do)
    // Future: Loop through multiple domains based on attention allocation
    await this.serviceChatDomain();
  }

  /**
   * Build cognitive context for scheduler decisions
   */
  private buildCognitiveContext(): CognitiveContext {
    const state = this.config.personaState.getState();

    return {
      // Energy and mood
      energy: state.energy,
      mood: state.mood,

      // Activity levels (Phase 1: chat only)
      activeGames: 0,              // Not implemented yet
      unreadMessages: this.config.inbox.getSize(),
      pendingReviews: 0,           // Not implemented yet
      backgroundTasksPending: 0,   // Not implemented yet

      // Performance
      avgResponseTime: 0,          // Not tracked yet
      queueBacklog: this.config.inbox.getSize(),

      // System
      cpuPressure: 0,              // Not monitored yet
      memoryPressure: 0,           // Not monitored yet

      // Model capabilities
      modelCapabilities: new Set(['text']) // Basic for now
    };
  }

  /**
   * Service chat domain - delegates to PersonaUser via callback
   */
  private async serviceChatDomain(): Promise<void> {
    // Peek at highest priority message
    const candidates = await this.config.inbox.peek(1);
    if (candidates.length === 0) {
      return;
    }

    const message = candidates[0];

    // Check if we should engage based on mood/energy (existing behavior)
    if (!this.config.personaState.shouldEngage(message.priority)) {
      this.log(`Skipping message (priority=${message.priority.toFixed(2)}, mood=${this.config.personaState.getState().mood})`);

      // Rest while skipping to recover energy
      const cadence = this.config.personaState.getCadence();
      await this.config.personaState.rest(cadence);
      return;
    }

    // Pop message from inbox (we're processing it now)
    await this.config.inbox.pop(0); // Immediate pop (no timeout)

    this.log(`Processing message (priority=${message.priority.toFixed(2)}, mood=${this.config.personaState.getState().mood}, inbox remaining=${this.config.inbox.getSize()})`);

    // Delegate to PersonaUser via callback (existing logic)
    await this.config.handleChatMessage(message);
  }

  /**
   * Logging helper
   */
  private log(message: string): void {
    console.log(`🧠 ${this.config.personaName} [CNS]: ${message}`);
  }
}
