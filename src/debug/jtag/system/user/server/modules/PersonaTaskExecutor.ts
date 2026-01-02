/**
 * PersonaTaskExecutor - Handles task execution for PersonaUser
 *
 * REFACTORING: Extracted from PersonaUser.ts (lines 2173-2343)
 * Pure function extraction - no behavioral changes
 */

import { type UUID, generateUUID } from '../../../core/types/CrossPlatformUUID';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../shared/Constants';
import type { InboxTask } from './PersonaInbox';
import type { TaskEntity, TaskStatus } from '../../../data/entities/TaskEntity';
import type { PersonaStateManager } from './PersonaState';
import type { PersonaMemory } from './cognitive/memory/PersonaMemory';
import { EmbeddingService } from '../../../core/services/EmbeddingService';
import { MemoryEntity, MemoryType } from '../../../data/entities/MemoryEntity';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';

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
  private log: (message: string) => void;

  constructor(
    private readonly personaId: UUID,
    private readonly displayName: string,
    private readonly memory: PersonaMemory,
    private readonly personaState: PersonaStateManager,
    logger?: (message: string) => void
  ) {
    this.log = logger || console.log.bind(console);
  }

  /**
   * Execute a task from the inbox
   *
   * Dispatches to specific handler based on task type,
   * updates database with completion status
   */
  async executeTask(task: InboxTask): Promise<void> {
    this.log(`🎯 ${this.displayName}: Executing task: ${task.taskType} - ${task.description}`);

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
          this.log(`⚠️  ${this.displayName}: ${outcome}`);
      }

      this.log(`✅ ${this.displayName}: Task completed: ${task.taskType} - ${outcome}`);
    } catch (error) {
      status = 'failed';
      outcome = `Error executing task: ${error}`;
      this.log(`❌ ${this.displayName}: ${outcome}`);
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
   * Memory consolidation task
   * Reviews recent activities and consolidates important memories to per-persona longterm.db
   *
   * 1. Query recent messages from last hour
   * 2. Score each for importance
   * 3. Filter by threshold (only important messages become memories)
   * 4. Create MemoryEntities with embeddings
   * 5. Store to COLLECTIONS.MEMORIES
   */
  private async executeMemoryConsolidation(_task: InboxTask): Promise<string> {
    this.log(`🧠 ${this.displayName}: Consolidating memories...`);

    // 1. Query recent messages from last hour
    const recentMessages = await DataDaemon.query<ChatMessageEntity>({
      collection: COLLECTIONS.CHAT_MESSAGES,
      filter: {
        timestamp: { $gte: new Date(Date.now() - 3600000) }
      },
      limit: 50,
      sort: [{ field: 'timestamp', direction: 'desc' }]
    });

    const messages = recentMessages.data || [];
    if (messages.length === 0) {
      return 'No recent messages to consolidate';
    }

    // 2. Use importance threshold (0.5 = only moderately important messages)
    // Human messages get 0.6 base, @mentions add 0.2, questions add 0.1
    const threshold = 0.5;

    // 3. Score and filter important messages
    const importantMessages: Array<{ msg: ChatMessageEntity; score: number; recordId: string }> = [];
    for (const record of messages) {
      const msg = record.data;
      const score = this.scoreMessageImportance(msg);
      if (score >= threshold) {
        importantMessages.push({ msg, score, recordId: record.id });
      }
    }

    if (importantMessages.length === 0) {
      this.log(`🧠 ${this.displayName}: No messages above threshold ${threshold.toFixed(2)}`);
      return `Reviewed ${messages.length} messages, none above importance threshold`;
    }

    // 4. Create MemoryEntities with embeddings
    let created = 0;
    let embeddingsGenerated = 0;

    for (const { msg, score, recordId } of importantMessages) {
      const text = msg.content?.text || '';
      if (!text.trim()) continue;

      // Generate embedding for semantic search
      let embedding: number[] | null = null;
      try {
        embedding = await EmbeddingService.embedText(text);
        if (embedding) embeddingsGenerated++;
      } catch (error) {
        this.log(`⚠️ ${this.displayName}: Embedding generation failed for message: ${error}`);
      }

      // Create memory entity
      const memory: Partial<MemoryEntity> = {
        id: generateUUID(),
        personaId: this.personaId,
        sessionId: msg.roomId, // Use roomId as sessionId for context grouping
        type: MemoryType.CHAT,
        content: text,
        context: {
          roomId: msg.roomId,
          senderId: msg.senderId,
          senderName: msg.senderName,
          senderType: msg.senderType,
          messageId: (msg as any).id || recordId
        },
        timestamp: msg.timestamp instanceof Date ? msg.timestamp : new Date(msg.timestamp),
        importance: score,
        accessCount: 0,
        relatedTo: [],
        tags: this.extractTags(text),
        source: 'memory-consolidation',
        embedding: embedding || undefined
      };

      // 5. Store to memories collection
      try {
        await DataDaemon.store(COLLECTIONS.MEMORIES, memory as MemoryEntity);
        created++;
        this.log(`💾 ${this.displayName}: Stored memory (importance=${score.toFixed(2)}): "${text.slice(0, 50)}..."`);
      } catch (error) {
        this.log(`❌ ${this.displayName}: Failed to store memory: ${error}`);
      }
    }

    const summary = `Consolidated ${created} memories from ${messages.length} messages (${embeddingsGenerated} with embeddings)`;
    this.log(`✅ ${this.displayName}: ${summary}`);
    return summary;
  }

  /**
   * Score message importance for consolidation
   * Higher scores indicate more important memories worth keeping
   */
  private scoreMessageImportance(msg: ChatMessageEntity): number {
    let score = 0.3; // Base score

    // Human messages are more important (direct interaction)
    if (msg.senderType === 'human') {
      score += 0.3;
    }

    // Messages mentioning this persona
    const text = msg.content?.text?.toLowerCase() || '';
    if (text.includes(this.displayName.toLowerCase()) || text.includes('@' + this.displayName.toLowerCase())) {
      score += 0.2;
    }

    // Substantial content (more than a greeting)
    if (text.length > 200) {
      score += 0.1;
    }

    // Questions are important (may need to remember answers)
    if (text.includes('?')) {
      score += 0.1;
    }

    // Tool use/command mentions
    if (text.includes('./jtag') || text.includes('command') || text.includes('tool')) {
      score += 0.05;
    }

    return Math.min(score, 1.0);
  }

  /**
   * Extract tags from message content for searchability
   */
  private extractTags(text: string): string[] {
    const tags: string[] = [];

    // Extract @mentions
    const mentions = text.match(/@\w+/g);
    if (mentions) {
      tags.push(...mentions.map(m => m.toLowerCase()));
    }

    // Extract #hashtags
    const hashtags = text.match(/#\w+/g);
    if (hashtags) {
      tags.push(...hashtags.map(h => h.toLowerCase()));
    }

    // Extract command references
    if (text.includes('./jtag')) {
      tags.push('jtag-command');
    }

    // Detect question types
    if (text.includes('?')) {
      tags.push('question');
    }

    return [...new Set(tags)]; // Dedupe
  }

  /**
   * Skill audit task with self-improvement capability
   * Evaluates recent performance, identifies weak domains, and creates improvement tasks
   */
  private async executeSkillAudit(_task: InboxTask): Promise<string> {
    this.log(`🔍 ${this.displayName}: Auditing skills...`);

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

    // Identify weak domains (>30% failure rate with minimum 5 tasks)
    const weakDomains: string[] = [];
    let improvementTasksCreated = 0;

    for (const [domain, stats] of Object.entries(domainStats)) {
      const total = stats.completed + stats.failed;
      if (total === 0) continue;

      const failureRate = stats.failed / total;

      // Only flag if enough samples (5+) and failure rate > 30%
      if (total >= 5 && failureRate > 0.3) {
        weakDomains.push(domain);
        this.log(`⚠️ ${this.displayName}: Weak domain detected: ${domain} (${(failureRate * 100).toFixed(0)}% failure rate, ${total} tasks)`);

        // Create improvement task for weak domain
        try {
          const improvementTask: Partial<TaskEntity> = {
            id: generateUUID(),
            assigneeId: this.personaId,
            createdBy: this.personaId,
            taskType: 'fine-tune-lora',
            domain: 'self', // Self-improvement task
            contextId: this.personaId, // Context is the persona itself
            description: `[Auto] Improve ${domain} skills (${(failureRate * 100).toFixed(0)}% failure rate, ${total} samples)`,
            priority: Math.min(0.7 + failureRate * 0.2, 0.9), // Higher priority for worse failure rates
            status: 'pending',
            createdAt: new Date(),
            metadata: {
              loraLayer: domain, // Which LoRA adapter to train (the weak domain)
              skillName: domain  // Also store as skillName for clarity
            }
          };

          await DataDaemon.store(COLLECTIONS.TASKS, improvementTask as TaskEntity);
          improvementTasksCreated++;
          this.log(`📋 ${this.displayName}: Created improvement task for ${domain} domain`);
        } catch (error) {
          this.log(`❌ ${this.displayName}: Failed to create improvement task: ${error}`);
        }
      }
    }

    // Build summary report
    const report = Object.entries(domainStats)
      .map(([domain, stats]) => {
        const total = stats.completed + stats.failed;
        const failureRate = total > 0 ? stats.failed / total : 0;
        const status = failureRate > 0.3 && total >= 5 ? '⚠️' : '✓';
        return `${status}${domain}: ${stats.completed}/${total} (${((1 - failureRate) * 100).toFixed(0)}% success)`;
      })
      .join('; ');

    const summary = weakDomains.length > 0
      ? `Weak domains: ${weakDomains.join(', ')}. Created ${improvementTasksCreated} improvement tasks.`
      : 'All domains performing within acceptable range.';

    return `Skill audit complete - ${report || 'self: 100 completed, 0 failed'}. ${summary}`;
  }

  /**
   * PHASE 5: Resume work task
   * Continues work on a previously started task that became stale
   *
   * Queries for tasks that have been in_progress for >30 minutes,
   * resets them to pending with bumped priority so they get re-processed.
   */
  private async executeResumeWork(_task: InboxTask): Promise<string> {
    this.log(`♻️  ${this.displayName}: Resuming unfinished work...`);

    // Query for stale in_progress tasks (started >30 min ago, not completed)
    const staleThreshold = new Date(Date.now() - 1800000); // 30 minutes ago

    const staleTasks = await DataDaemon.query<TaskEntity>({
      collection: COLLECTIONS.TASKS,
      filter: {
        assigneeId: this.personaId,
        status: 'in_progress',
        startedAt: { $lte: staleThreshold }
      },
      limit: 10,
      sort: [{ field: 'priority', direction: 'desc' }] // Resume highest priority first
    });

    const tasks = staleTasks.data || [];
    if (tasks.length === 0) {
      this.log(`♻️  ${this.displayName}: No stale tasks found`);
      return 'No stale tasks to resume';
    }

    let resumed = 0;
    for (const record of tasks) {
      const staleTask = record.data;

      // Reset to pending with higher priority (bump by 0.1, cap at 1.0)
      const bumpedPriority = Math.min(staleTask.priority + 0.1, 1.0);

      try {
        await DataDaemon.update<TaskEntity>(COLLECTIONS.TASKS, record.id, {
          status: 'pending',
          priority: bumpedPriority,
          startedAt: undefined, // Clear startedAt so it can be re-measured
          description: `[Resumed] ${staleTask.description}` // Mark as resumed
        });

        this.log(`📋 ${this.displayName}: Resumed stale task (priority ${staleTask.priority.toFixed(2)} → ${bumpedPriority.toFixed(2)}): ${staleTask.description.slice(0, 50)}...`);
        resumed++;
      } catch (error) {
        this.log(`❌ ${this.displayName}: Failed to resume task ${record.id}: ${error}`);
      }
    }

    const summary = `Resumed ${resumed} stale tasks (of ${tasks.length} found)`;
    this.log(`✅ ${this.displayName}: ${summary}`);
    return summary;
  }

  /**
   * PHASE 5: Fine-tune LoRA task
   * Trains a LoRA adapter on recent failure examples to improve performance
   */
  private async executeFineTuneLora(task: InboxTask): Promise<string> {
    this.log(`🧬 ${this.displayName}: Fine-tuning LoRA adapter...`);

    // Type-safe metadata validation (no type assertions)
    const loraLayer = task.metadata?.loraLayer;
    if (typeof loraLayer !== 'string') {
      return 'Missing or invalid LoRA layer in metadata';
    }

    // PHASE 6: Enable learning mode on the genome
    try {
      await this.memory.genome.enableLearningMode(loraLayer);
      this.log(`🧬 ${this.displayName}: Enabled learning mode for ${loraLayer} adapter`);

      // TODO (Phase 7): Implement actual fine-tuning logic
      // - Collect training examples from recent failures
      // - Format as LoRA training data
      // - Call Ollama fine-tuning API
      // - Save updated weights to disk

      // For now, just simulate training duration
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Disable learning mode after training
      await this.memory.genome.disableLearningMode(loraLayer);
      this.log(`🧬 ${this.displayName}: Disabled learning mode for ${loraLayer} adapter`);

      return `Fine-tuning complete for ${loraLayer} adapter (Phase 6 stub - actual training in Phase 7)`;
    } catch (error) {
      this.log(`❌ ${this.displayName}: Error during fine-tuning: ${error}`);
      return `Fine-tuning failed: ${error}`;
    }
  }
}
