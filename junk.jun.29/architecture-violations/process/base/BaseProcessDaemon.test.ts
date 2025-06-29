/**
 * BaseProcessDaemon Tests
 * Comprehensive test suite for base daemon functionality
 */

import { BaseProcessDaemon } from './BaseProcessDaemon.js';
import { ProcessMessage, ProcessResult } from '../interfaces/IProcessCoordinator.js';

// Test implementation
class TestDaemon extends BaseProcessDaemon {
  readonly daemonType = 'test-daemon';
  readonly capabilities = ['test', 'demo'];
  
  private startCalled = false;
  private stopCalled = false;

  protected async onStart(): Promise<void> {
    this.startCalled = true;
  }

  protected async onStop(): Promise<void> {
    this.stopCalled = true;
  }

  protected async onMessage(message: ProcessMessage): Promise<ProcessResult> {
    return {
      success: true,
      data: { echo: message.data },
      processId: this.processId
    };
  }

  protected async sendToTarget(targetProcess: string, message: ProcessMessage): Promise<void> {
    // Mock implementation
    console.log(`Mock send to ${targetProcess}:`, message);
  }

  // Test accessors
  get wasStartCalled() { return this.startCalled; }
  get wasStopCalled() { return this.stopCalled; }
}

// Test suite
describe('BaseProcessDaemon', () => {
  let daemon: TestDaemon;

  beforeEach(() => {
    daemon = new TestDaemon();
  });

  afterEach(async () => {
    try {
      await daemon.stop();
    } catch {
      // Ignore cleanup errors
    }
  });

  describe('Initialization', () => {
    test('should have valid daemon type and capabilities', () => {
      expect(daemon.daemonType).toBe('test-daemon');
      expect(daemon.capabilities).toEqual(['test', 'demo']);
      expect(daemon.processId).toMatch(/^test-daemon-\d+-\w+$/);
    });

    test('should register capabilities correctly', () => {
      const caps = daemon.registerCapabilities();
      expect(caps).toEqual(['test', 'demo']);
    });

    test('should have valid configuration', () => {
      const config = daemon.getConfiguration();
      expect(config.daemonType).toBe('test-daemon');
      expect(config.capabilities).toEqual(['test', 'demo']);
      expect(config.isRunning).toBe(false);
    });
  });

  describe('Lifecycle Management', () => {
    test('should start daemon successfully', async () => {
      await daemon.start();
      
      expect(daemon.wasStartCalled).toBe(true);
      
      const health = daemon.getHealth();
      expect(health.status).toBe('healthy');
      expect(health.processId).toBe(daemon.processId);
    });

    test('should prevent double start', async () => {
      await daemon.start();
      
      await expect(daemon.start()).rejects.toThrow('already running');
    });

    test('should stop daemon successfully', async () => {
      await daemon.start();
      await daemon.stop();
      
      expect(daemon.wasStopCalled).toBe(true);
      
      const health = daemon.getHealth();
      expect(health.status).toBe('unhealthy');
    });

    test('should restart daemon successfully', async () => {
      await daemon.start();
      await daemon.restart();
      
      expect(daemon.wasStopCalled).toBe(true);
      expect(daemon.wasStartCalled).toBe(true);
    });
  });

  describe('Message Handling', () => {
    beforeEach(async () => {
      await daemon.start();
    });

    test('should handle ping message', async () => {
      const message: ProcessMessage = {
        id: 'test-1',
        type: 'ping',
        data: {},
        timestamp: Date.now()
      };

      const result = await daemon.handleMessage(message);
      
      expect(result.success).toBe(true);
      expect(result.data.pong).toBe(true);
      expect(result.processId).toBe(daemon.processId);
    });

    test('should handle health check message', async () => {
      const message: ProcessMessage = {
        id: 'test-2',
        type: 'health',
        data: {},
        timestamp: Date.now()
      };

      const result = await daemon.handleMessage(message);
      
      expect(result.success).toBe(true);
      expect(result.data.status).toBe('healthy');
      expect(result.data.processId).toBe(daemon.processId);
    });

    test('should handle config message', async () => {
      const message: ProcessMessage = {
        id: 'test-3',
        type: 'config',
        data: {},
        timestamp: Date.now()
      };

      const result = await daemon.handleMessage(message);
      
      expect(result.success).toBe(true);
      expect(result.data.daemonType).toBe('test-daemon');
      expect(result.data.capabilities).toEqual(['test', 'demo']);
    });

    test('should handle custom message via onMessage', async () => {
      const message: ProcessMessage = {
        id: 'test-4',
        type: 'custom',
        data: { test: 'value' },
        timestamp: Date.now()
      };

      const result = await daemon.handleMessage(message);
      
      expect(result.success).toBe(true);
      expect(result.data.echo).toEqual({ test: 'value' });
    });

    test('should update heartbeat on message handling', async () => {
      const initialHealth = daemon.getHealth();
      const initialHeartbeat = initialHealth.lastHeartbeat;
      
      // Wait a moment
      await new Promise(resolve => setTimeout(resolve, 10));
      
      const message: ProcessMessage = {
        id: 'test-5',
        type: 'ping',
        data: {},
        timestamp: Date.now()
      };

      await daemon.handleMessage(message);
      
      const newHealth = daemon.getHealth();
      expect(newHealth.lastHeartbeat).toBeGreaterThan(initialHeartbeat);
    });
  });

  describe('Health Monitoring', () => {
    test('should provide health status when stopped', () => {
      const health = daemon.getHealth();
      
      expect(health.processId).toBe(daemon.processId);
      expect(health.status).toBe('unhealthy');
      expect(health.uptime).toBeGreaterThanOrEqual(0);
    });

    test('should provide health status when running', async () => {
      await daemon.start();
      
      const health = daemon.getHealth();
      
      expect(health.status).toBe('healthy');
      expect(health.uptime).toBeGreaterThan(0);
      expect(health.lastHeartbeat).toBeGreaterThan(0);
    });

    test('should update heartbeat manually', () => {
      const initialHealth = daemon.getHealth();
      const initialHeartbeat = initialHealth.lastHeartbeat;
      
      daemon.heartbeat();
      
      const newHealth = daemon.getHealth();
      expect(newHealth.lastHeartbeat).toBeGreaterThan(initialHeartbeat);
    });
  });

  describe('Error Handling', () => {
    test('should handle message processing errors gracefully', async () => {
      // Create a daemon that throws errors
      class ErrorDaemon extends TestDaemon {
        protected async onMessage(): Promise<ProcessResult> {
          throw new Error('Test error');
        }
      }

      const errorDaemon = new ErrorDaemon();
      await errorDaemon.start();

      const message: ProcessMessage = {
        id: 'error-test',
        type: 'custom',
        data: {},
        timestamp: Date.now()
      };

      const result = await errorDaemon.handleMessage(message);
      
      expect(result.success).toBe(false);
      expect(result.error).toBe('Test error');
      expect(result.processId).toBe(errorDaemon.processId);

      await errorDaemon.stop();
    });
  });
});

// Export for use in other test files
export { TestDaemon };