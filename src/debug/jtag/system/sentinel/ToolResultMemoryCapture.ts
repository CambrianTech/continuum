/**
 * ToolResultMemoryCapture - Captures ALL tool execution results into persona memory
 *
 * Subscribes to the unified tool:result event and stores outcomes in the
 * initiating persona's long-term memory. This enables AIs to learn from their actions.
 *
 * Works with ANY tool that uses ToolResult.emit() - not just sentinels.
 */

import { Events } from '../core/shared/Events';
import { TOOL_EVENTS, type ToolResultEvent } from '../core/shared/ToolResult';
import { LongTermMemoryStore, type LongTermMemoryEntry } from '../user/server/modules/cognition/memory/LongTermMemoryStore';
import { v4 as uuid } from 'uuid';

// Cache of memory stores by personaId
const memoryStores = new Map<string, LongTermMemoryStore>();

function getMemoryStore(personaId: string): LongTermMemoryStore {
  let store = memoryStores.get(personaId);
  if (!store) {
    store = new LongTermMemoryStore(personaId, (msg) => console.log(`[ToolMemory] ${msg}`));
    memoryStores.set(personaId, store);
  }
  return store;
}

/**
 * Create a memory entry from any tool result
 */
function createToolMemory(event: ToolResultEvent): LongTermMemoryEntry {
  const now = Date.now();
  return {
    id: uuid(),
    personaId: event.userId!,
    domain: 'tool-use',
    contextId: null,
    thoughtType: 'tool-result',
    thoughtContent: `${event.tool} (${event.handle}): ${event.success ? 'SUCCESS' : 'FAILED'} - ${event.summary}`,
    importance: event.success ? 0.5 : 0.8, // Failures are more important to remember
    embedding: [], // TODO: Generate embedding for similarity search
    metadata: {
      tool: event.tool,
      handle: event.handle,
      success: event.success,
      error: event.error,
      durationMs: event.durationMs,
      ...event.data,
    },
    createdAt: now,
    consolidatedAt: now,
  };
}

let initialized = false;

/**
 * Initialize event subscriptions for tool result capture
 * Call this once at system startup
 */
export function initToolResultMemoryCapture(): void {
  if (initialized) {
    console.log('[ToolResultMemoryCapture] Already initialized, skipping');
    return;
  }

  console.log('[ToolResultMemoryCapture] Initializing unified tool result capture...');

  // Subscribe to unified tool:result event - captures ALL tools
  Events.subscribe(TOOL_EVENTS.RESULT, async (event: ToolResultEvent) => {
    if (!event.userId) {
      // No userId = can't store in persona memory
      return;
    }

    try {
      const memory = createToolMemory(event);
      const store = getMemoryStore(event.userId);
      await store.appendBatch([memory]);

      console.log(
        `[ToolMemory] ${event.tool} → ${event.success ? '✓' : '✗'} → persona ${event.userId.slice(0, 8)}`
      );
    } catch (error) {
      console.error(`[ToolResultMemoryCapture] Failed to store memory:`, error);
    }
  });

  // Also capture legacy sentinel events for backwards compatibility
  Events.subscribe('sentinel:complete', async (event: any) => {
    if (!event.userId) return;

    // Only capture if not already handled by tool:result
    // (ToolResult.emit sends both)
    const memory = createToolMemory({
      tool: `sentinel/${event.type}`,
      handle: event.handle,
      userId: event.userId,
      success: event.success,
      summary: event.data?.summary || 'Completed',
      data: event.data,
    });

    try {
      const store = getMemoryStore(event.userId);
      await store.appendBatch([memory]);
    } catch (error) {
      // Silently ignore duplicates
    }
  });

  Events.subscribe('sentinel:error', async (event: any) => {
    if (!event.userId) return;

    const memory = createToolMemory({
      tool: `sentinel/${event.type}`,
      handle: event.handle,
      userId: event.userId,
      success: false,
      summary: `Error: ${event.error}`,
      error: event.error,
    });

    try {
      const store = getMemoryStore(event.userId);
      await store.appendBatch([memory]);
    } catch (error) {
      // Silently ignore
    }
  });

  initialized = true;
  console.log('[ToolResultMemoryCapture] Ready - capturing all tool results');
}

/**
 * Query tool memories for a persona
 */
export async function queryToolMemories(
  personaId: string,
  options: { limit?: number; tool?: string; successOnly?: boolean } = {}
): Promise<LongTermMemoryEntry[]> {
  const store = getMemoryStore(personaId);

  // Use getByDomain to get all tool-use memories
  let filtered = await store.getByDomain('tool-use', options.limit ? options.limit * 2 : 100);

  if (options.tool) {
    filtered = filtered.filter(m => (m.metadata as any)?.tool === options.tool);
  }

  if (options.successOnly) {
    filtered = filtered.filter(m => (m.metadata as any)?.success === true);
  }

  if (options.limit) {
    filtered = filtered.slice(0, options.limit);
  }

  return filtered;
}

/**
 * Get memory stats for a persona
 */
export async function getToolMemoryStats(personaId: string): Promise<{
  total: number;
  byTool: Record<string, number>;
  successRate: number;
}> {
  const store = getMemoryStore(personaId);
  const memories = await store.getByDomain('tool-use');

  const byTool: Record<string, number> = {};
  let successCount = 0;

  for (const m of memories) {
    const tool = (m.metadata as any)?.tool || 'unknown';
    byTool[tool] = (byTool[tool] || 0) + 1;
    if ((m.metadata as any)?.success) successCount++;
  }

  return {
    total: memories.length,
    byTool,
    successRate: memories.length > 0 ? successCount / memories.length : 0,
  };
}
