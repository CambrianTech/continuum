/**
 * Academy Integration - Integration Tests
 * 
 * Tests the Academy Integration module's interaction with:
 * - Real daemon communication patterns
 * - Inter-daemon message routing
 * - System-level behavior validation
 * - Resource management and cleanup
 */

import { strict as assert } from 'assert';
import { describe, it, before, after, beforeEach, afterEach } from 'node:test';
import { AcademyIntegration } from '../../AcademyIntegration.js';
import { DaemonType } from '../../../../daemons/base/DaemonTypes.js';
import { TrainingSessionParams, PersonaSpawnParams } from '../../types.js';

describe('AcademyIntegration - Integration Tests', () => {
  let academyIntegration: AcademyIntegration;
  let testCleanupTasks: (() => Promise<void>)[] = [];

  beforeEach(() => {
    academyIntegration = new AcademyIntegration({
      local_mode: true,
      p2p_enabled: false,
      max_concurrent_sessions: 1,
      training_data_path: '.continuum/test/integration/academy/training',
      model_cache_path: '.continuum/test/integration/academy/models',
      evaluation_interval_ms: 1000 // Faster for testing
    });
    testCleanupTasks = [];
  });

  afterEach(async () => {
    // Clean up any test resources
    for (const cleanup of testCleanupTasks.reverse()) {
      try {
        await cleanup();
      } catch (error) {
        console.warn('Test cleanup warning:', error);
      }
    }
    
    // Ensure integration is properly shut down
    try {
      await academyIntegration.shutdown();
    } catch (error) {
      // Ignore shutdown errors in tests
    }
  });

  describe('Inter-Daemon Communication', () => {
    it('should route messages between daemons correctly', async () => {
      // This test verifies that the integration properly coordinates between
      // different daemon types using the message routing system
      
      const messageLog: any[] = [];
      
      // Mock the sendMessage method to capture inter-daemon communication
      const originalSendMessage = academyIntegration['sendMessage'];
      academyIntegration['sendMessage'] = async (target: any, type: string, data: any) => {
        messageLog.push({ target, type, data, timestamp: Date.now() });
        
        // Mock responses based on message type
        switch (type) {
          case 'get_capabilities':
            return { success: true, data: { capabilities: ['training', 'evaluation'] } };
          case 'get_comprehensive_status':
            return { 
              success: true, 
              data: { 
                active_sessions: 0, 
                queue_length: 0,
                status: 'operational' 
              } 
            };
          default:
            return { success: false, error: `Unknown message type: ${type}` };
        }
      };
      
      testCleanupTasks.push(async () => {
        academyIntegration['sendMessage'] = originalSendMessage;
      });

      // Trigger operations that require inter-daemon communication
      const status = await academyIntegration.getIntegrationStatus();
      
      // Verify messages were sent to appropriate daemon types
      assert.ok(messageLog.length > 0, 'Should have sent inter-daemon messages');
      
      // Verify message routing follows expected patterns
      const capabilityChecks = messageLog.filter(msg => msg.type === 'get_capabilities');
      assert.ok(capabilityChecks.length >= 3, 'Should check capabilities of all critical daemons');
      
      assert.ok(status.integration_health, 'Integration status should be determined');
    });

    it('should handle daemon communication failures gracefully', async () => {
      // Mock failing daemon communication
      academyIntegration['sendMessage'] = async () => {
        throw new Error('Daemon communication timeout');
      };

      const status = await academyIntegration.getIntegrationStatus();
      
      // Should handle communication failures without crashing
      assert.equal(status.integration_health, 'failed');
      assert.equal(status.academy_daemon, 'stopped');
      assert.equal(status.persona_daemon, 'stopped');
      assert.equal(status.database_daemon, 'stopped');
    });
  });

  describe('Resource Management Integration', () => {
    it('should manage training session resources properly', async () => {
      let activeSessionCount = 0;
      const maxSessions = academyIntegration['config'].max_concurrent_sessions;
      
      // Mock session management
      academyIntegration['sendMessage'] = async (target: any, type: string, data: any) => {
        if (type === 'start_evolution_session') {
          if (activeSessionCount >= maxSessions) {
            return { 
              success: false, 
              error: 'Maximum concurrent sessions reached' 
            };
          }
          
          activeSessionCount++;
          const sessionId = `session-${activeSessionCount}-${Date.now()}`;
          
          return {
            success: true,
            data: {
              session_id: sessionId,
              student_persona: data.student_persona,
              status: 'initializing',
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString()
            }
          };
        }
        return { success: true, data: {} };
      };
      
      academyIntegration['isInitialized'] = true;

      const params: TrainingSessionParams = {
        student_persona: 'test-persona-1',
        trainer_mode: 'reinforcement'
      };

      // Should successfully create session within limits
      const session1 = await academyIntegration.startTrainingSession(params);
      assert.ok(session1.session_id);
      assert.equal(activeSessionCount, 1);

      // Should handle resource limits
      if (maxSessions === 1) {
        try {
          await academyIntegration.startTrainingSession({
            student_persona: 'test-persona-2'
          });
          assert.fail('Should have hit session limit');
        } catch (error) {
          assert.ok(error instanceof Error);
          assert.ok(error.message.includes('Maximum concurrent sessions'));
        }
      }
    });

    it('should coordinate persona spawning with resource availability', async () => {
      let spawnedPersonas: string[] = [];
      
      academyIntegration['sendMessage'] = async (target: any, type: string, data: any) => {
        if (type === 'spawn_persona') {
          const personaId = `persona-${spawnedPersonas.length + 1}-${Date.now()}`;
          spawnedPersonas.push(personaId);
          
          return {
            success: true,
            data: {
              persona_id: personaId,
              persona_name: data.persona_name,
              base_model: data.base_model || 'default',
              specialization: data.specialization || 'general',
              skill_vector: data.skill_vector || [],
              status: 'spawning',
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
              capabilities: data.capabilities || []
            }
          };
        }
        return { success: true, data: {} };
      };
      
      academyIntegration['isInitialized'] = true;

      const params: PersonaSpawnParams = {
        persona_name: 'IntegrationTestPersona',
        specialization: 'testing',
        capabilities: ['integration-testing', 'validation']
      };

      const persona = await academyIntegration.spawnPersona(params);
      
      assert.equal(persona.persona_name, 'IntegrationTestPersona');
      assert.equal(persona.specialization, 'testing');
      assert.equal(spawnedPersonas.length, 1);
      assert.ok(spawnedPersonas.includes(persona.persona_id));
    });
  });

  describe('System-Level Behavior', () => {
    it('should maintain consistency across multiple operations', async () => {
      const operationLog: any[] = [];
      
      academyIntegration['sendMessage'] = async (target: any, type: string, data: any) => {
        operationLog.push({ target, type, data, timestamp: Date.now() });
        
        // Simulate realistic responses
        switch (type) {
          case 'get_comprehensive_status':
            return {
              success: true,
              data: {
                active_sessions: operationLog.filter(op => op.type === 'start_evolution_session').length,
                queue_length: 0,
                status: 'operational'
              }
            };
          case 'get_capabilities':
            return { success: true, data: { capabilities: ['training', 'evaluation'] } };
          default:
            return { success: true, data: {} };
        }
      };
      
      academyIntegration['isInitialized'] = true;

      // Perform multiple operations
      const status1 = await academyIntegration.getIntegrationStatus();
      const academyStatus = await academyIntegration.getAcademyStatus();
      const status2 = await academyIntegration.getIntegrationStatus();

      // Verify consistency
      assert.equal(status1.integration_health, status2.integration_health);
      assert.ok(academyStatus.academy_daemon);
      assert.ok(operationLog.length > 0);
      
      // Verify operations are properly sequenced
      const timestamps = operationLog.map(op => op.timestamp);
      const sortedTimestamps = [...timestamps].sort((a, b) => a - b);
      assert.deepEqual(timestamps, sortedTimestamps, 'Operations should be properly sequenced');
    });

    it('should handle concurrent operations safely', async () => {
      let operationCount = 0;
      
      academyIntegration['sendMessage'] = async (target: any, type: string, data: any) => {
        operationCount++;
        // Simulate async operation delay
        await new Promise(resolve => setTimeout(resolve, 10));
        
        return {
          success: true,
          data: {
            operation_id: operationCount,
            status: 'completed'
          }
        };
      };
      
      academyIntegration['isInitialized'] = true;

      // Start multiple concurrent operations
      const operations = [
        academyIntegration.getIntegrationStatus(),
        academyIntegration.getIntegrationStatus(),
        academyIntegration.getIntegrationStatus()
      ];

      const results = await Promise.all(operations);
      
      // All operations should complete successfully
      assert.equal(results.length, 3);
      for (const result of results) {
        assert.ok(result.integration_health !== undefined);
      }
      
      // Verify that concurrent operations don't interfere with each other
      assert.ok(operationCount >= 3, 'All operations should have been processed');
    });
  });

  describe('Error Recovery and Resilience', () => {
    it('should recover from transient daemon failures', async () => {
      let failureCount = 0;
      const maxFailures = 2;
      
      academyIntegration['sendMessage'] = async (target: any, type: string, data: any) => {
        // Simulate transient failures
        if (type === 'get_capabilities' && failureCount < maxFailures) {
          failureCount++;
          throw new Error('Transient daemon failure');
        }
        
        return { success: true, data: { capabilities: ['training'] } };
      };

      // First calls should fail (some daemons still working, so degraded not failed)
      let status = await academyIntegration.getIntegrationStatus();
      assert.equal(status.integration_health, 'degraded');
      
      // After transient failures pass, should recover
      status = await academyIntegration.getIntegrationStatus();
      assert.equal(status.integration_health, 'healthy');
      
      assert.equal(failureCount, maxFailures, 'Should have experienced expected failures');
    });

    it('should maintain state consistency during partial failures', async () => {
      academyIntegration['sendMessage'] = async (target: any, type: string, data: any) => {
        // Simulate partial system failure - academy daemon fails, others work
        if (target === DaemonType.ACADEMY) {
          return { success: false, error: 'Academy daemon temporarily unavailable' };
        }
        
        return { success: true, data: { capabilities: ['basic'] } };
      };

      const status = await academyIntegration.getIntegrationStatus();
      
      // Should report degraded but not failed (some daemons still working)
      assert.equal(status.integration_health, 'degraded');
      assert.equal(status.academy_daemon, 'stopped');
      assert.equal(status.persona_daemon, 'running');
      assert.equal(status.database_daemon, 'running');
    });
  });

  describe('Configuration and Environment Integration', () => {
    it('should adapt behavior based on configuration', async () => {
      // Test with P2P enabled configuration
      const p2pIntegration = new AcademyIntegration({
        local_mode: false,
        p2p_enabled: true,
        max_concurrent_sessions: 5
      });
      
      testCleanupTasks.push(async () => {
        try {
          await p2pIntegration.shutdown();
        } catch (error) {
          // Ignore cleanup errors
        }
      });

      p2pIntegration['sendMessage'] = async (target: any, type: string, data: any) => {
        if (type === 'get_comprehensive_status') {
          // Should include P2P-specific data when enabled
          assert.equal(data.include_p2p, true, 'Should request P2P data when enabled');
        }
        
        return { success: true, data: {} };
      };
      
      p2pIntegration['isInitialized'] = true;
      
      await p2pIntegration.getAcademyStatus();
      // Verification happens in the mocked sendMessage function
    });

    it('should handle environment-specific resource paths', async () => {
      const customPaths = {
        training_data_path: '.continuum/custom/training',
        model_cache_path: '.continuum/custom/models'
      };
      
      const customIntegration = new AcademyIntegration(customPaths);
      
      testCleanupTasks.push(async () => {
        try {
          await customIntegration.shutdown();
        } catch (error) {
          // Ignore cleanup errors
        }
      });
      
      assert.equal(customIntegration['config'].training_data_path, customPaths.training_data_path);
      assert.equal(customIntegration['config'].model_cache_path, customPaths.model_cache_path);
    });
  });
});