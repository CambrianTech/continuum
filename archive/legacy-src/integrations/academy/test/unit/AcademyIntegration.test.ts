/**
 * Academy Integration - Unit Tests
 * 
 * Tests the Academy Integration module's core functionality including:
 * - Initialization and shutdown lifecycle
 * - Daemon coordination and health monitoring
 * - Training session management
 * - Persona spawning capabilities
 * - Type safety and error handling
 */

import { strict as assert } from 'assert';
import { describe, it, before, after, beforeEach } from 'node:test';
import { AcademyIntegration, ACADEMY_MODULE_DEPENDENCIES } from '../../AcademyIntegration.js';
import { TrainingSessionParams, PersonaSpawnParams } from '../../types.js';
import { ModuleUtils } from '../../../core/modules/index.js';

describe('AcademyIntegration', () => {
  let academyIntegration: AcademyIntegration;

  beforeEach(() => {
    academyIntegration = new AcademyIntegration({
      local_mode: true,
      p2p_enabled: false,
      max_concurrent_sessions: 2,
      training_data_path: '.continuum/test/academy/training',
      model_cache_path: '.continuum/test/academy/models'
    });
  });

  describe('Configuration Management', () => {
    it('should initialize with default configuration', () => {
      const defaultIntegration = new AcademyIntegration();
      assert.equal(defaultIntegration['config'].local_mode, true);
      assert.equal(defaultIntegration['config'].p2p_enabled, false);
      assert.equal(defaultIntegration['config'].max_concurrent_sessions, 3);
    });

    it('should merge custom configuration with defaults', () => {
      const customConfig = {
        max_concurrent_sessions: 5,
        p2p_enabled: true
      };
      const integration = new AcademyIntegration(customConfig);
      
      assert.equal(integration['config'].max_concurrent_sessions, 5);
      assert.equal(integration['config'].p2p_enabled, true);
      assert.equal(integration['config'].local_mode, true); // Default preserved
    });

    it('should have proper module metadata', () => {
      assert.equal(academyIntegration.name, 'academy-integration');
      assert.equal(academyIntegration.version, '1.0.0');
      assert.ok(academyIntegration.daemonType);
    });
  });

  describe('Lifecycle Management', () => {
    it('should handle multiple initialization calls gracefully', async () => {
      // Mock the daemon dependencies using core module utilities
      academyIntegration['daemons'] = ModuleUtils.createMockInstances(ACADEMY_MODULE_DEPENDENCIES);

      // Mock the sendMessage method to simulate successful health checks
      academyIntegration['sendMessage'] = async () => ({ success: true, data: {} });

      let initializationCount = 0;
      const originalInit = academyIntegration['daemons']['academy'].start;
      academyIntegration['daemons']['academy'].start = async () => {
        initializationCount++;
        return originalInit.call(academyIntegration['daemons']['academy']);
      };

      // First initialization
      await academyIntegration.initialize();
      assert.equal(initializationCount, 1);

      // Second initialization should be idempotent
      await academyIntegration.initialize();
      assert.equal(initializationCount, 1); // Should not increment
    });

    it('should clean up properly on initialization failure', async () => {
      // Mock failing daemon using core module utilities with overrides
      academyIntegration['daemons'] = ModuleUtils.createMockInstances(ACADEMY_MODULE_DEPENDENCIES, {
        academy: {
          start: async () => { throw new Error('Daemon startup failed'); },
          stop: async () => { /* cleanup */ }
        }
      });

      try {
        await academyIntegration.initialize();
        assert.fail('Should have thrown initialization error');
      } catch (error) {
        assert.ok(error instanceof Error);
        assert.ok(error.message.includes('Daemon startup failed'));
      }

      // Verify cleanup was attempted
      assert.equal(academyIntegration['isInitialized'], false);
    });

    it('should shutdown daemons in reverse dependency order', async () => {
      const shutdownOrder: string[] = [];
      
      // Use module utilities to create mocks that track shutdown order
      academyIntegration['daemons'] = ModuleUtils.createMockInstances(ACADEMY_MODULE_DEPENDENCIES, {
        academy: { stop: async () => shutdownOrder.push('academy') },
        persona: { stop: async () => shutdownOrder.push('persona') },
        database: { stop: async () => shutdownOrder.push('database') }
      });
      
      await academyIntegration.shutdown();
      
      // Should shutdown in reverse startup order (calculated by ModuleUtils)
      const expectedOrder = ModuleUtils.calculateShutdownOrder(ACADEMY_MODULE_DEPENDENCIES);
      assert.deepEqual(shutdownOrder, expectedOrder);
    });
  });

  describe('Integration Status Monitoring', () => {
    beforeEach(() => {
      // Mock healthy daemons using core module utilities
      academyIntegration['daemons'] = ModuleUtils.createMockInstances(ACADEMY_MODULE_DEPENDENCIES);
      
      // Mock the sendMessage method to simulate successful health checks
      academyIntegration['sendMessage'] = async () => ({ success: true, data: {} });
    });

    it('should report healthy status when all daemons are running', async () => {
      const status = await academyIntegration.getIntegrationStatus();
      
      assert.equal(status.integration_health, 'healthy');
      assert.equal(status.academy_daemon, 'running');
      assert.equal(status.persona_daemon, 'running');
      assert.equal(status.database_daemon, 'running');
    });

    it('should report degraded status when some daemons fail', async () => {
      // Mock sendMessage to fail for academy daemon specifically
      academyIntegration['sendMessage'] = async (target: any, type: string, data: any) => {
        if (target === 'academy') {
          return { success: false, error: 'Academy daemon failed' };
        }
        return { success: true, data: {} };
      };
      
      const status = await academyIntegration.getIntegrationStatus();
      
      assert.equal(status.integration_health, 'degraded');
      assert.equal(status.academy_daemon, 'stopped');
      assert.equal(status.persona_daemon, 'running');
      assert.equal(status.database_daemon, 'running');
    });

    it('should report failed status when all critical daemons are down', async () => {
      // Mock sendMessage to fail for all daemons
      academyIntegration['sendMessage'] = async () => ({ success: false, error: 'All daemons failed' });
      
      const status = await academyIntegration.getIntegrationStatus();
      
      assert.equal(status.integration_health, 'failed');
      assert.equal(status.academy_daemon, 'stopped');
      assert.equal(status.persona_daemon, 'stopped');
      assert.equal(status.database_daemon, 'stopped');
    });
  });

  describe('Training Session Management', () => {
    beforeEach(async () => {
      // Setup integration with mocked daemons using core module utilities
      academyIntegration['daemons'] = ModuleUtils.createMockInstances(ACADEMY_MODULE_DEPENDENCIES);
      academyIntegration['isInitialized'] = true;
    });

    it('should start training session with valid parameters', async () => {
      const params: TrainingSessionParams = {
        student_persona: 'test-persona',
        trainer_mode: 'reinforcement',
        evolution_target: 'code-generation',
        vector_exploration: true,
        learning_rate: 0.001,
        max_epochs: 100
      };

      // Mock successful training session response
      const mockResponse = {
        success: true,
        data: {
          session_id: 'session-123',
          student_persona: 'test-persona',
          trainer_mode: 'reinforcement',
          evolution_target: 'code-generation',
          vector_exploration: true,
          status: 'initializing',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString()
        }
      };

      academyIntegration['sendMessage'] = async () => mockResponse;

      const result = await academyIntegration.startTrainingSession(params);
      
      assert.equal(result.session_id, 'session-123');
      assert.equal(result.student_persona, 'test-persona');
      assert.equal(result.status, 'initializing');
    });

    it('should throw error when not initialized', async () => {
      academyIntegration['isInitialized'] = false;
      
      const params: TrainingSessionParams = {
        student_persona: 'test-persona'
      };

      try {
        await academyIntegration.startTrainingSession(params);
        assert.fail('Should have thrown initialization error');
      } catch (error) {
        assert.ok(error instanceof Error);
        assert.ok(error.message.includes('not initialized'));
      }
    });

    it('should handle training session failures gracefully', async () => {
      const params: TrainingSessionParams = {
        student_persona: 'test-persona'
      };

      // Mock failed response
      academyIntegration['sendMessage'] = async () => ({
        success: false,
        error: 'Training resource unavailable'
      });

      try {
        await academyIntegration.startTrainingSession(params);
        assert.fail('Should have thrown training error');
      } catch (error) {
        assert.ok(error instanceof Error);
        assert.ok(error.message.includes('Training resource unavailable'));
      }
    });
  });

  describe('Persona Management', () => {
    beforeEach(() => {
      academyIntegration['isInitialized'] = true;
    });

    it('should spawn persona with complete parameters', async () => {
      const params: PersonaSpawnParams = {
        persona_name: 'TestPersona',
        base_model: 'gpt-4',
        specialization: 'code-generation',
        skill_vector: [0.8, 0.9, 0.7],
        p2p_seed: true,
        capabilities: ['coding', 'debugging', 'testing'],
        initial_training_data: 'sample-data-path'
      };

      const mockResponse = {
        success: true,
        data: {
          persona_id: 'persona-456',
          persona_name: 'TestPersona',
          base_model: 'gpt-4',
          specialization: 'code-generation',
          skill_vector: [0.8, 0.9, 0.7],
          status: 'spawning',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          capabilities: ['coding', 'debugging', 'testing']
        }
      };

      academyIntegration['sendMessage'] = async () => mockResponse;

      const result = await academyIntegration.spawnPersona(params);
      
      assert.equal(result.persona_id, 'persona-456');
      assert.equal(result.persona_name, 'TestPersona');
      assert.equal(result.specialization, 'code-generation');
      assert.deepEqual(result.capabilities, ['coding', 'debugging', 'testing']);
    });

    it('should spawn persona with minimal parameters', async () => {
      const params: PersonaSpawnParams = {
        persona_name: 'MinimalPersona'
      };

      const mockResponse = {
        success: true,
        data: {
          persona_id: 'persona-789',
          persona_name: 'MinimalPersona',
          base_model: 'default',
          specialization: 'general',
          skill_vector: [],
          status: 'spawning',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          capabilities: []
        }
      };

      academyIntegration['sendMessage'] = async () => mockResponse;

      const result = await academyIntegration.spawnPersona(params);
      
      assert.equal(result.persona_name, 'MinimalPersona');
      assert.equal(result.base_model, 'default');
    });
  });

  describe('Error Handling and Type Safety', () => {
    it('should handle message handler errors gracefully', async () => {
      const invalidMessage = {
        type: 'nonexistent-command',
        data: {}
      } as any;

      const response = await academyIntegration.handleMessage(invalidMessage);
      
      assert.equal(response.success, false);
      assert.ok(response.error?.includes('Unknown message type'));
    });

    it('should maintain type safety in all public methods', () => {
      // Verify return types are properly typed (TypeScript compilation check)
      const integration = new AcademyIntegration();
      
      // These should compile without type assertions
      const statusPromise: Promise<any> = integration.getIntegrationStatus();
      const academyStatusPromise: Promise<any> = integration.getAcademyStatus();
      
      assert.ok(statusPromise instanceof Promise);
      assert.ok(academyStatusPromise instanceof Promise);
    });
  });

  describe('Daemon Access and Coordination', () => {
    it('should provide read-only access to internal daemons', () => {
      // Setup mocked daemons
      academyIntegration['daemons'] = ModuleUtils.createMockInstances(ACADEMY_MODULE_DEPENDENCIES);
      
      const academy = academyIntegration.academy;
      const persona = academyIntegration.persona;
      const database = academyIntegration.database;
      
      assert.ok(academy);
      assert.ok(persona);
      assert.ok(database);
      
      // Verify they are the same instances
      assert.equal(academy, academyIntegration['daemons'].academy);
      assert.equal(persona, academyIntegration['daemons'].persona);
      assert.equal(database, academyIntegration['daemons'].database);
    });

    it('should provide generic daemon access using dependency names', () => {
      // Setup mocked daemons
      academyIntegration['daemons'] = ModuleUtils.createMockInstances(ACADEMY_MODULE_DEPENDENCIES);
      
      // Test generic daemon access
      const academyDaemon = academyIntegration.getDaemon('academy');
      const personaDaemon = academyIntegration.getDaemon('persona');
      const databaseDaemon = academyIntegration.getDaemon('database');
      
      assert.ok(academyDaemon);
      assert.ok(personaDaemon);
      assert.ok(databaseDaemon);
      
      // Verify consistency with getters
      assert.equal(academyDaemon, academyIntegration.academy);
      assert.equal(personaDaemon, academyIntegration.persona);
      assert.equal(databaseDaemon, academyIntegration.database);
    });
  });
});