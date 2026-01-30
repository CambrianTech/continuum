/**
 * TaskQueueItem - Queue item for tasks assigned to the persona
 *
 * Tasks are dependency-aware and can consolidate related tasks.
 *
 * Overrides from BaseQueueItem:
 *   - isUrgent: true when past due date
 *   - canBeKicked: false for in-progress tasks
 *   - shouldConsolidateWith: true for same domain + context
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import type { TaskDomain, TaskType, TaskStatus } from '../../../../data/entities/TaskEntity';
import { BaseQueueItem, ActivityDomain, type BaseQueueItemParams } from './BaseQueueItem';

export interface TaskQueueItemParams extends BaseQueueItemParams {
  taskId: UUID;
  assigneeId: UUID;
  createdBy: UUID;
  taskDomain: TaskDomain;
  taskType: TaskType;
  contextId: UUID;
  description: string;
  priority: number;
  status: TaskStatus;
  dueDate?: number;
  estimatedDuration?: number;
  dependsOn?: UUID[];
  blockedBy?: UUID[];
  metadata?: Record<string, unknown>;
}

export class TaskQueueItem extends BaseQueueItem {
  readonly itemType = 'task' as const;
  readonly domain = ActivityDomain.BACKGROUND;

  readonly taskId: UUID;
  readonly assigneeId: UUID;
  readonly createdBy: UUID;
  readonly taskDomain: TaskDomain;
  readonly taskType: TaskType;
  readonly contextId: UUID;
  readonly description: string;
  readonly status: TaskStatus;
  readonly dueDate?: number;
  readonly estimatedDuration?: number;
  readonly dependsOn?: UUID[];
  readonly blockedBy?: UUID[];
  readonly metadata?: Record<string, unknown>;

  private readonly _basePriority: number;

  constructor(params: TaskQueueItemParams) {
    super(params);
    this.taskId = params.taskId;
    this.assigneeId = params.assigneeId;
    this.createdBy = params.createdBy;
    this.taskDomain = params.taskDomain;
    this.taskType = params.taskType;
    this.contextId = params.contextId;
    this.description = params.description;
    this._basePriority = params.priority;
    this.status = params.status;
    this.dueDate = params.dueDate;
    this.estimatedDuration = params.estimatedDuration;
    this.dependsOn = params.dependsOn;
    this.blockedBy = params.blockedBy;
    this.metadata = params.metadata;
  }

  get basePriority(): number { return this._basePriority; }

  // Urgent if past due date
  get isUrgent(): boolean {
    return this.dueDate != null && this.dueDate < Date.now();
  }

  // Don't kick in-progress tasks — dropping mid-work is wrong
  get canBeKicked(): boolean {
    return this.status !== 'in_progress';
  }

  // Blocked tasks have zero kick resistance (kick blocked tasks first)
  get kickResistance(): number {
    if (this.isBlocked) return 0;
    return this.effectivePriority;
  }

  /** Is this task blocked by unfinished dependencies? */
  get isBlocked(): boolean {
    return (this.blockedBy != null && this.blockedBy.length > 0);
  }

  // Consolidate related tasks: same task domain AND same context
  shouldConsolidateWith(other: BaseQueueItem): boolean {
    if (!(other instanceof TaskQueueItem)) return false;
    return other.taskDomain === this.taskDomain
      && other.contextId === this.contextId;
  }

  /**
   * Consolidate related tasks: keep highest priority task as primary,
   * attach others as related work.
   */
  consolidateWith(others: BaseQueueItem[]): TaskQueueItem {
    const taskOthers = others.filter(
      (o): o is TaskQueueItem => o instanceof TaskQueueItem
    );

    // Find highest priority task (including self)
    const allTasks = [this, ...taskOthers].sort(
      (a, b) => b._basePriority - a._basePriority
    );

    const primary = allTasks[0];

    // Use primary's data but combine metadata with related task IDs
    const relatedTaskIds = allTasks
      .filter(t => t !== primary)
      .map(t => t.taskId);

    return new TaskQueueItem({
      ...{
        id: primary.id,
        timestamp: primary.timestamp,
        enqueuedAt: this.enqueuedAt,
        taskId: primary.taskId,
        assigneeId: primary.assigneeId,
        createdBy: primary.createdBy,
        taskDomain: primary.taskDomain,
        taskType: primary.taskType,
        contextId: primary.contextId,
        description: primary.description,
        priority: primary._basePriority,
        status: primary.status,
        dueDate: primary.dueDate,
        estimatedDuration: primary.estimatedDuration,
        dependsOn: primary.dependsOn,
        blockedBy: primary.blockedBy,
        metadata: {
          ...primary.metadata,
          relatedTaskIds,
          consolidatedCount: allTasks.length,
        },
      }
    });
  }
}
