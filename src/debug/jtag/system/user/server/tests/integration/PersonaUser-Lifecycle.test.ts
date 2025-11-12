/**
 * PersonaUser Lifecycle - Baseline Integration Test
 *
 * PURPOSE: Prove that PersonaUser works RIGHT NOW before refactoring.
 * This test establishes a baseline - if this passes before refactoring and after,
 * we know we didn't break the core initialization and lifecycle.
 *
 * IMPORTANT: This is a SLOW test (requires full system initialization).
 * Run sparingly - only at phase boundaries.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { PersonaUser } from '../../PersonaUser';
import { UserEntity } from '../../../../data/entities/UserEntity';
import { UserStateEntity } from '../../../../data/entities/UserStateEntity';
import { MemoryStateBackend } from '../../../storage/MemoryStateBackend';
import type { UUID } from '../../../../core/types/CrossPlatformUUID';

describe('PersonaUser Lifecycle (Baseline)', () => {
  let personaUser: PersonaUser;
  let testEntity: UserEntity;
  let testState: UserStateEntity;
  let storage: MemoryStateBackend;

  beforeAll(async () => {
    // Create minimal test entities
    testEntity = {
      id: 'test-persona-baseline' as UUID,
      uniqueId: '@test-persona-baseline',
      displayName: 'Test Persona (Baseline)',
      type: 'persona',
      modelConfig: {
        provider: 'ollama',
        model: 'llama3.2',
        capabilities: ['text']
      },
      capabilities: ['text'],
      preferences: {},
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString()
    };

    testState = {
      userId: testEntity.id,
      currentTabId: null,
      openContentIds: [],
      theme: 'system',
      lastUpdated: new Date().toISOString()
    };

    storage = new MemoryStateBackend();
  });

  afterAll(async () => {
    if (personaUser) {
      try {
        await personaUser.shutdown();
      } catch (error) {
        // Ignore shutdown errors in tests
        console.warn('PersonaUser shutdown error (ignoring):', error);
      }
    }
  });

  it('should construct without errors', () => {
    personaUser = new PersonaUser(
      testEntity,
      testState,
      storage,
      undefined // No client needed for baseline test
    );

    expect(personaUser).toBeDefined();
    expect(personaUser.id).toBe(testEntity.id);
    expect(personaUser.type).toBe('persona');
    expect(personaUser.displayName).toBe(testEntity.displayName);
    expect(personaUser.entity).toBe(testEntity);
  });

  it('should have required modules after construction', () => {
    // Check that internal modules exist (after Phase 2 refactoring)
    // We access them via bracket notation to bypass TypeScript private checking
    expect(personaUser['inbox']).toBeDefined();
    expect(personaUser['personaState']).toBeDefined();
    expect(personaUser['memory']).toBeDefined(); // Phase 2: genome → memory
    expect(personaUser['memory']['genome']).toBeDefined(); // genome is now inside memory
    expect(personaUser['rateLimiter']).toBeDefined();
    // CNS is initialized in initialize(), not constructor
  });

  it('should initialize successfully', async () => {
    await expect(personaUser.initialize()).resolves.not.toThrow();

    // After initialization, CNS should exist
    expect(personaUser['cns']).toBeDefined();
  });

  it('should have CNS initialized after initialize()', () => {
    expect(personaUser['cns']).toBeDefined();
    expect(personaUser['cns']).not.toBeNull();
  });

  it('should shutdown gracefully', async () => {
    await expect(personaUser.shutdown()).resolves.not.toThrow();
  });
});
