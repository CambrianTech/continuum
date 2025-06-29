/**
 * ProcessCoordinator Integration Test
 * Test the multi-process daemon system
 */

import { ProcessCoordinator } from './ProcessCoordinator.js';
import { ProcessMessage } from '../interfaces/IProcessCoordinator.js';

describe('ProcessCoordinator Integration', () => {
  let coordinator: ProcessCoordinator;

  beforeEach(async () => {
    coordinator = new ProcessCoordinator(['src/process/daemons']);
    await coordinator.start();
  });

  afterEach(async () => {
    try {
      await coordinator.stop();
    } catch {
      // Ignore cleanup errors
    }
  });

  describe('Basic Functionality', () => {
    test('should start and discover daemon types', async () => {
      const available = coordinator.getAvailable();
      expect(Array.isArray(available)).toBe(true);
      
      // Should discover the version daemon we created
      expect(available.includes('version')).toBe(true);
    }, 10000);

    test('should spawn version daemon process', async () => {
      const processId = await coordinator.spawn('version');
      
      expect(processId).toMatch(/^version-\d+-\w+$/);
      
      const status = coordinator.getSystemStatus();
      expect(status.coordinator.processCount).toBe(1);
      expect(status.processes[0].daemonType).toBe('version');
    }, 15000);

    test('should communicate with spawned daemon', async () => {
      const processId = await coordinator.spawn('version');
      
      const message: ProcessMessage = {
        id: 'test-version-request',
        type: 'version',
        data: {},
        timestamp: Date.now(),
        targetProcess: processId
      };

      const result = await coordinator.route(message);
      
      expect(result.success).toBe(true);
      expect(result.data.daemon).toBe('VersionDaemon');
      expect(result.data.version).toBeDefined();
      expect(result.processId).toBe(processId);
    }, 15000);

    test('should route messages by capability', async () => {
      await coordinator.spawn('version');
      
      const message: ProcessMessage = {
        id: 'test-info-request',
        type: 'info', // This capability is provided by version daemon
        data: {},
        timestamp: Date.now()
      };

      const result = await coordinator.route(message);
      
      expect(result.success).toBe(true);
      expect(result.data.daemon).toBe('version');
      expect(result.data.capabilities).toContain('info');
    }, 15000);

    test('should perform health checks', async () => {
      const processId = await coordinator.spawn('version');
      
      const healthChecks = await coordinator.healthCheck();
      
      expect(healthChecks.length).toBe(1);
      expect(healthChecks[0].processId).toBe(processId);
      expect(healthChecks[0].status).toBe('healthy');
      expect(healthChecks[0].uptime).toBeGreaterThan(0);
    }, 15000);

    test('should kill daemon process', async () => {
      const processId = await coordinator.spawn('version');
      
      // Verify process is running
      let status = coordinator.getSystemStatus();
      expect(status.coordinator.processCount).toBe(1);
      
      await coordinator.kill(processId);
      
      // Verify process is gone
      status = coordinator.getSystemStatus();
      expect(status.coordinator.processCount).toBe(0);
    }, 15000);

    test('should restart daemon process', async () => {
      const processId = await coordinator.spawn('version');
      
      // Get initial uptime
      let healthBefore = await coordinator.healthCheck();
      expect(healthBefore[0].processId).toBe(processId);
      
      await coordinator.restart(processId);
      
      // Process should still exist but with new uptime
      const status = coordinator.getSystemStatus();
      expect(status.coordinator.processCount).toBe(1);
      
      // New process should have different ID
      const newProcessId = status.processes[0].processId;
      expect(newProcessId).not.toBe(processId);
    }, 20000);
  });

  describe('Error Handling', () => {
    test('should handle spawn of unknown daemon type', async () => {
      await expect(coordinator.spawn('non-existent-daemon'))
        .rejects.toThrow('Unknown daemon type: non-existent-daemon');
    });

    test('should handle kill of non-existent process', async () => {
      // Should not throw - just log warning
      await expect(coordinator.kill('non-existent-process'))
        .resolves.not.toThrow();
    });

    test('should handle routing to non-existent capability', async () => {
      const message: ProcessMessage = {
        id: 'test-unknown',
        type: 'unknown-capability',
        data: {},
        timestamp: Date.now()
      };

      const result = await coordinator.route(message);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('No process available');
    });
  });
});

// Manual test helper for CLI debugging
export async function runManualTest() {
  console.log('🧪 Running manual ProcessCoordinator test...');
  
  const coordinator = new ProcessCoordinator(['src/process/daemons']);
  await coordinator.start();
  
  try {
    console.log('📦 Available daemons:', coordinator.getAvailable());
    
    const processId = await coordinator.spawn('version');
    console.log('🚀 Spawned process:', processId);
    
    const versionResult = await coordinator.route({
      id: 'manual-test',
      type: 'version',
      data: {},
      timestamp: Date.now()
    });
    console.log('📋 Version result:', versionResult);
    
    const health = await coordinator.healthCheck();
    console.log('🏥 Health check:', health);
    
    console.log('✅ Manual test completed successfully');
    
  } catch (error) {
    console.error('❌ Manual test failed:', error);
  } finally {
    await coordinator.stop();
  }
}

// Run manual test if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runManualTest().catch(console.error);
}