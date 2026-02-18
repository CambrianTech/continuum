/**
 * SentinelEscalationService — Bridges sentinel lifecycle to persona cognition
 *
 * When sentinels complete, fail, or need attention, this service routes
 * notifications to the owning persona's inbox. This is the subconscious →
 * conscious escalation pathway: sentinels run silently until something
 * needs the persona's attention, then it bubbles up as an inbox task.
 *
 * Subscribes to sentinel events emitted by the Rust engine:
 *   - sentinel:{handle}:complete → InboxTask with 'sentinel-complete'
 *   - sentinel:{handle}:error   → InboxTask with 'sentinel-failed'
 *   - sentinel:{handle}:step    → (future: progress tracking)
 *
 * The service also persists execution results to the SentinelEntity
 * when a sentinel finishes, linking the ephemeral Rust handle back
 * to the durable database entity.
 */

import { Events } from '../core/shared/Events';
import { Commands } from '../core/shared/Commands';
import type { UUID } from '../core/types/CrossPlatformUUID';
import { generateUUID } from '../core/types/CrossPlatformUUID';
import type { InboxTask } from '../user/server/modules/QueueItemTypes';
import type { SentinelEntity, SentinelExecutionResult } from './SentinelDefinition';
import type { EscalationRule, EscalationPriority } from './entities/SentinelEntity';
import { DEFAULT_ESCALATION_RULES } from './entities/SentinelEntity';
import type { MemoryEntity } from '../user/server/modules/MemoryTypes';

/**
 * Priority mapping: escalation priority → numeric inbox priority
 */
const PRIORITY_MAP: Record<EscalationPriority, number> = {
  low: 0.3,
  normal: 0.5,
  high: 0.7,
  urgent: 0.9,
};

/**
 * Sentinel lifecycle event payload (emitted by Rust engine via MessageBus)
 */
interface SentinelLifecycleEvent {
  handle: string;
  status: 'completed' | 'failed' | 'cancelled';
  error?: string;
  durationMs?: number;
  stepsCompleted?: number;
  totalSteps?: number;
}

/**
 * In-memory tracking of handle → entity ID mapping.
 * When sentinel/run is called, the caller can register the mapping
 * so we know which entity to update when the sentinel finishes.
 */
const handleToEntityMap = new Map<string, {
  entityId: string;
  parentPersonaId?: string;
  escalationRules: EscalationRule[];
  sentinelName: string;
  startedAt: string;
}>();

/**
 * Register a sentinel handle for lifecycle tracking.
 * Called by sentinel/run or academy-session when spawning sentinels.
 */
export function registerSentinelHandle(
  handle: string,
  entityId: string,
  parentPersonaId?: string,
  escalationRules?: EscalationRule[],
  sentinelName?: string,
): void {
  handleToEntityMap.set(handle, {
    entityId,
    parentPersonaId,
    escalationRules: escalationRules ?? DEFAULT_ESCALATION_RULES,
    sentinelName: sentinelName ?? 'unnamed',
    startedAt: new Date().toISOString(),
  });
}

/**
 * Unregister a sentinel handle (cleanup after processing).
 */
export function unregisterSentinelHandle(handle: string): void {
  handleToEntityMap.delete(handle);
}

/**
 * Initialize the escalation service — subscribe to sentinel lifecycle events.
 * Called once during server startup.
 */
export function initializeSentinelEscalation(): void {
  // Subscribe to sentinel completion events
  Events.subscribe('sentinel:*:complete', async (payload: SentinelLifecycleEvent) => {
    await handleSentinelLifecycle(payload, 'completed');
  });

  Events.subscribe('sentinel:*:error', async (payload: SentinelLifecycleEvent) => {
    await handleSentinelLifecycle(payload, 'failed');
  });

  Events.subscribe('sentinel:*:cancelled', async (payload: SentinelLifecycleEvent) => {
    await handleSentinelLifecycle(payload, 'cancelled');
  });

  console.log('[SentinelEscalation] Initialized — listening for sentinel lifecycle events');
}

/**
 * Handle a sentinel lifecycle event: persist execution result + escalate to persona
 */
async function handleSentinelLifecycle(
  payload: SentinelLifecycleEvent,
  status: 'completed' | 'failed' | 'cancelled',
): Promise<void> {
  const handle = payload.handle;
  const tracking = handleToEntityMap.get(handle);

  if (!tracking) {
    // Sentinel was not registered for tracking (e.g., ad-hoc run without save)
    return;
  }

  try {
    // 1. Build execution result
    const executionResult: SentinelExecutionResult = {
      handle,
      success: status === 'completed',
      startedAt: tracking.startedAt,
      completedAt: new Date().toISOString(),
      durationMs: payload.durationMs,
      error: payload.error,
    };

    // 2. Persist execution result to entity
    await persistExecutionResult(tracking.entityId, executionResult, status);

    // 3. Check escalation rules and route to persona inbox
    const condition = status === 'completed' ? 'complete' :
                      status === 'failed' ? 'error' : 'error';
    const matchingRule = tracking.escalationRules.find(r => r.condition === condition);

    if (matchingRule && tracking.parentPersonaId && matchingRule.action !== 'pause') {
      await escalateToPersonaInbox(
        tracking.parentPersonaId,
        tracking.sentinelName,
        tracking.entityId,
        handle,
        status,
        matchingRule,
        payload.error,
      );
    }

    // 4. Store sentinel execution as persona memory (for pattern recall)
    if (tracking.parentPersonaId) {
      await storeSentinelMemory(
        tracking.parentPersonaId,
        tracking.sentinelName,
        tracking.entityId,
        status,
        payload.durationMs,
        payload.stepsCompleted,
        payload.totalSteps,
        payload.error,
      );
    }

    // 5. Cleanup tracking
    handleToEntityMap.delete(handle);
  } catch (err) {
    console.error(`[SentinelEscalation] Error handling lifecycle for ${handle}: ${err}`);
  }
}

/**
 * Persist a sentinel execution result to the entity in the database.
 */
async function persistExecutionResult(
  entityId: string,
  result: SentinelExecutionResult,
  status: string,
): Promise<void> {
  try {
    // Fetch current entity
    const listResult = await Commands.execute('data/list', {
      collection: 'sentinels',
      filter: { id: entityId },
      limit: 1,
    } as any) as any;

    const entity = listResult?.items?.[0] as SentinelEntity | undefined;
    if (!entity) return;

    // Update execution history
    const executions = [result, ...(entity.executions || [])].slice(0, 50);

    await Commands.execute('data/update', {
      collection: 'sentinels',
      id: entityId,
      data: {
        executions,
        status,
        activeHandle: null,
        executionCount: (entity.executionCount ?? 0) + 1,
        lastSuccess: result.success,
        lastRunAt: result.startedAt,
        updatedAt: new Date().toISOString(),
      },
    } as any);
  } catch (err) {
    console.error(`[SentinelEscalation] Failed to persist execution result for ${entityId}: ${err}`);
  }
}

/**
 * Create an InboxTask for the owning persona when a sentinel needs attention.
 */
async function escalateToPersonaInbox(
  parentPersonaId: string,
  sentinelName: string,
  entityId: string,
  handle: string,
  status: 'completed' | 'failed' | 'cancelled',
  rule: EscalationRule,
  error?: string,
): Promise<void> {
  const taskType = status === 'completed' ? 'sentinel-complete' as const :
                   status === 'failed' ? 'sentinel-failed' as const :
                   'sentinel-escalation' as const;

  const description = status === 'completed'
    ? `Sentinel "${sentinelName}" completed successfully`
    : status === 'failed'
    ? `Sentinel "${sentinelName}" failed: ${error ?? 'unknown error'}`
    : `Sentinel "${sentinelName}" was cancelled`;

  const task: InboxTask = {
    id: generateUUID(),
    type: 'task',
    taskId: generateUUID(),
    assigneeId: parentPersonaId as UUID,
    createdBy: parentPersonaId as UUID,
    domain: 'sentinel',
    taskType,
    contextId: entityId as UUID,
    description,
    priority: PRIORITY_MAP[rule.priority],
    status: 'pending',
    timestamp: Date.now(),
    metadata: {
      sentinelName,
      sentinelEntityId: entityId,
      sentinelHandle: handle,
      sentinelStatus: status,
      error,
    },
  };

  // Emit the task event — PersonaUser's event listener will pick it up
  // and enqueue it into the persona's inbox
  Events.emit(`task:${parentPersonaId}:created`, task);
}

/**
 * Store a sentinel execution as a MemoryEntity for the owning persona.
 *
 * This enables pattern recall: when the persona faces a similar task,
 * it can recall past sentinel executions that worked (or failed) and
 * re-use or avoid those patterns.
 */
async function storeSentinelMemory(
  parentPersonaId: string,
  sentinelName: string,
  entityId: string,
  status: 'completed' | 'failed' | 'cancelled',
  durationMs?: number,
  stepsCompleted?: number,
  totalSteps?: number,
  error?: string,
): Promise<void> {
  try {
    const now = new Date().toISOString();
    const durationStr = durationMs ? `${(durationMs / 1000).toFixed(1)}s` : 'unknown';
    const stepsStr = totalSteps ? `${stepsCompleted ?? '?'}/${totalSteps}` : '';

    const content = status === 'completed'
      ? `Sentinel "${sentinelName}" completed successfully in ${durationStr}${stepsStr ? ` (${stepsStr} steps)` : ''}`
      : status === 'failed'
      ? `Sentinel "${sentinelName}" failed after ${durationStr}: ${error ?? 'unknown error'}`
      : `Sentinel "${sentinelName}" was cancelled after ${durationStr}`;

    // Importance: successful executions are more worth remembering than failures
    // (we learn patterns from success; errors are already tracked in entity history)
    const importance = status === 'completed' ? 0.7 : status === 'failed' ? 0.5 : 0.3;

    const memory: Partial<MemoryEntity> = {
      id: generateUUID(),
      personaId: parentPersonaId,
      sessionId: 'sentinel-lifecycle',
      type: 'sentinel' as any,
      content,
      context: {
        sentinelName,
        sentinelEntityId: entityId,
        status,
        durationMs,
        stepsCompleted,
        totalSteps,
        error,
      },
      timestamp: now as any,
      importance,
      accessCount: 0,
      relatedTo: [entityId],
      tags: ['sentinel', sentinelName, status],
      source: 'sentinel-escalation',
    };

    await Commands.execute('data/create', {
      collection: 'memories',
      data: memory,
    } as any);

    console.log(`[SentinelEscalation] Stored sentinel memory for persona ${parentPersonaId}: ${content.slice(0, 80)}`);
  } catch (err) {
    // Non-critical — don't let memory storage failure break escalation flow
    console.error(`[SentinelEscalation] Failed to store sentinel memory: ${err}`);
  }
}
