/**
 * PersonaAutonomousLoop - Handles autonomous servicing loop for PersonaUser
 *
 * REFACTORING: Extracted from PersonaUser.ts (lines 1893-2140)
 * Pure function extraction - no behavioral changes
 *
 * This module manages:
 * - RTOS-inspired autonomous servicing loop
 * - Signal-based waiting (not polling) for instant response
 * - Adaptive cadence based on mood/energy state
 * - Task polling from database
 * - Self-task generation
 * - Message handling via CNS orchestration
 * - Training readiness checks
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '../../../shared/Constants';
import type { TaskEntity } from '../../../data/entities/TaskEntity';
import { RoomEntity } from '../../../data/entities/RoomEntity';
import { taskEntityToInboxTask, inboxMessageToProcessable, type InboxTask, type QueueItem } from './QueueItemTypes';
import type { FastPathDecision } from './central-nervous-system/CNSTypes';

// Import PersonaUser directly - circular dependency is fine for type-only imports
import type { PersonaUser } from '../PersonaUser';

export class PersonaAutonomousLoop {
  private servicingLoopActive: boolean = false;
  private trainingCheckLoop: NodeJS.Timeout | null = null;
  private taskPollLoop: NodeJS.Timeout | null = null;
  private selfTaskLoop: NodeJS.Timeout | null = null;
  private log: (message: string) => void;

  constructor(private readonly personaUser: PersonaUser, logger?: (message: string) => void) {
    this.log = logger || console.log.bind(console);
  }

  /**
   * Start autonomous servicing loop
   *
   * Creates:
   * 1. Continuous async service loop (signal-based waiting, not polling)
   * 2. Task poll loop (every 60 seconds) — OFF the hot path, staggered start
   * 3. Self-task generation loop (every 60 seconds) — OFF the hot path, staggered start
   * 4. Training readiness check loop (every 120 seconds)
   *
   * Architecture:
   * - Hot path is ONLY: wait for signal → Rust service_cycle → execute → drain → repeat
   * - DB queries and self-task generation run on their own timers, never blocking the hot path
   * - Loop uses signal/mutex pattern (RTOS-style, performant, no CPU spinning)
   * - Staggered starts prevent thundering herd (10+ personas polling simultaneously)
   */
  startAutonomousServicing(): void {
    this.log(`🔄 ${this.personaUser.displayName}: Starting autonomous servicing (SIGNAL-BASED WAITING)`);

    // Create continuous async loop (not setInterval) - signal-based waiting
    this.servicingLoopActive = true;
    this.runServiceLoop().catch((error: any) => {
      this.log(`❌ ${this.personaUser.displayName}: Service loop crashed: ${error}`);
    });

    // Stagger timer starts to prevent thundering herd (10+ personas all firing at once)
    const stagger = Math.floor(Math.random() * 15000); // 0-15s random offset

    // Task polling on separate timer (OFF hot path)
    setTimeout(() => {
      this.taskPollLoop = setInterval(async () => {
        await this.pollTasks();
      }, 60000); // 60 seconds (was 10s — 10 personas × 10s = thundering herd)
    }, stagger);

    // Self-task generation on separate timer (OFF hot path)
    setTimeout(() => {
      this.selfTaskLoop = setInterval(async () => {
        await this.generateSelfTasksFromCNS();
      }, 60000); // 60 seconds (was 30s)
    }, stagger + 5000);

    // Training readiness checks (every 120 seconds)
    this.log(`🧬 ${this.personaUser.displayName}: Starting training readiness checks (every 120s)`);
    setTimeout(() => {
      this.trainingCheckLoop = setInterval(async () => {
        await this.checkTrainingReadiness();
      }, 120000); // 120 seconds (was 60s)
    }, stagger + 10000);
  }

  /**
   * Continuous service loop - runs until servicingLoopActive = false
   * Uses signal-based waiting (not polling) for instant response
   *
   * CPU Safety:
   * - Every iteration MUST block on inbox.waitForWork() (EventEmitter-based)
   * - Errors logged but loop continues (will block again on next waitForWork)
   * - Node.js event loop handles scheduling (no pthread primitives needed)
   */
  private async runServiceLoop(): Promise<void> {
    while (this.servicingLoopActive) {
      try {
        await this.serviceInbox();
      } catch (error) {
        this.log(`❌ ${this.personaUser.displayName}: Error in service loop: ${error}`);
        // Loop continues - next iteration will block on waitForWork() again
      }
    }
    this.log(`🛑 ${this.personaUser.displayName}: Service loop stopped`);
  }

  /**
   * PHASE 7.5.1: Check training readiness and trigger micro-tuning
   *
   * Called periodically (less frequently than serviceInbox) to check if any
   * domain buffers are ready for training. When threshold reached, automatically
   * triggers genome/train command for that domain.
   *
   * Delegates to PersonaTrainingManager module for actual execution.
   */
  private async checkTrainingReadiness(): Promise<void> {
    // Delegate to training manager module
    await this.personaUser.trainingManager.checkTrainingReadiness();
  }

  /**
   * Poll task database for pending tasks assigned to this persona
   * Convert TaskEntity → InboxTask and enqueue in inbox
   */
  private async pollTasks(): Promise<void> {
    try {
      // Query for pending tasks assigned to this persona
      const queryResult = await ORM.query<TaskEntity>({
        collection: COLLECTIONS.TASKS,
        filter: {
          assigneeId: this.personaUser.id,
          status: 'pending'
        },
        limit: 10 // Poll top 10 pending tasks
      });

      if (!queryResult.success || !queryResult.data || queryResult.data.length === 0) {
        return; // No pending tasks
      }

      // Convert each TaskEntity to InboxTask and enqueue
      for (const record of queryResult.data) {
        const task = record.data;

        // Convert to InboxTask using helper
        // Note: DataDaemon stores ID separately from data, so we need to inject it
        const inboxTask = taskEntityToInboxTask({
          ...task,
          id: record.id // Inject ID from record root
        });

        // Enqueue in inbox (unified priority queue)
        await this.personaUser.inbox.enqueue(inboxTask);

        this.log(`📋 ${this.personaUser.displayName}: Enqueued task ${task.taskType} (priority=${task.priority.toFixed(2)})`);
      }

      this.log(`✅ ${this.personaUser.displayName}: Polled ${queryResult.data.length} pending tasks`);

    } catch (error) {
      this.log(`❌ ${this.personaUser.displayName}: Error polling tasks: ${error}`);
    }
  }

  /**
   * CNS callback: Poll tasks from database
   *
   * Called by PersonaCentralNervousSystem.serviceCycle() via callback pattern.
   */
  async pollTasksFromCNS(): Promise<void> {
    await this.pollTasks();
  }

  /**
   * CNS callback: Generate self-tasks for autonomous work
   *
   * Called by PersonaCentralNervousSystem.serviceCycle() via callback pattern.
   */
  async generateSelfTasksFromCNS(): Promise<void> {
    try {
      const selfTasks = await this.personaUser.taskGenerator.generateSelfTasks();
      if (selfTasks.length > 0) {
        this.log(`🧠 ${this.personaUser.displayName}: Generated ${selfTasks.length} self-tasks`);

        // Persist each task to database and enqueue in inbox
        for (const task of selfTasks) {
          const storedTask = await ORM.store(COLLECTIONS.TASKS, task);
          if (storedTask) {
            // Convert to InboxTask and enqueue (use storedTask which has database ID)
            const inboxTask = taskEntityToInboxTask(storedTask);
            await this.personaUser.inbox.enqueue(inboxTask);
            this.log(`📋 ${this.personaUser.displayName}: Created self-task: ${task.description}`);
          } else {
            this.log(`❌ ${this.personaUser.displayName}: Failed to create self-task`);
          }
        }
      }
    } catch (error) {
      this.log(`❌ ${this.personaUser.displayName}: Error generating self-tasks: ${error}`);
    }
  }

  /**
   * CNS callback: Handle chat message from CNS orchestrator
   *
   * This is called by PersonaCentralNervousSystem.serviceChatDomain() via callback pattern.
   * Preserves existing message handling logic (evaluation, RAG, AI response, posting).
   */
  async handleChatMessageFromCNS(item: QueueItem, decision?: FastPathDecision): Promise<void> {
    const handlerStart = performance.now();

    // If this is a task, update status to 'in_progress' in database (prevents re-polling)
    if (item.type === 'task') {
      await ORM.update<TaskEntity>(
        COLLECTIONS.TASKS,
        item.taskId,
        { status: 'in_progress', startedAt: new Date() }
      );
    }

    // PHASE 6: Activate appropriate LoRA adapter based on domain
    if (item.domain) {
      const domainToAdapter: Record<string, string> = {
        'chat': 'conversational',
        'code': 'typescript-expertise',
        'self': 'self-improvement'
      };
      const adapterName = domainToAdapter[item.domain];
      if (adapterName) {
        await this.personaUser.memory.genome.activateSkill(adapterName);
      } else {
        // Unknown domain - default to conversational
        await this.personaUser.memory.genome.activateSkill('conversational');
      }
    }

    const setupMs = performance.now() - handlerStart;

    // Type-safe handling: Check if this is a message or task
    if (item.type === 'message') {
      // Convert InboxMessage → ProcessableMessage (typed, no `any`)
      const processable = inboxMessageToProcessable(item);
      const senderIsHuman = item.senderType === 'human';
      const messageText = item.content ?? '';

      // Process message using cognition-enhanced evaluation logic
      // Pass pre-computed decision from Rust serviceCycleFull (eliminates separate IPC call)
      const evalStart = performance.now();
      await this.personaUser.evaluateAndPossiblyRespondWithCognition(processable, senderIsHuman, messageText, decision);
      const evalMs = performance.now() - evalStart;

      // Update bookmark AFTER processing complete - enables true pause/resume
      // Shutdown mid-processing will re-query this message on restart
      await this.personaUser.updateMessageBookmark(item.roomId, item.timestamp, item.id);

      const totalMs = performance.now() - handlerStart;
      this.log(`[TIMING] ${this.personaUser.displayName}: handleChatMessage total=${totalMs.toFixed(1)}ms (setup=${setupMs.toFixed(1)}ms, eval=${evalMs.toFixed(1)}ms, hasDecision=${!!decision})`);
    } else if (item.type === 'task') {
      // PHASE 5: Task execution based on task type
      await this.executeTask(item);
    }

    // Update inbox load in state (affects mood calculation)
    this.personaUser.personaState.updateInboxLoad(this.personaUser.inbox.getSize());

    // Note: No cadence adjustment needed with signal-based waiting
    // Loop naturally adapts: fast when busy (instant signal), slow when idle (blocked on wait)
  }

  /**
   * PHASE 3: Service inbox (one iteration)
   *
   * Delegates to CNS orchestrator for intelligent scheduling and coordination.
   * CNS handles: priority selection, mood/energy checks, domain scheduling, coordination
   */
  private async serviceInbox(): Promise<void> {
    // Delegate to CNS orchestrator (capability-based multi-domain attention management)
    await this.personaUser.cns.serviceCycle();
  }

  /**
   * PHASE 5: Execute a task based on its type
   *
   * Handles all task types: memory-consolidation, skill-audit, fine-tune-lora, resume-work,
   * and code tasks (write-feature, review-code).
   * Delegates to PersonaTaskExecutor module for actual execution.
   */
  private async executeTask(task: InboxTask): Promise<void> {
    // For code-domain tasks, ensure workspace exists with room-aware mode
    if (task.domain === 'code') {
      const roomId = task.metadata?.roomId ?? task.contextId;
      const roomSlug = await this.resolveRoomSlug(roomId);
      await this.personaUser.ensureWorkspace({
        contextKey: roomSlug,
        mode: 'worktree',
        taskSlug: roomSlug,
      });
    }

    // Delegate to task executor module
    await this.personaUser.taskExecutor.executeTask(task);
  }

  /**
   * Resolve a room UUID to its uniqueId slug for workspace naming.
   * Falls back to truncated UUID if room lookup fails.
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
   * Stop autonomous servicing loops and cleanup
   */
  async stopServicing(): Promise<void> {
    // Stop service loop (signal-based while loop)
    this.servicingLoopActive = false;
    this.log(`🔄 ${this.personaUser.displayName}: Stopped autonomous servicing loop`);

    // Stop all interval-based loops
    if (this.taskPollLoop) {
      clearInterval(this.taskPollLoop);
      this.taskPollLoop = null;
    }
    if (this.selfTaskLoop) {
      clearInterval(this.selfTaskLoop);
      this.selfTaskLoop = null;
    }
    if (this.trainingCheckLoop) {
      clearInterval(this.trainingCheckLoop);
      this.trainingCheckLoop = null;
    }
    this.log(`🧬 ${this.personaUser.displayName}: Stopped all background loops`);
  }
}
