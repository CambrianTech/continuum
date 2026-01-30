/**
 * PersonaCentralNervousSystem
 *
 * Orchestration layer that coordinates multi-domain attention for PersonaUser.
 *
 * Service cycle (Rust-delegated):
 *   1. Poll tasks, generate self-tasks (TS — DB access)
 *   2. Wait for work (signal-based)
 *   3. Rust service_cycle() → consolidate, state-gate, schedule, return next item
 *   4. Dispatch item to handler
 *
 * ALL scheduling logic lives in Rust. TS executes what Rust decides.
 */

import type { CNSConfig } from './CNSTypes';
import { SubsystemLogger } from '../being/logging/SubsystemLogger';
import { fromRustServiceItem } from '../QueueItemTypes';

export class PersonaCentralNervousSystem {
  private readonly config: CNSConfig;
  private readonly logger: SubsystemLogger;

  constructor(config: CNSConfig) {
    this.config = config;
    this.logger = new SubsystemLogger('cns', config.personaId, config.uniqueId);

    this.logger.info(`Initialized CNS with Rust-delegated scheduling`);
    this.logger.info(`Rust bridge: connected`);
    this.logger.info(`Background threads: ${config.allowBackgroundThreads ? 'enabled' : 'disabled'}`);
  }

  /**
   * Single service cycle — the heart of the autonomous entity.
   *
   * Delegates ALL scheduling to Rust:
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

    // STEP 2: Rust-delegated scheduling
    await this.serviceViaRust();
  }

  /**
   * Rust-delegated service cycle.
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
    const bridge = this.config.rustBridge;

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

      // Delegate to PersonaUser via callback
      await this.config.handleChatMessage(queueItem);
    } else {
      // No work — Rust says rest for wait_ms
      // Note: wait_ms is advisory; the outer loop will call waitForWork() next cycle
      // which provides signal-based wakeup if new work arrives before wait_ms
      this.logger.debug(`Rust service cycle: no work (wait_ms=${result.wait_ms}, stats=${result.stats.total_size} total)`);
    }
  }

  /**
   * Shutdown CNS subsystem (cleanup)
   */
  shutdown(): void {
    this.logger.info('CNS subsystem shutting down...');
    this.logger.close();
  }
}
