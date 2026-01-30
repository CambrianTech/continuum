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
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../shared/Constants';
import type { TaskEntity } from '../../../data/entities/TaskEntity';
import { taskEntityToInboxTask, inboxMessageToProcessable, type InboxTask, type QueueItem } from './QueueItemTypes';
import type { BaseQueueItem } from './channels/BaseQueueItem';
import { VoiceQueueItem } from './channels/VoiceQueueItem';
import { ChatQueueItem } from './channels/ChatQueueItem';
import { TaskQueueItem } from './channels/TaskQueueItem';
import type { ProcessableMessage } from './QueueItemTypes';

// Import PersonaUser directly - circular dependency is fine for type-only imports
import type { PersonaUser } from '../PersonaUser';

export class PersonaAutonomousLoop {
  private servicingLoopActive: boolean = false;
  private trainingCheckLoop: NodeJS.Timeout | null = null;
  private log: (message: string) => void;

  constructor(private readonly personaUser: PersonaUser, logger?: (message: string) => void) {
    this.log = logger || console.log.bind(console);
  }

  /**
   * PHASE 3: Start autonomous servicing loop
   *
   * Creates:
   * 1. Continuous async service loop (signal-based waiting, not polling)
   * 2. Training readiness check loop (every 60 seconds)
   *
   * Architecture:
   * - Loop uses signal/mutex pattern (RTOS-style, performant, no CPU spinning)
   * - CNS handles intelligence (priority, mood, coordination)
   * - Inbox provides EventEmitter-based signaling
   */
  startAutonomousServicing(): void {
    this.log(`🔄 ${this.personaUser.displayName}: Starting autonomous servicing (SIGNAL-BASED WAITING)`);

    // Create continuous async loop (not setInterval) - signal-based waiting
    this.servicingLoopActive = true;
    this.runServiceLoop().catch((error: any) => {
      this.log(`❌ ${this.personaUser.displayName}: Service loop crashed: ${error}`);
    });

    // PHASE 7.5.1: Create training check loop (every 60 seconds)
    // Checks less frequently than inbox servicing to avoid overhead
    this.log(`🧬 ${this.personaUser.displayName}: Starting training readiness checks (every 60s)`);
    this.trainingCheckLoop = setInterval(async () => {
      await this.checkTrainingReadiness();
    }, 60000); // 60 seconds
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
      const queryResult = await DataDaemon.query<TaskEntity>({
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
          const storedTask = await DataDaemon.store(COLLECTIONS.TASKS, task);
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
  async handleChatMessageFromCNS(item: QueueItem): Promise<void> {
    // If this is a task, update status to 'in_progress' in database (prevents re-polling)
    if (item.type === 'task') {
      await DataDaemon.update<TaskEntity>(
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

    // Type-safe handling: Check if this is a message or task
    if (item.type === 'message') {
      // Convert InboxMessage → ProcessableMessage (typed, no `any`)
      const processable = inboxMessageToProcessable(item);
      const senderIsHuman = item.senderType === 'human';
      const messageText = item.content ?? '';

      console.log(`🎙️🔊 VOICE-DEBUG [${this.personaUser.displayName}] CNS->handleChatMessageFromCNS: sourceModality=${processable.sourceModality}, voiceSessionId=${processable.voiceSessionId?.slice(0, 8) ?? 'none'}`);

      // Process message using cognition-enhanced evaluation logic
      await this.personaUser.evaluateAndPossiblyRespondWithCognition(processable, senderIsHuman, messageText);

      // Update bookmark AFTER processing complete - enables true pause/resume
      // Shutdown mid-processing will re-query this message on restart
      await this.personaUser.updateMessageBookmark(item.roomId, item.timestamp, item.id);
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
   * CNS callback: Handle a channel-routed queue item (new multi-channel path).
   *
   * Dispatches by itemType to the appropriate processing pipeline.
   * Items are BaseQueueItem subclasses (VoiceQueueItem, ChatQueueItem, TaskQueueItem).
   */
  async handleQueueItemFromCNS(item: BaseQueueItem): Promise<void> {
    // Activate LoRA adapter based on domain
    const domainToAdapter: Record<string, string> = {
      'chat': 'conversational',
      'audio': 'conversational',  // Voice uses same conversational adapter
      'code_review': 'typescript-expertise',
      'background': 'self-improvement',
    };
    const adapterName = domainToAdapter[item.domain] || 'conversational';
    await this.personaUser.memory.genome.activateSkill(adapterName);

    // Dispatch by concrete item type
    if (item instanceof VoiceQueueItem) {
      await this.handleVoiceItem(item);
    } else if (item instanceof ChatQueueItem) {
      await this.handleChatItem(item);
    } else if (item instanceof TaskQueueItem) {
      await this.handleTaskItem(item);
    } else {
      this.log(`⚠️ ${this.personaUser.displayName}: Unknown queue item type: ${item.itemType}`);
    }

    // Update inbox load in state (affects mood calculation)
    this.personaUser.personaState.updateInboxLoad(
      this.personaUser.inbox.getSize()
    );
  }

  /**
   * Handle a voice queue item — convert to ProcessableMessage with voice modality.
   */
  private async handleVoiceItem(item: VoiceQueueItem): Promise<void> {
    const processable: ProcessableMessage = {
      id: item.id,
      roomId: item.roomId,
      senderId: item.senderId,
      senderName: item.senderName,
      senderType: item.senderType,
      content: { text: item.content },
      timestamp: item.timestamp,
      sourceModality: 'voice',
      voiceSessionId: item.voiceSessionId,
    };

    const senderIsHuman = item.senderType === 'human';
    console.log(`🎙️ [${this.personaUser.displayName}] Channel: Processing VOICE item, voiceSessionId=${item.voiceSessionId.slice(0, 8)}`);

    await this.personaUser.evaluateAndPossiblyRespondWithCognition(
      processable, senderIsHuman, item.content
    );

    await this.personaUser.updateMessageBookmark(item.roomId, item.timestamp, item.id);
  }

  /**
   * Handle a chat queue item — convert to ProcessableMessage with text modality.
   * If consolidated, logs how many messages were merged.
   */
  private async handleChatItem(item: ChatQueueItem): Promise<void> {
    if (item.consolidatedCount > 1) {
      this.log(`📦 ${this.personaUser.displayName}: Processing consolidated chat (${item.consolidatedCount} messages from room ${item.roomId.slice(0, 8)})`);
    }

    const processable: ProcessableMessage = {
      id: item.id,
      roomId: item.roomId,
      senderId: item.senderId,
      senderName: item.senderName,
      senderType: item.senderType,
      content: { text: item.content },
      timestamp: item.timestamp,
      sourceModality: 'text',
    };

    const senderIsHuman = item.senderType === 'human';

    await this.personaUser.evaluateAndPossiblyRespondWithCognition(
      processable, senderIsHuman, item.content
    );

    await this.personaUser.updateMessageBookmark(item.roomId, item.timestamp, item.id);
  }

  /**
   * Handle a task queue item — update status and delegate to task executor.
   */
  private async handleTaskItem(item: TaskQueueItem): Promise<void> {
    // Mark as in_progress in database (prevents re-polling)
    await DataDaemon.update<TaskEntity>(
      COLLECTIONS.TASKS,
      item.taskId,
      { status: 'in_progress', startedAt: new Date() }
    );

    // Convert to InboxTask for backward compatibility with task executor
    const inboxTask: InboxTask = {
      id: item.id,
      type: 'task',
      taskId: item.taskId,
      assigneeId: item.assigneeId,
      createdBy: item.createdBy,
      domain: item.taskDomain,
      taskType: item.taskType,
      contextId: item.contextId,
      description: item.description,
      priority: item.basePriority,
      status: item.status,
      timestamp: item.timestamp,
      dueDate: item.dueDate,
      estimatedDuration: item.estimatedDuration,
      dependsOn: item.dependsOn,
      blockedBy: item.blockedBy,
      metadata: item.metadata as InboxTask['metadata'],
    };

    await this.executeTask(inboxTask);
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
   * Handles all task types: memory-consolidation, skill-audit, fine-tune-lora, resume-work, etc.
   * Delegates to PersonaTaskExecutor module for actual execution.
   */
  private async executeTask(task: InboxTask): Promise<void> {
    // Delegate to task executor module
    await this.personaUser.taskExecutor.executeTask(task);
  }

  /**
   * Stop autonomous servicing loops and cleanup
   */
  async stopServicing(): Promise<void> {
    // Stop service loop (signal-based while loop)
    this.servicingLoopActive = false;
    this.log(`🔄 ${this.personaUser.displayName}: Stopped autonomous servicing loop`);

    // Stop training check loop (interval-based)
    if (this.trainingCheckLoop) {
      clearInterval(this.trainingCheckLoop);
      this.trainingCheckLoop = null;
      this.log(`🧬 ${this.personaUser.displayName}: Stopped training readiness check loop`);
    }
  }
}
