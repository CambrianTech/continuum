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
import type { GapDetector } from './GapDetector';
import type { SelfTaskGenerator } from './SelfTaskGenerator';

// Import PersonaUser directly - circular dependency is fine for type-only imports
import type { PersonaUser } from '../PersonaUser';
import { PersonaTimingConfig } from './PersonaTimingConfig';

/** Gap assessment runs every N service cycles (~25-50s during active operation) */
const GAP_ASSESSMENT_INTERVAL = 50;

export class PersonaAutonomousLoop {
  private servicingLoopActive: boolean = false;
  private log: (message: string) => void;

  /** Cycle counter for gap assessment cadence */
  private gapCycleCount = 0;

  /** Circuit breaker: consecutive error tracking */
  private consecutiveFailures = 0;
  private circuitOpenUntil = 0;

  /** Optional gap detection + self-task generation */
  private gapDetector: GapDetector | null = null;
  private selfTaskGenerator: SelfTaskGenerator | null = null;

  constructor(private readonly personaUser: PersonaUser, logger: (message: string) => void) {
    this.log = logger;
  }

  /**
   * Wire gap detector and self-task generator for autonomous learning.
   * Called after PersonaUser initializes these components.
   */
  setGapDetection(gapDetector: GapDetector, selfTaskGenerator: SelfTaskGenerator): void {
    this.gapDetector = gapDetector;
    this.selfTaskGenerator = selfTaskGenerator;
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
   * Each iteration: wait for signal → Rust serviceCycleFull → dispatch item → repeat.
   *
   * Circuit breaker: after maxConsecutiveFailures errors in a row, the persona
   * enters cooldown (stops processing for cooldownMs). On any success, the
   * failure counter resets. This prevents hammering a broken provider.
   */
  private async runServiceLoop(): Promise<void> {
    const { maxConsecutiveFailures, cooldownMs } = PersonaTimingConfig.circuitBreaker;

    while (this.servicingLoopActive) {
      // Circuit breaker: if open, wait until cooldown expires
      if (this.circuitOpenUntil > 0) {
        const remaining = this.circuitOpenUntil - Date.now();
        if (remaining > 0) {
          this.log(`⚡ ${this.personaUser.displayName}: Circuit breaker open — cooling down ${Math.ceil(remaining / 1000)}s`);
          await new Promise(resolve => setTimeout(resolve, Math.min(remaining, 5000)));
          continue;
        }
        // Cooldown expired — close circuit, reset counter
        this.log(`⚡ ${this.personaUser.displayName}: Circuit breaker closed — resuming`);
        this.circuitOpenUntil = 0;
        this.consecutiveFailures = 0;
      }

      try {
        await this.serviceInbox();
        this.consecutiveFailures = 0;
      } catch (error) {
        this.consecutiveFailures++;
        this.log(`❌ ${this.personaUser.displayName}: Error in service loop (${this.consecutiveFailures}/${maxConsecutiveFailures}): ${error}`);

        if (this.consecutiveFailures >= maxConsecutiveFailures) {
          this.circuitOpenUntil = Date.now() + cooldownMs;
          this.log(`⚡ ${this.personaUser.displayName}: Circuit breaker OPEN — ${cooldownMs / 1000}s cooldown after ${this.consecutiveFailures} consecutive failures`);
        }
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

    // Periodically assess gaps and generate self-learning tasks
    this.gapCycleCount++;
    if (this.gapDetector && this.selfTaskGenerator && this.gapCycleCount >= GAP_ASSESSMENT_INTERVAL) {
      this.gapCycleCount = 0;
      this.runGapAssessment().catch(err => {
        this.log(`⚠️ ${this.personaUser.displayName}: Gap assessment failed: ${err}`);
      });
    }
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

    // If this is a task, verify it still exists then update status to 'in_progress'
    // Rust queue holds stale references to deleted tasks — check before expensive update
    if (item.type === 'task') {
      try {
        const existing = await ORM.read<TaskEntity>(COLLECTIONS.TASKS, item.taskId, 'default');
        if (!existing || existing.status === 'completed') {
          // Ghost task — deleted or already done. Skip silently (this is normal churn).
          return;
        }
        await ORM.update<TaskEntity>(
          COLLECTIONS.TASKS,
          item.taskId,
          { status: 'in_progress', startedAt: new Date() },
          true,
          'default'
        );
      } catch {
        // Task vanished between read and update — skip it
        this.log(`⚠️ ${this.personaUser.displayName}: Task ${item.taskId.slice(0, 8)} vanished before execution`);
        return;
      }
    }

    // Activate appropriate LoRA adapter based on domain
    // Uses Rust DomainClassifier for dynamic adapter-aware routing
    if (item.type === 'message' && item.content && this.personaUser.rustCognitionBridge) {
      try {
        const classification = await this.personaUser.rustCognitionBridge.classifyDomain(item.content);
        if (classification.adapter_name) {
          await this.personaUser.memory.genome.activateSkill(classification.adapter_name);
        }
      } catch {
        // Classification failure is non-fatal — proceed without adapter activation
      }
    } else if (item.domain) {
      // Task-domain fallback for non-message items or when Rust bridge unavailable
      await this.personaUser.memory.genome.activateForDomain(item.domain);
    }

    if (item.type === 'message') {
      const processable = inboxMessageToProcessable(item);
      const senderIsHuman = item.senderType === 'human' || item.senderType === 'agent';
      const messageText = item.content ?? '';

      // ALWAYS advance bookmark, even if response fails. Otherwise a single
      // failed message (e.g., provider 400/timeout) blocks the persona forever —
      // Rust re-polls the same un-bookmarked message every tick cycle.
      try {
        await this.personaUser.evaluateAndPossiblyRespondWithCognition(processable, senderIsHuman, messageText, decision);
      } catch (error: any) {
        this.log(`⚠️ ${this.personaUser.displayName}: Failed to respond to message ${item.id?.slice(0, 8)}: ${error.message ?? error}`);
      } finally {
        await this.personaUser.updateMessageBookmark(item.roomId, item.timestamp, item.id);
      }

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
      const room = await ORM.read<RoomEntity>(COLLECTIONS.ROOMS, roomId, 'default');
      if (room?.uniqueId) return room.uniqueId;
    } catch {
      // Room lookup failed — use truncated UUID
    }
    return roomId.slice(0, 8);
  }

  /**
   * Run gap assessment and generate self-learning tasks.
   * Fire-and-forget from the service loop — errors are logged, never thrown.
   */
  private async runGapAssessment(): Promise<void> {
    const report = await this.gapDetector!.assess();
    if (report.gaps.length > 0) {
      await this.selfTaskGenerator!.generateFromGaps(report);
    }
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
