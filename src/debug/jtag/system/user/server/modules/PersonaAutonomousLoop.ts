/**
 * PersonaAutonomousLoop - Thin orchestrator for PersonaUser servicing
 *
 * All background timers (task polling, self-task generation, training checks) have
 * been moved to Rust's ChannelModule.tick() — ONE tick loop replaces 30+ TS setIntervals.
 *
 * What remains in TypeScript:
 * - Signal-based service loop (wait for work → Rust schedule → execute in TS)
 * - Item dispatch (LoRA activation, message evaluation, task execution)
 * - These stay in TS because they call AI providers, RAG, file I/O
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '../../../shared/Constants';
import type { TaskEntity } from '../../../data/entities/TaskEntity';
import { RoomEntity } from '../../../data/entities/RoomEntity';
import { inboxMessageToProcessable, type InboxTask, type QueueItem } from './QueueItemTypes';
import { fromRustServiceItem } from './QueueItemTypes';
import type { FastPathDecision } from './central-nervous-system/CNSTypes';
import { LearningScheduler } from '../../../genome/server/LearningScheduler';

// Import PersonaUser directly - circular dependency is fine for type-only imports
import type { PersonaUser } from '../PersonaUser';

export class PersonaAutonomousLoop {
  private servicingLoopActive: boolean = false;
  private log: (message: string) => void;

  constructor(private readonly personaUser: PersonaUser, logger?: (message: string) => void) {
    this.log = logger || console.log.bind(console);
  }

  /**
   * Start autonomous servicing loop.
   *
   * Only creates the reactive service loop. All background timers
   * (task polling, self-task generation, training checks) run in Rust.
   */
  startAutonomousServicing(): void {
    this.log(`🔄 ${this.personaUser.displayName}: Starting autonomous servicing (SIGNAL-BASED WAITING)`);
    this.servicingLoopActive = true;

    // Register with system-wide learning scheduler for continuous learning
    try {
      const scheduler = LearningScheduler.sharedInstance();
      scheduler.registerPersona(
        this.personaUser.id,
        this.personaUser.displayName,
        this.personaUser.trainingManager,
        this.personaUser.trainingAccumulator,
      );
    } catch {
      // Non-fatal — continuous learning is optional
    }

    this.runServiceLoop().catch((error: any) => {
      this.log(`❌ ${this.personaUser.displayName}: Service loop crashed: ${error}`);
    });
  }

  /**
   * Continuous service loop — runs until servicingLoopActive = false.
   * Each iteration: wait for signal → Rust serviceCycleFull → dispatch item → repeat
   */
  private async runServiceLoop(): Promise<void> {
    while (this.servicingLoopActive) {
      try {
        await this.serviceInbox();
      } catch (error) {
        this.log(`❌ ${this.personaUser.displayName}: Error in service loop: ${error}`);
      }
    }
    this.log(`🛑 ${this.personaUser.displayName}: Service loop stopped`);
  }

  /**
   * Single service cycle — wait for work, then drain via Rust.
   *
   * Inlined from PersonaCentralNervousSystem (eliminated the wrapper):
   * 1. Wait for work (signal-based, cadence from PersonaState)
   * 2. Drain loop: call Rust serviceCycleFull repeatedly until queue empty
   */
  private async serviceInbox(): Promise<void> {
    const cadence = this.personaUser.prefrontal!.personaState.getCadence();
    const hasWork = await this.personaUser.inbox.waitForWork(cadence);

    if (!hasWork) {
      return;
    }

    // Drain loop — process all queued items before returning to wait
    let itemsProcessed = 0;
    const MAX_DRAIN = 20;

    while (itemsProcessed < MAX_DRAIN) {
      const bridge = this.personaUser.rustCognitionBridge!;
      const result = await bridge.serviceCycleFull();

      if (!result.should_process || !result.item) {
        break;
      }

      // Convert Rust JSON → TS QueueItem
      const queueItem = fromRustServiceItem(result.item as Record<string, unknown>);
      if (!queueItem) {
        this.log(`⚠️ ${this.personaUser.displayName}: Rust returned unparseable item`);
        break;
      }

      // Dispatch to handler with pre-computed decision
      await this.handleItem(queueItem, result.decision ?? undefined);
      itemsProcessed++;
    }

    // After draining queue, tick the learning scheduler
    // Low overhead: just increments a counter most cycles, triggers training when ready
    LearningScheduler.sharedInstance().tick(this.personaUser.id).catch(err => {
      this.log(`⚠️ ${this.personaUser.displayName}: Learning scheduler tick failed: ${err}`);
    });
  }

  /**
   * Handle a dequeued item — dispatch based on type.
   *
   * Handles both messages and tasks:
   * - Messages: LoRA activation → evaluateAndPossiblyRespondWithCognition → bookmark
   * - Tasks: mark in_progress → LoRA activation → executeTask
   */
  async handleItem(item: QueueItem, decision?: FastPathDecision): Promise<void> {
    const handlerStart = performance.now();

    // If this is a task, update status to 'in_progress' in database (prevents re-polling)
    if (item.type === 'task') {
      await ORM.update<TaskEntity>(
        COLLECTIONS.TASKS,
        item.taskId,
        { status: 'in_progress', startedAt: new Date() }
      );
    }

    // Activate appropriate LoRA adapter based on domain
    if (item.domain) {
      const domainToAdapter: Record<string, string> = {
        'chat': 'conversational',
        'code': 'typescript-expertise',
        'self': 'self-improvement'
      };
      const adapterName = domainToAdapter[item.domain] || 'conversational';
      await this.personaUser.memory.genome.activateSkill(adapterName);
    }

    if (item.type === 'message') {
      const processable = inboxMessageToProcessable(item);
      const senderIsHuman = item.senderType === 'human';
      const messageText = item.content ?? '';

      await this.personaUser.evaluateAndPossiblyRespondWithCognition(processable, senderIsHuman, messageText, decision);
      await this.personaUser.updateMessageBookmark(item.roomId, item.timestamp, item.id);

      const totalMs = performance.now() - handlerStart;
      this.log(`[TIMING] ${this.personaUser.displayName}: handleItem total=${totalMs.toFixed(1)}ms (hasDecision=${!!decision})`);
    } else if (item.type === 'task') {
      await this.executeTask(item);
    }

    // Update inbox load in state (affects mood calculation)
    this.personaUser.personaState.updateInboxLoad(this.personaUser.inbox.getSize());
  }

  /**
   * Execute a task based on its type.
   * Delegates to PersonaTaskExecutor for actual execution.
   */
  private async executeTask(task: InboxTask): Promise<void> {
    if (task.domain === 'code') {
      const roomId = task.metadata?.roomId ?? task.contextId;
      const roomSlug = await this.resolveRoomSlug(roomId);
      await this.personaUser.ensureWorkspace({
        contextKey: roomSlug,
        mode: 'worktree',
        taskSlug: roomSlug,
      });
    }
    await this.personaUser.taskExecutor.executeTask(task);
  }

  /**
   * Resolve a room UUID to its uniqueId slug for workspace naming.
   */
  private async resolveRoomSlug(roomId: UUID): Promise<string> {
    try {
      const room = await ORM.read<RoomEntity>(COLLECTIONS.ROOMS, roomId);
      if (room?.uniqueId) return room.uniqueId;
    } catch {
      // Room lookup failed — use truncated UUID
    }
    return roomId.slice(0, 8);
  }

  /**
   * Stop autonomous servicing loop.
   * No timers to clear — Rust handles all background work.
   */
  async stopServicing(): Promise<void> {
    this.servicingLoopActive = false;

    // Unregister from learning scheduler
    try {
      LearningScheduler.sharedInstance().unregisterPersona(this.personaUser.id);
    } catch {
      // Non-fatal
    }

    this.log(`🔄 ${this.personaUser.displayName}: Stopped autonomous servicing loop`);
  }
}
