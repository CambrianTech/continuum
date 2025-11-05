/**
 * SelfTaskGenerator - Autonomous task creation for PersonaUser
 *
 * Philosophy: AIs create their own work for self-improvement
 * - Memory consolidation (every hour)
 * - Skill audits (every 6 hours)
 * - Resume unfinished work
 * - Continuous learning from mistakes
 *
 * This is the KEY to AI autonomy - not just reacting to external events,
 * but proactively creating self-improvement tasks.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { TaskEntity } from '../../../data/entities/TaskEntity';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../data/config/DatabaseConfig';

export interface SelfTaskGeneratorConfig {
  /**
   * How often to review memory (ms)
   * Default: 1 hour
   */
  memoryReviewInterval: number;

  /**
   * How often to audit skills (ms)
   * Default: 6 hours
   */
  skillAuditInterval: number;

  /**
   * Minimum time since last activity to consider work "unfinished" (ms)
   * Default: 30 minutes
   */
  unfinishedWorkThreshold: number;

  /**
   * Enable self-task generation
   * Default: true
   */
  enabled: boolean;
}

const DEFAULT_CONFIG: SelfTaskGeneratorConfig = {
  memoryReviewInterval: 3600000,      // 1 hour
  skillAuditInterval: 21600000,       // 6 hours
  unfinishedWorkThreshold: 1800000,   // 30 minutes
  enabled: true
};

export class SelfTaskGenerator {
  private personaId: UUID;
  private displayName: string;
  private config: SelfTaskGeneratorConfig;

  // Track when we last created each task type
  private lastMemoryReview: number = 0;
  private lastSkillAudit: number = 0;

  constructor(personaId: UUID, displayName: string, config?: Partial<SelfTaskGeneratorConfig>) {
    this.personaId = personaId;
    this.displayName = displayName;
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  /**
   * Generate self-tasks based on current state
   * Called periodically by PersonaUser.serviceInbox()
   *
   * Returns array of tasks that should be created
   */
  async generateSelfTasks(): Promise<TaskEntity[]> {
    if (!this.config.enabled) {
      return [];
    }

    const tasks: TaskEntity[] = [];
    const now = Date.now();

    // 1. Memory consolidation (every hour)
    if (now - this.lastMemoryReview > this.config.memoryReviewInterval) {
      const memoryTask = await this.createMemoryReviewTask();
      if (memoryTask) {
        tasks.push(memoryTask);
        this.lastMemoryReview = now;
      }
    }

    // 2. Skill audit (every 6 hours)
    if (now - this.lastSkillAudit > this.config.skillAuditInterval) {
      const auditTask = await this.createSkillAuditTask();
      if (auditTask) {
        tasks.push(auditTask);
        this.lastSkillAudit = now;
      }
    }

    // 3. Unfinished work detection
    const resumeTasks = await this.detectUnfinishedWork();
    tasks.push(...resumeTasks);

    // 4. Continuous learning (if mistakes detected)
    const learningTasks = await this.detectLearningOpportunities();
    tasks.push(...learningTasks);

    return tasks;
  }

  /**
   * Create memory consolidation task
   * Reviews recent activities and consolidates important memories
   */
  private async createMemoryReviewTask(): Promise<TaskEntity | null> {
    const task = new TaskEntity();

    task.assigneeId = this.personaId;
    task.createdBy = this.personaId;  // Self-created!
    task.domain = 'self';
    task.taskType = 'memory-consolidation';
    task.contextId = this.personaId;  // Self-context
    task.description = `[Self-Task] Review and consolidate recent memories`;
    task.priority = 0.5;              // Medium priority
    task.status = 'pending';

    const validation = task.validate();
    if (!validation.success) {
      console.error(`❌ ${this.displayName}: Failed to create memory review task:`, validation.error);
      return null;
    }

    return task;
  }

  /**
   * Create skill audit task
   * Evaluates current capabilities and identifies areas for improvement
   */
  private async createSkillAuditTask(): Promise<TaskEntity | null> {
    const task = new TaskEntity();

    task.assigneeId = this.personaId;
    task.createdBy = this.personaId;
    task.domain = 'self';
    task.taskType = 'skill-audit';
    task.contextId = this.personaId;
    task.description = `[Self-Task] Audit skills and identify improvement areas`;
    task.priority = 0.6;              // Medium-high priority
    task.status = 'pending';

    const validation = task.validate();
    if (!validation.success) {
      console.error(`❌ ${this.displayName}: Failed to create skill audit task:`, validation.error);
      return null;
    }

    return task;
  }

  /**
   * Detect unfinished work
   * Looks for in_progress tasks that haven't been updated recently
   */
  private async detectUnfinishedWork(): Promise<TaskEntity[]> {
    try {
      // Query for in_progress tasks assigned to this persona
      const result = await DataDaemon.query<TaskEntity>({
        collection: COLLECTIONS.TASKS,
        filter: {
          assigneeId: this.personaId,
          status: 'in_progress'
        },
        limit: 10
      });

      if (!result.success || !result.data) {
        return [];
      }

      const resumeTasks: TaskEntity[] = [];
      const threshold = Date.now() - this.config.unfinishedWorkThreshold;

      for (const record of result.data) {
        const task = record.data;
        const updatedAt = task.updatedAt ? new Date(task.updatedAt).getTime() : 0;

        // If task hasn't been updated in a while, create resume task
        if (updatedAt < threshold) {
          const resumeTask = new TaskEntity();

          resumeTask.assigneeId = this.personaId;
          resumeTask.createdBy = this.personaId;
          resumeTask.domain = 'self';
          resumeTask.taskType = 'resume-work';
          resumeTask.contextId = this.personaId;
          resumeTask.description = `[Self-Task] Resume unfinished work: ${task.description}`;
          resumeTask.priority = 0.7;  // High priority
          resumeTask.status = 'pending';

          const validation = resumeTask.validate();
          if (validation.success) {
            resumeTasks.push(resumeTask);
          }
        }
      }

      return resumeTasks;
    } catch (error) {
      console.error(`❌ ${this.displayName}: Error detecting unfinished work:`, error);
      return [];
    }
  }

  /**
   * Detect learning opportunities
   * Analyzes recent failed tasks to create fine-tuning tasks
   */
  private async detectLearningOpportunities(): Promise<TaskEntity[]> {
    try {
      // Query for recent failed tasks
      const result = await DataDaemon.query<TaskEntity>({
        collection: COLLECTIONS.TASKS,
        filter: {
          assigneeId: this.personaId,
          status: 'failed'
        },
        limit: 5
      });

      if (!result.success || !result.data || result.data.length === 0) {
        return [];
      }

      // Group failures by domain
      const failuresByDomain: Record<string, typeof result.data> = {};
      for (const record of result.data) {
        const domain = record.data.domain;
        if (!failuresByDomain[domain]) {
          failuresByDomain[domain] = [];
        }
        failuresByDomain[domain].push(record);
      }

      const learningTasks: TaskEntity[] = [];

      // Create learning task for each domain with failures
      for (const [domain, failures] of Object.entries(failuresByDomain)) {
        const learningTask = new TaskEntity();

        learningTask.assigneeId = this.personaId;
        learningTask.createdBy = this.personaId;
        learningTask.domain = 'self';
        learningTask.taskType = 'fine-tune-lora';
        learningTask.contextId = this.personaId;
        learningTask.description = `[Self-Task] Learn from ${failures.length} recent ${domain} failures`;
        learningTask.priority = 0.8;  // Very high priority (learning is important!)
        learningTask.status = 'pending';

        learningTask.metadata = {
          loraLayer: `${domain}-expertise`  // Which adapter to fine-tune
        };

        const validation = learningTask.validate();
        if (validation.success) {
          learningTasks.push(learningTask);
        }
      }

      return learningTasks;
    } catch (error) {
      console.error(`❌ ${this.displayName}: Error detecting learning opportunities:`, error);
      return [];
    }
  }

  /**
   * Update configuration
   */
  setConfig(config: Partial<SelfTaskGeneratorConfig>): void {
    this.config = { ...this.config, ...config };
  }

  /**
   * Enable/disable self-task generation
   */
  setEnabled(enabled: boolean): void {
    this.config.enabled = enabled;
  }

  /**
   * Get current configuration
   */
  getConfig(): SelfTaskGeneratorConfig {
    return { ...this.config };
  }

  /**
   * Reset timers (for testing)
   */
  resetTimers(): void {
    this.lastMemoryReview = 0;
    this.lastSkillAudit = 0;
  }
}
