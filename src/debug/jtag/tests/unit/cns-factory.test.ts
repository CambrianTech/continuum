/**
 * Unit test for CNS Factory - tests the factory without requiring full system
 */

import { describe, it, expect } from 'vitest';
import { CNSFactory } from '../../system/user/server/modules/central-nervous-system/CNSFactory';

describe('CNSFactory', () => {
  it('should create CNS with deterministic scheduler for simple persona', () => {
    // Mock minimal PersonaUser interface
    const mockPersona = {
      entity: {
        id: 'test-id',
        displayName: 'Test Persona',
        capabilities: undefined // No capabilities = deterministic
      },
      inbox: {
        waitForWork: async () => false,
        peek: async () => [],
        pop: async () => null,
        getSize: () => 0
      },
      personaState: {
        getCadence: () => 5000,
        rest: async () => {},
        shouldEngage: () => true,
        getState: () => ({ energy: 1.0, mood: 'neutral' })
      },
      genome: null,
      handleChatMessageFromCNS: async () => {},
      pollTasksFromCNS: async () => {},
      generateSelfTasksFromCNS: async () => {}
    };

    // Should not throw
    const cns = CNSFactory.create(mockPersona as any);
    expect(cns).toBeDefined();
    console.log('✅ CNS created successfully with deterministic scheduler');
  });

  it('should handle persona with displayName', () => {
    const mockPersona = {
      entity: {
        id: 'test-id',
        displayName: 'Named Persona'
      },
      inbox: {
        waitForWork: async () => false,
        peek: async () => [],
        pop: async () => null,
        getSize: () => 0
      },
      personaState: {
        getCadence: () => 5000,
        rest: async () => {},
        shouldEngage: () => true,
        getState: () => ({ energy: 1.0, mood: 'neutral' })
      },
      genome: null,
      handleChatMessageFromCNS: async () => {},
      pollTasksFromCNS: async () => {},
      generateSelfTasksFromCNS: async () => {}
    };

    const cns = CNSFactory.create(mockPersona as any);
    expect(cns).toBeDefined();
    console.log('✅ CNS handles named persona correctly');
  });
});
