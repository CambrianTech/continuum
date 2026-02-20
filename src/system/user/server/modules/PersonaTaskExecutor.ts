/**
 * PersonaTaskExecutor - Handles task execution for PersonaUser
 *
 * REFACTORING: Extracted from PersonaUser.ts (lines 2173-2343)
 * Pure function extraction - no behavioral changes
 */

import { type UUID, generateUUID } from '../../../core/types/CrossPlatformUUID';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '../../../shared/Constants';
import type { InboxTask } from './PersonaInbox';
import type { TaskEntity, TaskStatus } from '../../../data/entities/TaskEntity';
import type { PersonaStateManager } from './PersonaState';
import type { PersonaMemory } from './cognitive/memory/PersonaMemory';
import { RustEmbeddingClient } from '../../../core/services/RustEmbeddingClient';
import { MemoryEntity, MemoryType } from '../../../data/entities/MemoryEntity';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import { getFineTuningAdapter, supportsFineTuning } from '../../../genome/fine-tuning/server/FineTuningAdapterFactory';
import type {
  TrainingDataset,
  TrainingExample,
  LoRATrainingRequest,
  LoRATrainingResult
} from '../../../genome/fine-tuning/shared/FineTuningTypes';
import type { TraitType } from '../../../genome/entities/GenomeLayerEntity';
import { Commands } from '../../../core/shared/Commands';
import type { GenomeAcademySessionParams, GenomeAcademySessionResult } from '../../../../commands/genome/academy-session/shared/GenomeAcademySessionTypes';
import type { RustCognitionBridge } from './RustCognitionBridge';
import { CognitionLogger } from './cognition/CognitionLogger';

/**
 * Interface for PersonaUser dependency injection into task executor.
 * Provides access to genome reload and domain classifier sync.
 */
export interface PersonaUserForTaskExecutor {
  readonly rustCognitionBridge: RustCognitionBridge | null;
  readonly limbicSystem: {
    loadGenomeFromDatabase(): Promise<void>;
  };
}

/**
 * PersonaTaskExecutor - Executes various task types for autonomous PersonaUsers
 *
 * Handles:
 * - memory-consolidation: Reviews recent activities
 * - skill-audit: Evaluates performance by domain
 * - resume-work: Continues stale tasks
 * - fine-tune-lora: Trains LoRA adapters
 * - enroll-academy: Self-enroll in academy for detected skill gaps
 * - sentinel-complete/failed/escalation/approval: Sentinel lifecycle notifications
 */
export class PersonaTaskExecutor {
  private log: (message: string) => void;
  private personaUser?: PersonaUserForTaskExecutor;

  constructor(
    private readonly personaId: UUID,
    private readonly displayName: string,
    private readonly memory: PersonaMemory,
    private readonly personaState: PersonaStateManager,
    private readonly provider: string = 'candle',
    logger: (message: string) => void
  ) {
    this.log = logger;
  }

  /**
   * Set PersonaUser reference for features that need genome/classifier access.
   * Called after PersonaUser is fully initialized.
   */
  setPersonaUser(personaUser: PersonaUserForTaskExecutor): void {
    this.personaUser = personaUser;
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

        case 'enroll-academy':
          outcome = await this.executeEnrollAcademy(task);
          break;

        case 'sentinel-complete':
        case 'sentinel-failed':
        case 'sentinel-escalation':
        case 'sentinel-approval':
          outcome = await this.executeSentinelTask(task);
          break;

        case 'write-feature':
        case 'review-code':
          outcome = await this.executeCodeTask(task);
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
    try {
      await ORM.update<TaskEntity>(
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
    } catch {
      // Task was deleted between dequeue and completion — work was still done, just can't record it
      this.log(`⚠️ ${this.displayName}: Task ${task.taskId.slice(0, 8)} vanished during execution (deleted externally?)`);
    }

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
    const recentMessages = await ORM.query<ChatMessageEntity>({
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

      // Generate embedding for semantic search via Rust worker (fast, ~5ms)
      let embedding: number[] | null = null;
      try {
        const client = RustEmbeddingClient.instance;
        if (await client.isAvailable()) {
          embedding = await client.embed(text);
          if (embedding) embeddingsGenerated++;
        } else {
          this.log(`⚠️ ${this.displayName}: Rust embedding worker not available`);
        }
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

      // 5. Store to persona's longterm.db (not shared database)
      try {
        const memoryDbHandle = CognitionLogger.getDbHandle(this.personaId);
        await Commands.execute('data/create', {
          dbHandle: memoryDbHandle,
          collection: COLLECTIONS.MEMORIES,
          data: memory,
        } as any);
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
    const recentTasks = await ORM.query<TaskEntity>({
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

          await ORM.store(COLLECTIONS.TASKS, improvementTask as TaskEntity);
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

    const staleTasks = await ORM.query<TaskEntity>({
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
        await ORM.update<TaskEntity>(COLLECTIONS.TASKS, record.id, {
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
   * Fine-tune LoRA adapter using recent chat interactions
   *
   * Collects training examples from this persona's successful responses,
   * trains a LoRA adapter via llama.cpp, and registers it with the genome.
   *
   * @param task - Inbox task with metadata.loraLayer specifying adapter name
   */
  private async executeFineTuneLora(task: InboxTask): Promise<string> {
    this.log(`🧬 ${this.displayName}: Fine-tuning LoRA adapter...`);

    // Validate metadata
    const loraLayer = task.metadata?.loraLayer;
    if (typeof loraLayer !== 'string') {
      return 'Missing or invalid LoRA layer in metadata';
    }

    try {
      // 1. Try to enable learning mode on existing adapter (optional for first-time training)
      const adapterExists = this.memory.genome.hasAdapter(loraLayer);
      if (adapterExists) {
        await this.memory.genome.enableLearningMode(loraLayer);
        this.log(`🧬 ${this.displayName}: Enabled learning mode for ${loraLayer} adapter`);
      } else {
        this.log(`🧬 ${this.displayName}: First-time training for ${loraLayer} - will create new adapter`);
      }

      // 2. Get training data - prefer pre-collected examples from signal-driven training
      let trainingData: TrainingDataset;
      const preCollectedExamples = task.metadata?.trainingData as Array<{ prompt: string; completion: string; isPositive: boolean }> | undefined;

      if (preCollectedExamples && Array.isArray(preCollectedExamples) && preCollectedExamples.length > 0) {
        // Signal-driven training: convert pre-collected examples to TrainingDataset format
        this.log(`🧬 ${this.displayName}: Using ${preCollectedExamples.length} pre-collected signal-driven examples`);

        trainingData = {
          examples: preCollectedExamples.map(ex => ({
            messages: [
              // For positive examples: system context + user prompt + original AI response
              // For negative examples: system context + user prompt + corrected response
              { role: 'user' as const, content: ex.prompt },
              { role: 'assistant' as const, content: ex.completion }
            ],
            metadata: {
              timestamp: Date.now(),
              confidence: ex.isPositive ? 0.9 : 0.7  // Higher confidence for positive reinforcement
            }
          })),
          metadata: {
            personaId: this.personaId,
            personaName: this.displayName,
            traitType: loraLayer as TraitType,
            createdAt: Date.now(),
            source: 'corrections',  // Signal-driven examples are corrections/approvals
            totalExamples: preCollectedExamples.length
          }
        };
      } else {
        // Legacy path: collect training examples from recent chat interactions
        trainingData = await this.collectTrainingExamples(loraLayer);
      }

      if (trainingData.examples.length === 0) {
        await this.memory.genome.disableLearningMode(loraLayer);
        return `No training examples found for ${loraLayer} - skipping fine-tuning`;
      }

      this.log(`🧬 ${this.displayName}: Collected ${trainingData.examples.length} training examples`);

      // 3. Build training request
      const baseModel = this.memory.genome.getState().baseModel || 'llama3.2:3b';
      const trainingRequest: LoRATrainingRequest = {
        personaId: this.personaId,
        personaName: this.displayName,
        traitType: loraLayer as TraitType,
        baseModel,
        dataset: trainingData,
        // LoRA hyperparameters (sensible defaults)
        rank: 16,
        alpha: 32,
        epochs: 3,
        learningRate: 0.0001,
        batchSize: 4
      };

      // 4. Get the appropriate fine-tuning adapter
      // PEFT is preferred for local training (candle, local) as it:
      // - Supports any HuggingFace model
      // - Enables multi-adapter composition (genome vision)
      // - Works cross-platform (MPS/CUDA/CPU)
      const localProviders = ['candle', 'local', 'peft'];
      const effectiveProvider = localProviders.includes(this.provider.toLowerCase()) ? 'peft' : this.provider;
      const adapter = getFineTuningAdapter(effectiveProvider);

      if (!adapter) {
        this.log(`⚠️ ${this.displayName}: Provider '${effectiveProvider}' (from ${this.provider}) does not support fine-tuning`);
        return `Fine-tuning not supported for provider: ${this.provider}`;
      }

      this.log(`🧬 ${this.displayName}: Starting fine-tuning via ${effectiveProvider} adapter for ${loraLayer}...`);
      const result: LoRATrainingResult = await adapter.trainLoRA(trainingRequest);

      // 5. Disable learning mode if adapter was already loaded
      if (adapterExists) {
        await this.memory.genome.disableLearningMode(loraLayer);
      }

      // 6. Register trained adapter with genome if successful
      if (result.success && result.modelPath) {
        this.memory.genome.registerAdapter({
          name: loraLayer,
          domain: loraLayer,
          path: result.modelPath,
          sizeMB: 50, // Estimate - actual size varies
          priority: 0.5,
        });

        this.log(`✅ ${this.displayName}: LoRA training complete! Adapter saved: ${result.modelPath}`);
        return `Fine-tuning complete for ${loraLayer}: ${result.metrics?.examplesProcessed || 0} examples, loss=${result.metrics?.finalLoss?.toFixed(4) || 'N/A'}`;
      } else {
        this.log(`❌ ${this.displayName}: LoRA training failed: ${result.error}`);
        return `Fine-tuning failed for ${loraLayer}: ${result.error}`;
      }
    } catch (error) {
      // Try to disable learning mode if it was enabled
      if (this.memory.genome.hasAdapter(loraLayer)) {
        await this.memory.genome.disableLearningMode(loraLayer).catch(() => {});
      }
      const errorMsg = error instanceof Error ? error.message : String(error);
      this.log(`❌ ${this.displayName}: Error during fine-tuning: ${errorMsg}`);
      return `Fine-tuning failed: ${errorMsg}`;
    }
  }

  /**
   * Code task execution (write-feature, review-code)
   *
   * Infrastructure hook for code-domain tasks. The workspace is guaranteed to exist
   * by the time this runs (PersonaAutonomousLoop.ensureWorkspace called beforehand).
   *
   * The actual coding agent loop (read→reason→edit→verify→commit) is driven by the
   * persona's tool execution pipeline with code/* tools — not by this method.
   * This method logs the task and returns, allowing the recipe pipeline to handle execution.
   */
  private async executeCodeTask(task: InboxTask): Promise<string> {
    this.log(`💻 ${this.displayName}: Code task received — ${task.taskType}: ${task.description}`);

    const roomId = task.metadata?.roomId ?? task.contextId;
    this.log(`💻 ${this.displayName}: Code task for room=${roomId}, workspace ensured by caller`);

    return `Code task acknowledged: ${task.taskType} — ${task.description}`;
  }

  /**
   * Collect training examples from recent chat interactions
   *
   * Queries chat messages where this persona responded and converts them
   * to training format (system/user/assistant message sequences).
   */
  private async collectTrainingExamples(domain: string): Promise<TrainingDataset> {
    const examples: TrainingExample[] = [];

    // Query recent messages in the last 24 hours
    const oneDayAgo = new Date(Date.now() - 24 * 60 * 60 * 1000);

    const messagesResult = await ORM.query<ChatMessageEntity>({
      collection: COLLECTIONS.CHAT_MESSAGES,
      filter: {
        senderId: this.personaId,
        createdAt: { $gte: oneDayAgo }
      },
      sort: [{ field: 'createdAt', direction: 'desc' }],
      limit: 100
    });

    const myMessages = messagesResult.data || [];
    this.log(`🧬 ${this.displayName}: Found ${myMessages.length} recent messages from this persona`);

    // Group messages by room and find preceding user messages
    for (const record of myMessages) {
      const myResponse = record.data;
      const responseText = typeof myResponse.content === 'string'
        ? myResponse.content
        : myResponse.content?.text || '';

      if (!responseText || responseText.length < 20) {
        continue; // Skip very short responses
      }

      // Find the message this was responding to
      const precedingResult = await ORM.query<ChatMessageEntity>({
        collection: COLLECTIONS.CHAT_MESSAGES,
        filter: {
          roomId: myResponse.roomId,
          createdAt: { $lt: myResponse.createdAt }
        },
        sort: [{ field: 'createdAt', direction: 'desc' }],
        limit: 1
      });

      const precedingMsg = precedingResult.data?.[0]?.data;
      if (!precedingMsg) {
        continue; // No preceding message found
      }

      const userText = typeof precedingMsg.content === 'string'
        ? precedingMsg.content
        : precedingMsg.content?.text || '';

      if (!userText || userText.length < 5) {
        continue; // Skip if user message too short
      }

      // Create training example: user message -> assistant response
      const example: TrainingExample = {
        messages: [
          {
            role: 'system',
            content: `You are ${this.displayName}, a helpful AI assistant.`
          },
          {
            role: 'user',
            content: userText
          },
          {
            role: 'assistant',
            content: responseText
          }
        ],
        metadata: {
          timestamp: myResponse.createdAt ? new Date(myResponse.createdAt).getTime() : Date.now(),
          roomId: myResponse.roomId
        }
      };

      examples.push(example);
    }

    return {
      examples,
      metadata: {
        personaId: this.personaId,
        personaName: this.displayName,
        traitType: domain as TraitType,
        createdAt: Date.now(),
        source: 'conversations',
        totalExamples: examples.length
      }
    };
  }

  /**
   * Enroll in academy session for a detected skill gap.
   * Triggered by SelfTaskGenerator when a domain has activity but no adapter.
   */
  private async executeEnrollAcademy(task: InboxTask): Promise<string> {
    const domain = (task.metadata?.domain as string) ?? task.description;
    const suggestedMode = (task.metadata?.suggested_mode as string) ?? 'knowledge';

    this.log(`🎓 ${this.displayName}: Enrolling in academy for skill gap: ${domain} (mode=${suggestedMode})`);

    // Check: no concurrent academy session already running
    try {
      const existing = await ORM.query<TaskEntity>({
        collection: COLLECTIONS.TASKS,
        filter: {
          assigneeId: this.personaId,
          taskType: 'enroll-academy',
          status: 'in_progress',
        },
        sort: [{ field: 'createdAt', direction: 'desc' }],
        limit: 1,
      });

      if (existing.data && existing.data.length > 0) {
        return `Skipped: academy session already in progress for this persona`;
      }
    } catch {
      // Query failure is non-fatal — proceed with enrollment
    }

    // Determine academy mode
    const mode = suggestedMode === 'coding' || suggestedMode === 'project' || suggestedMode === 'knowledge'
      ? suggestedMode
      : 'knowledge';

    try {
      const result = await Commands.execute<GenomeAcademySessionParams, GenomeAcademySessionResult>(
        'genome/academy-session',
        {
          personaId: this.personaId,
          personaName: this.displayName,
          skill: domain,
          mode: mode as 'knowledge' | 'coding' | 'project',
        }
      );

      const sessionId = result?.academySessionId ?? 'unknown';
      return `Enrolled in academy: ${domain} (mode=${mode}, session=${sessionId})`;
    } catch (error) {
      return `Academy enrollment failed for ${domain}: ${error}`;
    }
  }

  /**
   * Handle sentinel lifecycle tasks (escalated from SentinelEscalationService)
   *
   * When a sentinel completes, fails, or needs approval, the persona processes
   * the notification. This enables the persona to:
   * - Acknowledge completion ("my training sentinel finished")
   * - React to failures ("the build sentinel failed, should I retry?")
   * - Recall similar past sentinel patterns for learning
   */
  private async executeSentinelTask(task: InboxTask): Promise<string> {
    const metadata = task.metadata ?? {};
    const sentinelName = metadata.sentinelName ?? 'unknown';
    const sentinelStatus = metadata.sentinelStatus ?? task.taskType;
    const error = metadata.error as string | undefined;

    this.log(`🤖 ${this.displayName}: Sentinel notification — "${sentinelName}" ${sentinelStatus}`);

    // Recall similar sentinel memories for context
    const relevantMemories = await this.recallSentinelPatterns(sentinelName);
    if (relevantMemories.length > 0) {
      this.log(`🧠 ${this.displayName}: Recalled ${relevantMemories.length} similar sentinel executions`);
    }

    switch (task.taskType) {
      case 'sentinel-complete': {
        // If this was an academy session, reload genome to activate new adapters
        const isAcademySentinel = typeof sentinelName === 'string' &&
          (sentinelName.includes('academy') || sentinelName.includes('student') || sentinelName.includes('learning'));
        if (isAcademySentinel && this.personaUser) {
          this.log(`🧬 ${this.displayName}: Academy sentinel completed — reloading genome to activate new adapters`);
          try {
            await this.personaUser.limbicSystem.loadGenomeFromDatabase();
            // Sync domain classifier with new adapters
            if (this.personaUser.rustCognitionBridge) {
              await this.personaUser.rustCognitionBridge.syncDomainClassifier();
            }
            this.log(`✅ ${this.displayName}: Genome reloaded and domain classifier synced after academy completion`);
          } catch (error) {
            this.log(`⚠️ ${this.displayName}: Post-academy genome reload failed: ${error}`);
          }
        }
        return `Sentinel "${sentinelName}" completed successfully. ` +
          (isAcademySentinel ? 'Genome reloaded with new adapters. ' : '') +
          (relevantMemories.length > 0
            ? `This is execution #${relevantMemories.length + 1} of similar sentinels.`
            : 'First execution of this sentinel type.');
      }

      case 'sentinel-failed':
        return `Sentinel "${sentinelName}" failed: ${error ?? 'unknown error'}. ` +
          (relevantMemories.length > 0
            ? `${relevantMemories.filter(m => m.context?.status === 'failed').length} previous failures recorded.`
            : 'No prior execution history.');

      case 'sentinel-escalation':
        return `Sentinel "${sentinelName}" requires attention: ${task.description}`;

      case 'sentinel-approval':
        return `Sentinel "${sentinelName}" awaiting approval: ${task.description}`;

      default:
        return `Sentinel task: ${task.description}`;
    }
  }

  /**
   * Recall sentinel memories relevant to a given sentinel name or pattern.
   *
   * Queries the global memories collection for type='sentinel' memories
   * belonging to this persona, filtered by sentinel name tags.
   * Returns most recent first, limited to 10.
   */
  async recallSentinelPatterns(sentinelName?: string): Promise<Array<{
    content: string;
    context: Record<string, any>;
    importance: number;
    timestamp: any;
  }>> {
    try {
      const filter: Record<string, any> = {
        personaId: this.personaId,
        type: 'sentinel',
      };

      const dbHandle = CognitionLogger.getDbHandle(this.personaId);
      const result = await Commands.execute('data/list', {
        dbHandle,
        collection: 'memories',
        filter,
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: 10,
      } as any) as any;

      const memories = (result?.items ?? []) as Array<{
        content: string;
        context: Record<string, any>;
        importance: number;
        timestamp: any;
        tags: string[];
      }>;

      // If a sentinel name is given, prioritize matching memories
      if (sentinelName && memories.length > 0) {
        const nameMatches = memories.filter(m =>
          m.tags?.includes(sentinelName) ||
          m.context?.sentinelName === sentinelName
        );
        if (nameMatches.length > 0) return nameMatches;
      }

      return memories;
    } catch (err) {
      this.log(`⚠️ ${this.displayName}: Failed to recall sentinel patterns: ${err}`);
      return [];
    }
  }
}
