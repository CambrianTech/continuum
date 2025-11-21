/**
 * PersonaTaskExecutor - Handles task execution for PersonaUser
 *
 * REFACTORING: Extracted from PersonaUser.ts (lines 2173-2343)
 * Pure function extraction - no behavioral changes
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../shared/Constants';
import type { InboxTask } from './PersonaInbox';
import type { TaskEntity, TaskStatus } from '../../../data/entities/TaskEntity';
import type { PersonaStateManager } from './PersonaState';
import type { PersonaMemory } from './cognitive/memory/PersonaMemory';

/**
 * PersonaTaskExecutor - Executes various task types for autonomous PersonaUsers
 *
 * Handles:
 * - memory-consolidation: Reviews recent activities
 * - skill-audit: Evaluates performance by domain
 * - resume-work: Continues stale tasks
 * - fine-tune-lora: Trains LoRA adapters
 */
export class PersonaTaskExecutor {
  constructor(
    private readonly personaId: UUID,
    private readonly displayName: string,
    private readonly memory: PersonaMemory,
    private readonly personaState: PersonaStateManager
  ) {}

  /**
   * Execute a task from the inbox
   *
   * Dispatches to specific handler based on task type,
   * updates database with completion status
   */
  async executeTask(task: InboxTask): Promise<void> {
    console.log(`🎯 ${this.displayName}: Executing task: ${task.taskType} - ${task.description}`);

    const startTime = Date.now();
    let outcome = '';
    let status: TaskStatus = 'completed';

    try {
      switch (task.taskType) {
        case 'memory-consolidation':
          outcome = await this.executeMemoryConsolidation(task);
          break;

        case 'skill-audit':
          outcome = await this.executeSkillAudit(task);
          break;

        case 'resume-work':
          outcome = await this.executeResumeWork(task);
          break;

        case 'fine-tune-lora':
          outcome = await this.executeFineTuneLora(task);
          break;

        default:
          outcome = `Unknown task type: ${task.taskType}`;
          status = 'failed';
          console.warn(`⚠️  ${this.displayName}: ${outcome}`);
      }

      console.log(`✅ ${this.displayName}: Task completed: ${task.taskType} - ${outcome}`);
    } catch (error) {
      status = 'failed';
      outcome = `Error executing task: ${error}`;
      console.error(`❌ ${this.displayName}: ${outcome}`);
    }

    // Update task in database with completion status
    const duration = Date.now() - startTime;
    await DataDaemon.update<TaskEntity>(
      COLLECTIONS.TASKS,
      task.taskId,
      {
        status,
        completedAt: new Date(),
        result: {
          success: status === 'completed',
          output: outcome,
          error: status === 'failed' ? outcome : undefined,
          metrics: {
            latencyMs: duration
          }
        }
      }
    );

    // Record activity in persona state (affects energy/mood)
    const complexity = task.priority; // Use priority as proxy for complexity
    await this.personaState.recordActivity(duration, complexity);
  }

  /**
   * PHASE 5: Memory consolidation task
   * Reviews recent activities and consolidates important memories
   */
  private async executeMemoryConsolidation(_task: InboxTask): Promise<string> {
    // TODO: Implement memory consolidation logic
    // For now, just log and return success
    console.log(`🧠 ${this.displayName}: Consolidating memories...`);

    // Query recent messages from rooms this persona is in
    const recentMessages = await DataDaemon.query({
      collection: COLLECTIONS.CHAT_MESSAGES,
      filter: {
        // Get messages from last hour
        timestamp: { $gte: Date.now() - 3600000 }
      },
      limit: 50
    });

    const messageCount = recentMessages.data?.length || 0;
    return `Reviewed ${messageCount} recent messages for memory consolidation`;
  }

  /**
   * PHASE 5: Skill audit task
   * Evaluates current capabilities and identifies areas for improvement
   */
  private async executeSkillAudit(_task: InboxTask): Promise<string> {
    // TODO: Implement skill audit logic
    console.log(`🔍 ${this.displayName}: Auditing skills...`);

    // Query recent tasks to evaluate performance by domain
    const recentTasks = await DataDaemon.query<TaskEntity>({
      collection: COLLECTIONS.TASKS,
      filter: {
        assigneeId: this.personaId,
        completedAt: { $gte: new Date(Date.now() - 21600000) } // Last 6 hours
      },
      limit: 100
    });

    const tasks = recentTasks.data || [];
    const domainStats: Record<string, { completed: number; failed: number }> = {};

    for (const record of tasks) {
      const t = record.data;
      if (!domainStats[t.domain]) {
        domainStats[t.domain] = { completed: 0, failed: 0 };
      }
      if (t.status === 'completed') domainStats[t.domain].completed++;
      if (t.status === 'failed') domainStats[t.domain].failed++;
    }

    const report = Object.entries(domainStats)
      .map(([domain, stats]) => `${domain}: ${stats.completed} completed, ${stats.failed} failed`)
      .join('; ');

    return `Skill audit complete - ${report || 'No recent tasks'}`;
  }

  /**
   * PHASE 5: Resume work task
   * Continues work on a previously started task that became stale
   */
  private async executeResumeWork(_task: InboxTask): Promise<string> {
    console.log(`♻️  ${this.displayName}: Resuming unfinished work...`);

    // TODO: Implement resume logic - query for stale in_progress tasks and re-enqueue them
    // For now, just acknowledge the task
    return 'Resume work task acknowledged - full implementation pending';
  }

  /**
   * PHASE 5: Fine-tune LoRA task
   * Trains a LoRA adapter on recent failure examples to improve performance
   */
  private async executeFineTuneLora(task: InboxTask): Promise<string> {
    console.log(`🧬 ${this.displayName}: Fine-tuning LoRA adapter...`);

    // Type-safe metadata validation (no type assertions)
    const loraLayer = task.metadata?.loraLayer;
    if (typeof loraLayer !== 'string') {
      return 'Missing or invalid LoRA layer in metadata';
    }

    // PHASE 6: Enable learning mode on the genome
    try {
      await this.memory.genome.enableLearningMode(loraLayer);
      console.log(`🧬 ${this.displayName}: Enabled learning mode for ${loraLayer} adapter`);

      // TODO (Phase 7): Implement actual fine-tuning logic
      // - Collect training examples from recent failures
      // - Format as LoRA training data
      // - Call Ollama fine-tuning API
      // - Save updated weights to disk

      // For now, just simulate training duration
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Disable learning mode after training
      await this.memory.genome.disableLearningMode(loraLayer);
      console.log(`🧬 ${this.displayName}: Disabled learning mode for ${loraLayer} adapter`);

      return `Fine-tuning complete for ${loraLayer} adapter (Phase 6 stub - actual training in Phase 7)`;
    } catch (error) {
      console.error(`❌ ${this.displayName}: Error during fine-tuning: ${error}`);
      return `Fine-tuning failed: ${error}`;
    }
  }
}
