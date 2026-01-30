/**
 * PersonaCentralNervousSystem
 *
 * Orchestration layer that coordinates multi-domain attention for PersonaUser.
 *
 * Service cycle (Rust-delegated, Phase 2):
 *   1. Poll tasks, generate self-tasks (TS — DB access)
 *   2. Wait for work (signal-based)
 *   3. Rust service_cycle() → consolidate, state-gate, schedule, return next item
 *   4. Dispatch item to handler
 *
 * Fallback (TS channels):
 *   If Rust bridge unavailable, falls back to TS channel registry + legacy queue.
 */

import type { CNSConfig } from './CNSTypes';
import type { CognitiveContext } from '../cognitive-schedulers/ICognitiveScheduler';
import { ActivityDomain } from '../cognitive-schedulers/ICognitiveScheduler';
import { SubsystemLogger } from '../being/logging/SubsystemLogger';
import { getEffectivePriority } from '../PersonaInbox';
import { fromRustServiceItem } from '../QueueItemTypes';

export class PersonaCentralNervousSystem {
  private readonly config: CNSConfig;
  private readonly logger: SubsystemLogger;

  constructor(config: CNSConfig) {
    this.config = config;
    this.logger = new SubsystemLogger('cns', config.personaId, config.uniqueId);

    this.logger.info(`Initialized CNS with ${config.scheduler.name} scheduler`);
    this.logger.info(`Enabled domains: ${config.enabledDomains.join(', ')}`);
    this.logger.info(`Channels registered: ${config.channelRegistry.domains().join(', ')}`);
    this.logger.info(`Rust bridge: ${config.rustBridge ? 'connected' : 'not available (TS fallback)'}`);
    this.logger.info(`Background threads: ${config.allowBackgroundThreads ? 'enabled' : 'disabled'}`);
  }

  /**
   * Single service cycle — the heart of the autonomous entity.
   *
   * Phase 2: Delegates scheduling to Rust when bridge available.
   * - Rust handles: consolidation, state gating, priority ordering, item selection
   * - TS handles: task polling, self-task generation, item execution (LLM, DB, tools)
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
      await this.config.personaState.rest(cadence);
      return;
    }

    // STEP 2: Try Rust-delegated scheduling (Phase 2)
    if (this.config.rustBridge) {
      await this.serviceViaRust();
      return;
    }

    // STEP 3: Fallback to TS channel service
    const channelRegistry = this.config.channelRegistry;
    if (channelRegistry.hasWork()) {
      await this.serviceChannels();
      return;
    }

    // STEP 4: Fall back to legacy flat-queue path
    await this.serviceLegacyQueue();
  }

  /**
   * Rust-delegated service cycle (Phase 2).
   *
   * Rust's service_cycle() does ALL scheduling work in <1ms:
   * - Consolidates all channels (items decide merge policy)
   * - Updates persona state (inbox_load, mood)
   * - Checks urgent channels first (AUDIO → CHAT → BACKGROUND)
   * - State-gates non-urgent items (mood/energy threshold)
   * - Returns next item to process or adaptive wait cadence
   *
   * TS just executes what Rust decided.
   */
  private async serviceViaRust(): Promise<void> {
    const bridge = this.config.rustBridge!;

    try {
      const result = await bridge.serviceCycle();

      if (result.should_process && result.item) {
        // Convert Rust JSON item → TS QueueItem
        const queueItem = fromRustServiceItem(result.item as Record<string, unknown>);
        if (!queueItem) {
          this.logger.warn(`Rust returned unparseable item: ${JSON.stringify(result.item).slice(0, 200)}`);
          return;
        }

        const channelName = result.channel ?? 'unknown';
        this.logger.info(`[rust:${channelName}] Processing ${queueItem.type} (priority=${queueItem.priority.toFixed(2)}, stats=${result.stats.total_size} total)`);

        // Delegate to PersonaUser via callback (same as TS channel path)
        await this.config.handleChatMessage(queueItem);
      } else {
        // No work — Rust says rest for wait_ms
        // Note: wait_ms is advisory; the outer loop will call waitForWork() next cycle
        // which provides signal-based wakeup if new work arrives before wait_ms
        this.logger.debug(`Rust service cycle: no work (wait_ms=${result.wait_ms}, stats=${result.stats.total_size} total)`);
      }
    } catch (error) {
      this.logger.warn(`Rust service cycle failed, falling back to TS channels: ${error}`);
      // Fallback: try TS channels
      if (this.config.channelRegistry.hasWork()) {
        await this.serviceChannels();
      } else {
        await this.serviceLegacyQueue();
      }
    }
  }

  /**
   * Multi-channel service loop (TS fallback).
   *
   * Items control their own destiny:
   * - consolidate(): items decide if/how they merge
   * - hasUrgentWork: items decide what's urgent
   * - pop(): items decide sort order via compareTo()
   */
  private async serviceChannels(): Promise<void> {
    const registry = this.config.channelRegistry;

    // STEP 1: Consolidate all channels (items decide how)
    for (const channel of registry.all()) {
      channel.consolidate();
    }

    // STEP 2: Build cognitive context for scheduler decisions
    const context = this.buildCognitiveContext();

    // STEP 3: Get domain priority from scheduler
    const priorities = this.config.scheduler.getDomainPriority(context);

    // STEP 4: Service channels in priority order
    for (const domain of priorities) {
      const channel = registry.get(domain);
      if (!channel || !channel.hasWork) continue;

      // Urgent work bypasses scheduler (items said so — e.g., voice is always urgent)
      const shouldService = channel.hasUrgentWork
        || await this.config.scheduler.shouldServiceDomain(domain, context);

      if (!shouldService) {
        this.logger.debug(`Skipping ${channel.name} channel (scheduler declined, size=${channel.size})`);
        continue;
      }

      // Peek to check engagement threshold (mood/energy gating)
      const candidate = channel.peek();
      if (!candidate) continue;

      // Urgent items bypass mood/energy check
      if (!candidate.isUrgent && !this.config.personaState.shouldEngage(candidate.effectivePriority)) {
        this.logger.debug(`Skipping ${channel.name} item (priority=${candidate.effectivePriority.toFixed(2)}, below engagement threshold)`);
        const restCadence = this.config.personaState.getCadence();
        await this.config.personaState.rest(restCadence);
        continue;
      }

      // Pop and process
      const item = channel.pop();
      if (!item) continue;

      const waitMs = item.enqueuedAt ? Date.now() - item.enqueuedAt : 0;
      this.logger.info(`[${channel.name}] Processing ${item.itemType} (priority=${item.effectivePriority.toFixed(2)}, waitMs=${waitMs}, urgent=${item.isUrgent}, channelSize=${channel.size})`);

      // Delegate to PersonaUser via callback
      await this.config.handleQueueItem(item);
    }
  }

  /**
   * Legacy flat-queue service path (backward compatibility).
   * Handles items that haven't been routed to channels yet.
   * Will be removed once all items flow through channels.
   */
  private async serviceLegacyQueue(): Promise<void> {
    const context = this.buildCognitiveContext();

    const shouldServiceChat = await this.config.scheduler.shouldServiceDomain(
      ActivityDomain.CHAT,
      context
    );

    if (!shouldServiceChat) {
      this.logger.debug('Scheduler decided not to service chat (legacy path)');
      return;
    }

    // Peek at highest priority message from legacy inbox
    const candidates = await this.config.inbox.peek(1);
    if (candidates.length === 0) return;

    const message = candidates[0];
    const effectivePriority = getEffectivePriority(message);

    if (!this.config.personaState.shouldEngage(effectivePriority)) {
      this.logger.debug(`Skipping message (legacy path, effective=${effectivePriority.toFixed(2)})`);
      const cadence = this.config.personaState.getCadence();
      await this.config.personaState.rest(cadence);
      return;
    }

    await this.config.inbox.pop(0);

    const waitMs = message.enqueuedAt ? Date.now() - message.enqueuedAt : 0;
    this.logger.info(`[legacy] Processing message (effective=${effectivePriority.toFixed(2)}, waitMs=${waitMs})`);

    await this.config.handleChatMessage(message);
  }

  /**
   * Build cognitive context for scheduler decisions.
   * Uses both legacy inbox stats and channel registry stats.
   */
  private buildCognitiveContext(): CognitiveContext {
    const state = this.config.personaState.getState();
    const registry = this.config.channelRegistry;

    return {
      energy: state.energy,
      mood: state.mood,

      // Activity levels from channels
      activeGames: 0,
      unreadMessages: (registry.get(ActivityDomain.CHAT)?.size ?? 0)
        + (registry.get(ActivityDomain.AUDIO)?.size ?? 0)
        + this.config.inbox.getSize(),  // Include legacy inbox
      pendingReviews: 0,
      backgroundTasksPending: registry.get(ActivityDomain.BACKGROUND)?.size ?? 0,

      // Performance
      avgResponseTime: 0,
      queueBacklog: registry.totalSize() + this.config.inbox.getSize(),

      // System
      cpuPressure: 0,
      memoryPressure: 0,

      // Model capabilities
      modelCapabilities: new Set(['text'])
    };
  }

  /**
   * Shutdown CNS subsystem (cleanup)
   */
  shutdown(): void {
    this.logger.info('CNS subsystem shutting down...');
    this.config.channelRegistry.clearAll();
    this.logger.close();
  }
}
