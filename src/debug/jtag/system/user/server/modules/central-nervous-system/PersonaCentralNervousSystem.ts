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
   * HOT PATH ONLY: wait → Rust schedule → execute → drain → repeat.
   * DB polling and self-task generation run on separate timers (see PersonaAutonomousLoop).
   *
   * Drain loop: after processing one item, immediately check for more.
   * Only returns to waitForWork when Rust says the queue is empty.
   */
  async serviceCycle(): Promise<void> {
    // STEP 1: Wait for work (signal-based, delegates to inbox)
    const cadence = this.config.personaState.getCadence();
    const hasWork = await this.config.inbox.waitForWork(cadence);

    if (!hasWork) {
      return; // No work — loop will call us again
    }

    // STEP 2: Drain loop — process all queued items before returning to wait
    await this.drainQueue();
  }

  /**
   * Drain all queued items from Rust.
   * Keeps calling Rust service_cycle() until no more work is available.
   * This eliminates the overhead of re-entering waitForWork between items.
   */
  private async drainQueue(): Promise<void> {
    let itemsProcessed = 0;
    const MAX_DRAIN = 20; // Safety cap — don't monopolize the event loop forever

    while (itemsProcessed < MAX_DRAIN) {
      const processed = await this.serviceViaRust();
      if (!processed) {
        break; // Queue empty, return to wait
      }
      itemsProcessed++;
    }

    if (itemsProcessed > 1) {
      this.logger.info(`Drained ${itemsProcessed} items in burst`);
    }
  }

  /**
   * Rust-delegated service cycle (MERGED: schedule + fast-path decision in ONE IPC call).
   *
   * Rust's serviceCycleFull() does ALL scheduling + cognition in <1ms:
   * - Consolidates all channels (items decide merge policy)
   * - Updates persona state (inbox_load, mood)
   * - Checks urgent channels first (AUDIO → CHAT → BACKGROUND)
   * - State-gates non-urgent items (mood/energy threshold)
   * - Runs fast-path decision on the dequeued item (dedup, mention detection, state gating)
   * - Returns next item + decision in ONE IPC round-trip
   *
   * TS just executes what Rust decided.
   * Returns true if an item was processed (drain loop continues).
   */
  private async serviceViaRust(): Promise<boolean> {
    const bridge = this.config.rustBridge;
    const ipcStart = performance.now();

    const result = await bridge.serviceCycleFull();

    const ipcMs = performance.now() - ipcStart;

    if (result.should_process && result.item) {
      // Convert Rust JSON item → TS QueueItem
      const parseStart = performance.now();
      const queueItem = fromRustServiceItem(result.item as Record<string, unknown>);
      const parseMs = performance.now() - parseStart;

      if (!queueItem) {
        this.logger.warn(`Rust returned unparseable item: ${JSON.stringify(result.item).slice(0, 200)}`);
        return false;
      }

      const channelName = result.channel ?? 'unknown';
      const decisionStr = result.decision
        ? `respond=${result.decision.should_respond}`
        : 'no-decision';
      this.logger.info(`[rust:${channelName}] Processing ${queueItem.type} (priority=${queueItem.priority.toFixed(2)}, stats=${result.stats.total_size} total) [ipc=${ipcMs.toFixed(1)}ms, parse=${parseMs.toFixed(1)}ms, ${decisionStr}]`);

      // Delegate to PersonaUser via callback — pass pre-computed decision
      const handlerStart = performance.now();
      await this.config.handleChatMessage(queueItem, result.decision ?? undefined);
      const handlerMs = performance.now() - handlerStart;

      this.logger.info(`[rust:${channelName}] Handler complete (${handlerMs.toFixed(1)}ms total, ipc=${ipcMs.toFixed(1)}ms)`);
      return true;
    }

    return false;
  }

  /**
   * Shutdown CNS subsystem (cleanup)
   */
  shutdown(): void {
    this.logger.info('CNS subsystem shutting down...');
    this.logger.close();
  }
}
