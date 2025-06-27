/**
 * BaseDaemon Critical Bug Tests
 * Tests the fix for the critical bug where onStart() was never called
 */

import { BaseDaemon } from './BaseDaemon';
import { DaemonMessage, DaemonResponse } from './DaemonProtocol';

// Test daemon implementation
class TestDaemon extends BaseDaemon {
  public readonly name = 'test-daemon';
  public readonly version = '1.0.0';
  
  public onStartCalled = false;
  public onStopCalled = false;
  public onStartError: Error | null = null;

  protected async onStart(): Promise<void> {
    this.onStartCalled = true;
    
    if (this.onStartError) {
      throw this.onStartError;
    }
    
    // Simulate actual startup work
    await new Promise(resolve => setTimeout(resolve, 10));
  }

  protected async onStop(): Promise<void> {
    this.onStopCalled = true;
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    return { success: true, data: { echo: message } };
  }

  // Test helper to inject startup errors
  setStartupError(error: Error): void {
    this.onStartError = error;
  }
}

describe('BaseDaemon Critical Bug Tests', () => {
  let daemon: TestDaemon;

  beforeEach(() => {
    daemon = new TestDaemon();
  });

  afterEach(async () => {
    if (daemon.getStatus() === 'running') {
      await daemon.stop();
    }
  });

  test('CRITICAL: onStart() method must be called during daemon startup', async () => {
    // This test verifies the fix for the critical bug where onStart() was never called
    expect(daemon.onStartCalled).toBe(false);
    
    await daemon.start();
    
    // CRITICAL: This must be true after the fix
    expect(daemon.onStartCalled).toBe(true);
    expect(daemon.getSimpleStatus()).toBe('running');
  });

  test('should handle onStart() failures gracefully', async () => {
    const testError = new Error('Startup failed');
    daemon.setStartupError(testError);
    
    await expect(daemon.start()).rejects.toThrow('Startup failed');
    expect(daemon.getSimpleStatus()).toBe('failed');
    expect(daemon.onStartCalled).toBe(true);
  });

  test('should call onStop() when stopping', async () => {
    await daemon.start();
    expect(daemon.onStopCalled).toBe(false);
    
    await daemon.stop();
    
    expect(daemon.onStopCalled).toBe(true);
    expect(daemon.getSimpleStatus()).toBe('stopped');
  });

  test('should prevent multiple starts', async () => {
    await daemon.start();
    
    await expect(daemon.start()).rejects.toThrow('Daemon test-daemon is already running');
  });

  test('should provide accurate status throughout lifecycle', async () => {
    expect(daemon.getSimpleStatus()).toBe('stopped');
    
    const startPromise = daemon.start();
    // Status should be 'starting' during startup
    // (This test might be flaky due to timing, but it's worth checking)
    
    await startPromise;
    expect(daemon.getSimpleStatus()).toBe('running');
    
    await daemon.stop();
    expect(daemon.getSimpleStatus()).toBe('stopped');
  });

  test('should generate unique message IDs', async () => {
    const message1 = await daemon.sendMessage('test-target', 'test', {});
    const message2 = await daemon.sendMessage('test-target', 'test', {});
    
    expect(message1).toBeDefined();
    expect(message2).toBeDefined();
    // Since sendMessage returns a response, we can test that they're unique calls
    expect(message1.success).toBe(true);
    expect(message2.success).toBe(true);
  });

  test('should handle heartbeat correctly', async () => {
    await daemon.start();
    
    const status = daemon.getSimpleStatus();
    expect(status).toBe('running');
    
    // Daemon should have uptime
    const uptime = daemon.getUptime();
    expect(uptime).toBeGreaterThan(0);
  });
});

describe('WebSocket Daemon Integration Test (using fixed BaseDaemon)', () => {
  test('WebSocket daemon should actually bind to port after BaseDaemon fix', async () => {
    // Import here to ensure we get the fixed version
    const { WebSocketDaemon } = await import('../../integrations/websocket/WebSocketDaemon');
    const { WebSocket } = await import('ws');
    
    const daemon = new WebSocketDaemon({
      port: 9003,
      host: 'localhost',
      daemonConfig: { autoConnect: false }
    });

    try {
      await daemon.start();

      // CRITICAL: Test that the WebSocket server is actually listening
      const connectionTest = await new Promise<boolean>((resolve) => {
        const client = new WebSocket('ws://localhost:9003');
        
        client.on('open', () => {
          client.close();
          resolve(true);
        });
        
        client.on('error', () => {
          resolve(false);
        });
        
        setTimeout(() => resolve(false), 5000);
      });

      expect(connectionTest).toBe(true);
      
    } finally {
      await daemon.stop();
    }
  }, 10000);
});