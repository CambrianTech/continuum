/**
 * SentinelTrigger - Event-triggered sentinel execution
 *
 * Manages sentinels that should run in response to events.
 * Supports:
 * - Event triggers (e.g., 'git:push', 'file:changed')
 * - Cron/schedule triggers (future)
 * - Manual triggers
 */

import { Events } from '../core/shared/Events';
import { SentinelRunner, runSentinel } from './SentinelRunner';
import type { PipelineSentinelDefinition, SentinelTrigger as TriggerConfig } from './SentinelDefinition';

/**
 * Registered trigger with its sentinel definition
 */
interface RegisteredTrigger {
  id: string;
  definition: PipelineSentinelDefinition;
  trigger: TriggerConfig;
  unsubscribe?: () => void;
  lastTriggered?: number;
  running: boolean;
}

/**
 * Active sentinel instance
 */
interface ActiveSentinel {
  id: string;
  definition: PipelineSentinelDefinition;
  startedAt: number;
  promise: Promise<unknown>;
}

/**
 * SentinelTriggerManager - Manages event-triggered sentinels
 */
export class SentinelTriggerManager {
  private triggers: Map<string, RegisteredTrigger> = new Map();
  private active: Map<string, ActiveSentinel> = new Map();
  private workingDir: string;

  constructor(workingDir: string) {
    this.workingDir = workingDir;
  }

  /**
   * Register a sentinel with an event trigger.
   */
  register(definition: PipelineSentinelDefinition): string {
    if (!definition.trigger) {
      throw new Error(`Sentinel ${definition.name} has no trigger configuration`);
    }

    const id = definition.id || `trigger-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const trigger = definition.trigger;

    const registered: RegisteredTrigger = {
      id,
      definition,
      trigger,
      running: false,
    };

    // Set up event subscription based on trigger type
    if (trigger.type === 'event') {
      const handler = (eventData: unknown) => {
        this.handleEvent(id, eventData);
      };
      // Events.subscribe returns the unsubscribe function
      registered.unsubscribe = Events.subscribe(trigger.event, handler);
    } else if (trigger.type === 'cron') {
      // Cron triggers would require a scheduler - not implemented yet
      console.warn(`Cron triggers not yet implemented for ${definition.name}`);
    }

    this.triggers.set(id, registered);
    const triggerInfo = trigger.type === 'event' ? trigger.event : (trigger.type === 'cron' ? trigger.schedule : trigger.type);
    console.log(`[SentinelTrigger] Registered: ${definition.name} on ${trigger.type}:${triggerInfo}`);

    return id;
  }

  /**
   * Unregister a trigger.
   */
  unregister(id: string): boolean {
    const registered = this.triggers.get(id);
    if (!registered) return false;

    if (registered.unsubscribe) {
      registered.unsubscribe();
    }

    this.triggers.delete(id);
    console.log(`[SentinelTrigger] Unregistered: ${registered.definition.name}`);
    return true;
  }

  /**
   * Handle an event that may trigger a sentinel.
   */
  private async handleEvent(triggerId: string, eventData: unknown): Promise<void> {
    const registered = this.triggers.get(triggerId);
    if (!registered) return;

    // Check debounce/throttle (only for event/cron triggers)
    const trigger = registered.trigger;
    const debounceMs = (trigger.type === 'event' || trigger.type === 'cron') ? trigger.debounceMs : undefined;
    const allowConcurrent = (trigger.type === 'event' || trigger.type === 'cron') ? trigger.allowConcurrent : false;

    if (debounceMs && registered.lastTriggered) {
      const elapsed = Date.now() - registered.lastTriggered;
      if (elapsed < debounceMs) {
        console.log(`[SentinelTrigger] Debounced: ${registered.definition.name} (${elapsed}ms < ${debounceMs}ms)`);
        return;
      }
    }

    // Check if already running (prevent concurrent execution)
    if (registered.running && !allowConcurrent) {
      console.log(`[SentinelTrigger] Skipped: ${registered.definition.name} already running`);
      return;
    }

    registered.lastTriggered = Date.now();
    registered.running = true;

    console.log(`[SentinelTrigger] Triggered: ${registered.definition.name}`);

    try {
      // Run the sentinel with event data as initial variable
      const result = await runSentinel(registered.definition, this.workingDir, {
        onLog: (msg, level) => console.log(`[${registered.definition.name}] ${msg}`),
      });

      // Inject event data into context before first iteration would be better,
      // but for now we can't modify runSentinel's initial variables easily.
      // The event data should be accessible via the trigger config.

      console.log(`[SentinelTrigger] Completed: ${registered.definition.name} - ${result.success ? 'SUCCESS' : 'FAILED'}`);
    } catch (error: any) {
      console.error(`[SentinelTrigger] Error: ${registered.definition.name} - ${error.message}`);
    } finally {
      registered.running = false;
    }
  }

  /**
   * Manually trigger a registered sentinel.
   */
  async triggerManually(id: string, eventData?: unknown): Promise<void> {
    await this.handleEvent(id, eventData);
  }

  /**
   * List all registered triggers.
   */
  list(): Array<{ id: string; name: string; trigger: TriggerConfig; running: boolean }> {
    return Array.from(this.triggers.values()).map(t => ({
      id: t.id,
      name: t.definition.name,
      trigger: t.trigger,
      running: t.running,
    }));
  }

  /**
   * Stop all triggers and cleanup.
   */
  shutdown(): void {
    for (const [id] of this.triggers) {
      this.unregister(id);
    }
    console.log('[SentinelTrigger] Shutdown complete');
  }
}

// Singleton instance for global access
let globalManager: SentinelTriggerManager | null = null;

/**
 * Get the global trigger manager (lazy initialization).
 */
export function getTriggerManager(workingDir?: string): SentinelTriggerManager {
  if (!globalManager) {
    if (!workingDir) {
      throw new Error('workingDir required for initial trigger manager creation');
    }
    globalManager = new SentinelTriggerManager(workingDir);
  }
  return globalManager;
}

/**
 * Register a sentinel for event-triggered execution.
 */
export function registerTrigger(definition: PipelineSentinelDefinition, workingDir: string): string {
  return getTriggerManager(workingDir).register(definition);
}

/**
 * Unregister a trigger.
 */
export function unregisterTrigger(id: string): boolean {
  if (!globalManager) return false;
  return globalManager.unregister(id);
}
