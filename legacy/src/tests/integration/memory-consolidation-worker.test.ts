/**
 * Integration test for MemoryConsolidationWorker
 *
 * Tests pattern detection, consolidation, and activation
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { MemoryConsolidationWorker } from '../../system/user/server/modules/cognition/memory/MemoryConsolidationWorker';
import { WorkingMemoryManager } from '../../system/user/server/modules/cognition/memory/WorkingMemoryManager';
import { LongTermMemoryStore } from '../../system/user/server/modules/cognition/memory/LongTermMemoryStore';
import { PersonaInbox } from '../../system/user/server/modules/PersonaInbox';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

describe('MemoryConsolidationWorker', () => {
  let personaId: UUID;
  let inbox: PersonaInbox;
  let worker: MemoryConsolidationWorker;
  let workingMemory: WorkingMemoryManager;
  let longTermMemory: LongTermMemoryStore;

  beforeEach(() => {
    personaId = generateUUID();
    inbox = new PersonaInbox(personaId);
    workingMemory = new WorkingMemoryManager(personaId);
    longTermMemory = new LongTermMemoryStore(personaId);

    worker = new MemoryConsolidationWorker(personaId, inbox, {
      minSimilarity: 0.7,
      minClusterSize: 2,
      minClusterStrength: 0.75,
      minImportance: 0.5,
      activationThreshold: 0.75
    });
  });

  afterEach(async () => {
    await worker.stop();
    await workingMemory.clear();
    await longTermMemory.clear();
  });

  it('should instantiate successfully', () => {
    expect(worker).toBeDefined();
    const status = worker.getStatus();
    expect(status.running).toBe(false);
    expect(status.personaId).toBe(personaId);
  });

  it('should start and stop gracefully', async () => {
    await worker.start();
    expect(worker.getStatus().running).toBe(true);

    await worker.stop();
    // Give it time to stop
    await new Promise(resolve => setTimeout(resolve, 200));
    expect(worker.getStatus().running).toBe(false);
  });

  it('should handle empty inbox and memory', async () => {
    await worker.start();

    // Wait for one cycle
    await new Promise(resolve => setTimeout(resolve, 150));

    // Should not crash with empty state
    expect(worker.getStatus().running).toBe(true);

    await worker.stop();
  });

  it('should not crash with working memories', async () => {
    // Add some working memories
    await workingMemory.store({
      domain: 'test',
      contextId: null,
      thoughtType: 'observation',
      thoughtContent: 'Test thought 1',
      importance: 0.8,
      shareable: true
    });

    await workingMemory.store({
      domain: 'test',
      contextId: null,
      thoughtType: 'reflection',
      thoughtContent: 'Test thought 2',
      importance: 0.7,
      shareable: true
    });

    await worker.start();

    // Wait for one cycle
    await new Promise(resolve => setTimeout(resolve, 150));

    expect(worker.getStatus().running).toBe(true);

    await worker.stop();
  });

  it('should expose correct configuration', () => {
    const status = worker.getStatus();
    expect(status.options.minSimilarity).toBe(0.7);
    expect(status.options.minClusterSize).toBe(2);
    expect(status.options.minClusterStrength).toBe(0.75);
    expect(status.options.minImportance).toBe(0.5);
    expect(status.options.activationThreshold).toBe(0.75);
  });

  it('should use default options when not specified', () => {
    const defaultWorker = new MemoryConsolidationWorker(personaId, inbox);
    const status = defaultWorker.getStatus();

    expect(status.options.minSimilarity).toBe(0.75);
    expect(status.options.minClusterSize).toBe(3);
    expect(status.options.minClusterStrength).toBe(0.8);
    expect(status.options.minImportance).toBe(0.6);
    expect(status.options.activationThreshold).toBe(0.8);
  });
});
