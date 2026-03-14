/**
 * SelfTaskGenerator — Converts GapReport into TaskEntity records
 *
 * Applies gates before creating any task:
 *   1. GPU pressure must be 'normal'
 *   2. Sweep cooldown (5 min between sweeps)
 *   3. Max 2 concurrent learning tasks per persona
 *   4. Per-domain cooldown (1 hour between academy enrollments for same domain)
 *   5. Dedup: no pending task for same domain+action
 *
 * Created tasks are standard TaskEntity records in the database.
 * PersonaInbox picks them up naturally through Rust polling — no new
 * execution code needed (PersonaTaskExecutor already handles all task types).
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { generateUUID } from '../../../core/types/CrossPlatformUUID';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { TaskEntity } from '../../../data/entities/TaskEntity';
import { COLLECTIONS } from '../../../shared/Constants';
import { GpuPressureWatcher } from '../../../gpu/server/GpuPressureWatcher';
import type { GapReport, DomainGap, SuggestedAction } from './GapDetector';
import { PersonaTimingConfig } from './PersonaTimingConfig';

// ============================================================================
// Constants
// ============================================================================

/** Minimum time between gap sweeps (ms) */
const SWEEP_COOLDOWN_MS = PersonaTimingConfig.selfTask.sweepCooldownMs;

/** Maximum concurrent learning tasks per persona */
const MAX_CONCURRENT_LEARNING = 2;

/** Minimum time between academy enrollments for the same domain (ms) */
const DOMAIN_COOLDOWN_MS = PersonaTimingConfig.selfTask.domainCooldownMs;

/** Minimum severity to act on */
const MIN_SEVERITY = 0.2;

// ============================================================================
// SelfTaskGenerator
// ============================================================================

export class SelfTaskGenerator {
  private readonly personaId: UUID;
  private readonly log: (message: string) => void;

  /** Last sweep timestamp */
  private lastSweepAt = 0;

  /** Per-domain last enrollment timestamp */
  private domainCooldowns: Map<string, number> = new Map();

  constructor(personaId: UUID, logger: (message: string) => void) {
    this.personaId = personaId;
    this.log = logger;
  }

  /**
   * Generate tasks from a gap report.
   *
   * Applies all gates, then creates TaskEntity records for actionable gaps.
   * Returns the number of tasks created.
   */
  async generateFromGaps(report: GapReport): Promise<number> {
    // Gate 1: GPU pressure — graduated blocking
    // high/critical → block all task generation
    // warning → block GPU-heavy tasks only (enroll-academy, fine-tune-lora)
    // normal → all allowed
    let gpuLevel: string = 'normal';
    try {
      gpuLevel = GpuPressureWatcher.instance.currentLevel;
      if (gpuLevel === 'high' || gpuLevel === 'critical') {
        this.log(`⏸️ SelfTaskGenerator: GPU pressure=${gpuLevel}, blocking all task generation`);
        return 0;
      }
    } catch {
      // GpuPressureWatcher not initialized — proceed anyway (no GPU contention info)
    }

    // Gate 2: Sweep cooldown
    const now = Date.now();
    if (now - this.lastSweepAt < SWEEP_COOLDOWN_MS) {
      return 0;
    }
    this.lastSweepAt = now;

    // Gate 3: Check concurrent learning task count
    const activeLearningCount = await this.countActiveLearningTasks();
    if (activeLearningCount >= MAX_CONCURRENT_LEARNING) {
      this.log(`⏸️ SelfTaskGenerator: ${activeLearningCount} learning tasks active (max=${MAX_CONCURRENT_LEARNING})`);
      return 0;
    }

    const slotsAvailable = MAX_CONCURRENT_LEARNING - activeLearningCount;

    // Filter to actionable gaps and create tasks (up to available slots)
    let created = 0;
    for (const gap of report.gaps) {
      if (created >= slotsAvailable) break;
      if (gap.severity < MIN_SEVERITY) break;  // Gaps are sorted by severity desc
      if (gap.suggestedAction === 'none') continue;

      // Gate: GPU warning blocks GPU-heavy tasks but allows lighter ones
      if (gpuLevel === 'warning') {
        const wouldBeGpuHeavy = gap.suggestedAction === 'retrain' ||
          gap.suggestedAction === 'enroll-academy' ||
          gap.suggestedAction === 'fine-tune-lora';
        if (wouldBeGpuHeavy) {
          this.log(`⏸️ SelfTaskGenerator: GPU pressure=warning, skipping GPU-heavy task: ${gap.suggestedAction} for ${gap.domain}`);
          continue;
        }
      }

      // Gate 4: Per-domain cooldown
      const lastEnrollment = this.domainCooldowns.get(gap.domain) ?? 0;
      if (now - lastEnrollment < DOMAIN_COOLDOWN_MS) {
        this.log(`⏸️ SelfTaskGenerator: Domain ${gap.domain} in cooldown (${Math.round((DOMAIN_COOLDOWN_MS - (now - lastEnrollment)) / 60000)}min remaining)`);
        continue;
      }

      // Gate 5: Dedup — no pending task for same domain+action
      const isDuplicate = await this.hasPendingTask(gap.domain, gap.suggestedAction);
      if (isDuplicate) {
        continue;
      }

      // Create the task
      const taskCreated = await this.createLearningTask(gap);
      if (taskCreated) {
        created++;
        this.domainCooldowns.set(gap.domain, now);
      }
    }

    if (created > 0) {
      this.log(`🎓 SelfTaskGenerator: Created ${created} learning task(s) from ${report.gaps.length} gaps`);
    }

    return created;
  }

  // ── Private ──────────────────────────────────────────────────────────

  /**
   * Count in-progress or pending learning tasks for this persona.
   */
  private async countActiveLearningTasks(): Promise<number> {
    try {
      const result = await ORM.query<TaskEntity>({
        collection: COLLECTIONS.TASKS,
        filter: {
          assigneeId: this.personaId,
          domain: 'self',
          status: { $in: ['pending', 'in_progress'] },
        },
        limit: MAX_CONCURRENT_LEARNING + 1,
      }, 'default');

      const tasks = result.data ?? [];
      return tasks.filter(r => {
        const t = r.data;
        return t.taskType === 'enroll-academy' || t.taskType === 'fine-tune-lora';
      }).length;
    } catch {
      return 0;
    }
  }

  /**
   * Check if a pending or in_progress task already exists for domain+action.
   */
  private async hasPendingTask(domain: string, action: SuggestedAction): Promise<boolean> {
    const taskType = action === 'retrain' ? 'enroll-academy' : action;

    try {
      const result = await ORM.query<TaskEntity>({
        collection: COLLECTIONS.TASKS,
        filter: {
          assigneeId: this.personaId,
          taskType,
          status: { $in: ['pending', 'in_progress'] },
        },
        limit: 10,
      }, 'default');

      const tasks = result.data ?? [];
      return tasks.some(r => {
        const metadata = r.data.metadata;
        return metadata?.domain === domain || metadata?.loraLayer === domain;
      });
    } catch {
      return false;
    }
  }

  /**
   * Create a TaskEntity for a detected gap.
   */
  private async createLearningTask(gap: DomainGap): Promise<boolean> {
    const taskType = gap.suggestedAction === 'retrain' || gap.suggestedAction === 'enroll-academy'
      ? 'enroll-academy' as const
      : 'fine-tune-lora' as const;

    const signalSummary = gap.signals.map(s => s.description).join('; ');

    const task = new TaskEntity();
    task.id = generateUUID();
    task.assigneeId = this.personaId;
    task.createdBy = this.personaId;
    task.domain = 'self';
    task.taskType = taskType;
    task.contextId = this.personaId;
    task.description = `[Auto] ${taskType === 'enroll-academy' ? 'Learn' : 'Fine-tune'} ${gap.domain} (severity=${gap.severity.toFixed(2)}): ${signalSummary}`;
    task.priority = Math.min(0.5 + gap.severity * 0.4, 0.9);
    task.status = 'pending';
    task.createdAt = new Date();
    task.metadata = {
      domain: gap.domain,
      suggested_mode: gap.suggestedMode,
      loraLayer: gap.domain,
      skillName: gap.domain,
    };

    try {
      await ORM.store(COLLECTIONS.TASKS, task, false, 'default');
      this.log(`📋 SelfTaskGenerator: Created ${taskType} task for ${gap.domain} (severity=${gap.severity.toFixed(2)}, mode=${gap.suggestedMode})`);
      return true;
    } catch (error) {
      this.log(`❌ SelfTaskGenerator: Failed to create task for ${gap.domain}: ${error}`);
      return false;
    }
  }
}
