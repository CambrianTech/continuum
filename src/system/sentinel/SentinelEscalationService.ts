/**
 * SentinelEscalationService — Bridges sentinel lifecycle to persona cognition
 *
 * When sentinels complete, fail, or need attention, this service routes
 * notifications to the owning persona's inbox. This is the subconscious →
 * conscious escalation pathway: sentinels run silently until something
 * needs the persona's attention, then it bubbles up as an inbox task.
 *
 * Architecture:
 *   Rust SentinelModule owns the lifecycle. On completion, it pushes to
 *   TypeScript via execute_ts_json("sentinel/escalate", {...}). This service
 *   receives that push and handles:
 *     1. Execution result persistence to SentinelEntity
 *     2. Escalation rule evaluation → persona inbox delivery
 *     3. Sentinel memory storage for pattern recall
 *
 *   No polling, no in-memory maps, no Handle DB lookups.
 *   Rust is the single source of truth for sentinel lifecycle.
 */

import type { UUID } from '../core/types/CrossPlatformUUID';
import { generateUUID } from '../core/types/CrossPlatformUUID';
import type { InboxTask } from '../user/server/modules/QueueItemTypes';
import type { SentinelEntity, SentinelExecutionResult } from './SentinelDefinition';
import type { EscalationRule, EscalationPriority } from './entities/SentinelEntity';
import { DEFAULT_ESCALATION_RULES } from './entities/SentinelEntity';
import type { MemoryEntity } from '../user/server/modules/MemoryTypes';
import { MemoryType } from '../user/server/modules/MemoryTypes';
import { ISOString } from '../data/domains/CoreTypes';
import { CognitionLogger } from '../user/server/modules/cognition/CognitionLogger';
import { getPersonaInbox } from '../user/server/PersonaInboxRegistry';
import { DataList } from '../../commands/data/list/shared/DataListTypes';
import { DataUpdate } from '../../commands/data/update/shared/DataUpdateTypes';
import { DataCreate } from '../../commands/data/create/shared/DataCreateTypes';

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
 * Payload pushed from Rust via sentinel/escalate command.
 */
export interface SentinelEscalationPayload {
  handle: string;
  status: 'completed' | 'failed' | 'cancelled';
  durationMs?: number;
  error?: string;
  parentPersonaId?: string;
  entityId?: string;
  sentinelName: string;
  escalationRules?: EscalationRule[];
}

/**
 * Handle a sentinel lifecycle push from Rust.
 *
 * Called by sentinel/escalate command — Rust pushes on completion,
 * no polling or event subscriptions needed.
 */
export async function handleSentinelEscalation(
  payload: SentinelEscalationPayload,
): Promise<void> {
  const {
    handle,
    status,
    durationMs,
    error,
    parentPersonaId,
    entityId,
    sentinelName,
  } = payload;
  const escalationRules = payload.escalationRules ?? DEFAULT_ESCALATION_RULES;
  const startedAt = durationMs != null
    ? new Date(Date.now() - durationMs).toISOString()
    : new Date().toISOString();

  try {
    // 1. Build execution result
    const executionResult: SentinelExecutionResult = {
      handle,
      success: status === 'completed',
      startedAt,
      completedAt: new Date().toISOString(),
      durationMs,
      error,
    };

    // 2. Persist to SentinelEntity
    if (entityId) {
      await persistExecutionResult(entityId, executionResult, status);
    }

    // 3. Evaluate escalation rules → route to persona inbox
    const condition = status === 'completed' ? 'complete' : 'error';
    const matchingRule = escalationRules.find(r => r.condition === condition);

    if (matchingRule && parentPersonaId && matchingRule.action !== 'pause') {
      await escalateToPersonaInbox(
        parentPersonaId,
        sentinelName,
        entityId ?? '',
        handle,
        status,
        matchingRule,
        error,
      );
    }

    // 4. Store sentinel memory for pattern recall
    if (parentPersonaId) {
      await storeSentinelMemory(
        parentPersonaId,
        sentinelName,
        entityId ?? '',
        status,
        durationMs,
        error,
      );
    }
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
    const listResult = await DataList.execute({
      collection: 'sentinels',
      filter: { id: entityId },
      limit: 1,
    });

    const entity = listResult?.items?.[0] as unknown as SentinelEntity | undefined;
    if (!entity) return;

    const executions = [result, ...(entity.executions || [])].slice(0, 50);

    await DataUpdate.execute({
      collection: 'sentinels',
      id: entityId as UUID,
      data: {
        executions,
        status,
        activeHandle: null,
        executionCount: (entity.executionCount ?? 0) + 1,
        lastSuccess: result.success,
        lastRunAt: result.startedAt,
        updatedAt: new Date().toISOString(),
      },
    });
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

  const inbox = getPersonaInbox(parentPersonaId as UUID);
  if (inbox) {
    await inbox.enqueue(task);
    console.log(`[SentinelEscalation] Delivered ${taskType} to persona inbox (sentinel=${sentinelName})`);
  } else {
    // Persona not active — persist to DB for pickup on next startup
    try {
      await DataCreate.execute({
        collection: 'tasks',
        data: task,
      });
      console.log(`[SentinelEscalation] Persona offline, persisted ${taskType} task to DB (sentinel=${sentinelName})`);
    } catch (err) {
      console.error(`[SentinelEscalation] Failed to create ${taskType} task: ${err}`);
    }
  }
}

/**
 * Store a sentinel execution as a MemoryEntity for the owning persona.
 */
async function storeSentinelMemory(
  parentPersonaId: string,
  sentinelName: string,
  entityId: string,
  status: 'completed' | 'failed' | 'cancelled',
  durationMs?: number,
  error?: string,
): Promise<void> {
  try {
    const now = ISOString(new Date().toISOString());
    const durationStr = durationMs ? `${(durationMs / 1000).toFixed(1)}s` : 'unknown';

    const content = status === 'completed'
      ? `Sentinel "${sentinelName}" completed successfully in ${durationStr}`
      : status === 'failed'
      ? `Sentinel "${sentinelName}" failed after ${durationStr}: ${error ?? 'unknown error'}`
      : `Sentinel "${sentinelName}" was cancelled after ${durationStr}`;

    const importance = status === 'completed' ? 0.7 : status === 'failed' ? 0.5 : 0.3;

    const memory: Partial<MemoryEntity> = {
      id: generateUUID(),
      personaId: parentPersonaId,
      sessionId: 'sentinel-lifecycle',
      type: MemoryType.SENTINEL,
      content,
      context: {
        sentinelName,
        sentinelEntityId: entityId,
        status,
        durationMs,
        error,
      },
      timestamp: now,
      importance,
      accessCount: 0,
      relatedTo: [entityId],
      tags: ['sentinel', sentinelName, status],
      source: 'sentinel-escalation',
    };

    const dbHandle = CognitionLogger.getDbHandle(parentPersonaId as UUID);
    await DataCreate.execute({
      dbHandle,
      collection: 'memories',
      data: memory,
    });

    console.log(`[SentinelEscalation] Stored sentinel memory for persona ${parentPersonaId}: ${content.slice(0, 80)}`);
  } catch (err) {
    console.error(`[SentinelEscalation] Failed to store sentinel memory: ${err}`);
  }
}
