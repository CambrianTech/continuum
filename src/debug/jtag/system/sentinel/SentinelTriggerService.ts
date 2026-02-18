/**
 * SentinelTriggerService — Automatic sentinel execution based on trigger configuration
 *
 * Sentinels can declare triggers that cause them to start automatically:
 * - `immediate`: Start as soon as the trigger service initializes (one-shot)
 * - `event`: Start when a specific event fires (debounce-aware)
 * - `cron`: Start on a recurring schedule
 * - `manual`: Only started via explicit `sentinel/run` command
 *
 * The service loads all saved sentinels with trigger configs from the database
 * on startup, and subscribes to entity creation events to detect new triggers.
 *
 * Only sentinels with status='saved' are eligible for automatic triggering.
 * Running/failed/cancelled sentinels are skipped.
 */

import { Events } from '../core/shared/Events';
import { Commands } from '../core/shared/Commands';
import type { SentinelTrigger, PipelineSentinelDefinition } from './SentinelDefinition';
import type { SentinelEntity } from './SentinelDefinition';

/**
 * Tracked trigger registration for cleanup
 */
interface TriggerRegistration {
  entityId: string;
  sentinelName: string;
  trigger: SentinelTrigger;
  parentPersonaId?: string;
  /** For event triggers: subscription cleanup function */
  unsubscribe?: () => void;
  /** For cron triggers: interval timer handle */
  intervalHandle?: ReturnType<typeof setInterval>;
  /** For debounce: pending timeout handle */
  debounceHandle?: ReturnType<typeof setTimeout>;
  /** Whether a concurrent execution is already running */
  isRunning: boolean;
}

/**
 * Active trigger registrations by entity ID
 */
const activeTriggers = new Map<string, TriggerRegistration>();

/**
 * Initialize the trigger service — loads existing triggers and listens for new ones.
 * Called once during server startup (after data daemon is ready).
 */
export async function initializeSentinelTriggers(): Promise<void> {
  // Load all saved sentinels with trigger configs
  await loadExistingTriggers();

  // Listen for new sentinel saves
  Events.subscribe('data:sentinels:created', async (payload: any) => {
    const entity = payload as SentinelEntity;
    if (entity?.definition && hasTrigger(entity.definition)) {
      registerTrigger(entity);
    }
  });

  // Listen for sentinel updates (trigger might have been added/changed)
  Events.subscribe('data:sentinels:updated', async (payload: any) => {
    const entity = payload as SentinelEntity;
    if (!entity?.id) return;

    // Unregister old trigger if it existed
    unregisterTrigger(entity.id);

    // Re-register if still has a trigger and is in 'saved' status
    if (entity.definition && hasTrigger(entity.definition) && entity.status === 'saved') {
      registerTrigger(entity);
    }
  });

  // Listen for sentinel deletions
  Events.subscribe('data:sentinels:deleted', async (payload: any) => {
    if (payload?.id) {
      unregisterTrigger(payload.id);
    }
  });

  console.log(`[SentinelTrigger] Initialized — ${activeTriggers.size} trigger(s) registered`);
}

/**
 * Load all saved sentinels that have trigger configurations
 */
async function loadExistingTriggers(): Promise<void> {
  try {
    const result = await Commands.execute('data/list', {
      collection: 'sentinels',
      filter: { status: 'saved' },
      limit: 500,
    } as any) as any;

    const sentinels = (result?.items ?? []) as SentinelEntity[];

    for (const entity of sentinels) {
      if (entity.definition && hasTrigger(entity.definition)) {
        registerTrigger(entity);
      }
    }
  } catch (err) {
    console.error(`[SentinelTrigger] Failed to load existing triggers: ${err}`);
  }
}

/**
 * Check if a definition has a non-manual trigger
 */
function hasTrigger(definition: any): boolean {
  const trigger = (definition as PipelineSentinelDefinition).trigger;
  return !!trigger && trigger.type !== 'manual';
}

/**
 * Register a trigger for a sentinel entity
 */
function registerTrigger(entity: SentinelEntity): void {
  const definition = entity.definition as PipelineSentinelDefinition;
  const trigger = definition.trigger;
  if (!trigger || trigger.type === 'manual') return;

  const registration: TriggerRegistration = {
    entityId: entity.id,
    sentinelName: definition.name || 'unnamed',
    trigger,
    parentPersonaId: entity.parentPersonaId,
    isRunning: false,
  };

  switch (trigger.type) {
    case 'immediate':
      // Fire once, immediately
      console.log(`[SentinelTrigger] Immediate trigger: "${registration.sentinelName}" (${entity.id})`);
      executeSentinel(registration);
      break;

    case 'event':
      registerEventTrigger(registration, trigger);
      break;

    case 'cron':
      registerCronTrigger(registration, trigger);
      break;
  }

  activeTriggers.set(entity.id, registration);
}

/**
 * Register an event-based trigger
 */
function registerEventTrigger(
  registration: TriggerRegistration,
  trigger: Extract<SentinelTrigger, { type: 'event' }>,
): void {
  const handler = (payload: any) => {
    // Check concurrent execution guard
    if (registration.isRunning && !trigger.allowConcurrent) {
      return;
    }

    // Apply debounce if configured
    if (trigger.debounceMs && trigger.debounceMs > 0) {
      if (registration.debounceHandle) {
        clearTimeout(registration.debounceHandle);
      }
      registration.debounceHandle = setTimeout(() => {
        registration.debounceHandle = undefined;
        executeSentinel(registration, payload);
      }, trigger.debounceMs);
    } else {
      executeSentinel(registration, payload);
    }
  };

  registration.unsubscribe = Events.subscribe(trigger.event, handler);
  console.log(`[SentinelTrigger] Event trigger: "${registration.sentinelName}" on "${trigger.event}"` +
    `${trigger.debounceMs ? ` (debounce: ${trigger.debounceMs}ms)` : ''}` +
    `${trigger.allowConcurrent ? ' (concurrent OK)' : ''}`);
}

/**
 * Register a cron-like trigger (simplified: interval-based)
 *
 * Supported schedule formats:
 * - Milliseconds as number string: "60000" → every 60 seconds
 * - Simple interval: "every 5m", "every 1h", "every 30s"
 * - Standard cron: future enhancement (requires cron parser library)
 */
function registerCronTrigger(
  registration: TriggerRegistration,
  trigger: Extract<SentinelTrigger, { type: 'cron' }>,
): void {
  const intervalMs = parseCronSchedule(trigger.schedule);
  if (!intervalMs || intervalMs < 1000) {
    console.warn(`[SentinelTrigger] Invalid cron schedule "${trigger.schedule}" for "${registration.sentinelName}" — skipping`);
    return;
  }

  registration.intervalHandle = setInterval(() => {
    // Check concurrent execution guard
    if (registration.isRunning && !trigger.allowConcurrent) {
      return;
    }
    executeSentinel(registration);
  }, intervalMs);

  console.log(`[SentinelTrigger] Cron trigger: "${registration.sentinelName}" every ${intervalMs}ms (schedule: "${trigger.schedule}")`);
}

/**
 * Parse a cron schedule string into interval milliseconds.
 *
 * Supports:
 * - "every Ns" / "every Nm" / "every Nh" — interval shorthand
 * - Plain number string — treated as milliseconds
 */
export function parseCronSchedule(schedule: string): number | null {
  // Plain number → milliseconds
  const asNumber = Number(schedule);
  if (!isNaN(asNumber) && asNumber > 0) {
    return asNumber;
  }

  // "every Xs/m/h" format
  const match = schedule.match(/^every\s+(\d+)\s*(s|sec|seconds?|m|min|minutes?|h|hr|hours?)$/i);
  if (match) {
    const value = parseInt(match[1], 10);
    const unit = match[2].toLowerCase();

    if (unit.startsWith('s')) return value * 1000;
    if (unit.startsWith('m')) return value * 60 * 1000;
    if (unit.startsWith('h')) return value * 60 * 60 * 1000;
  }

  return null;
}

/**
 * Execute a sentinel via sentinel/run
 */
async function executeSentinel(
  registration: TriggerRegistration,
  triggerPayload?: any,
): Promise<void> {
  registration.isRunning = true;

  try {
    const result = await Commands.execute('sentinel/run', {
      type: 'pipeline',
      definition: registration.entityId,  // sentinel/run can accept entity ID
      entityId: registration.entityId,
      parentPersonaId: registration.parentPersonaId,
      sentinelName: registration.sentinelName,
    } as any) as any;

    if (result?.success) {
      console.log(`[SentinelTrigger] Started "${registration.sentinelName}" → handle: ${result.handle}`);
    } else {
      console.error(`[SentinelTrigger] Failed to start "${registration.sentinelName}": ${result?.error}`);
    }
  } catch (err) {
    console.error(`[SentinelTrigger] Error starting "${registration.sentinelName}": ${err}`);
  } finally {
    registration.isRunning = false;
  }
}

/**
 * Unregister a trigger (cleanup subscriptions and timers)
 */
function unregisterTrigger(entityId: string): void {
  const registration = activeTriggers.get(entityId);
  if (!registration) return;

  if (registration.unsubscribe) {
    registration.unsubscribe();
  }
  if (registration.intervalHandle) {
    clearInterval(registration.intervalHandle);
  }
  if (registration.debounceHandle) {
    clearTimeout(registration.debounceHandle);
  }

  activeTriggers.delete(entityId);
}

/**
 * Get count of active triggers (for diagnostics)
 */
export function getActiveTriggerCount(): number {
  return activeTriggers.size;
}

/**
 * List all active triggers (for diagnostics)
 */
export function listActiveTriggers(): Array<{
  entityId: string;
  sentinelName: string;
  triggerType: string;
  isRunning: boolean;
}> {
  return Array.from(activeTriggers.values()).map(r => ({
    entityId: r.entityId,
    sentinelName: r.sentinelName,
    triggerType: r.trigger.type,
    isRunning: r.isRunning,
  }));
}

/**
 * Shutdown — clean up all triggers
 */
export function shutdownSentinelTriggers(): void {
  for (const entityId of activeTriggers.keys()) {
    unregisterTrigger(entityId);
  }
  console.log('[SentinelTrigger] Shutdown — all triggers unregistered');
}
