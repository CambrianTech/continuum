/**
 * DaemonManager Comprehensive Tests
 * Tests daemon management, self-healing, and event handling
 */

import { DaemonManager } from './DaemonManager';
import { EventEmitter } from 'events';

// Mock a simple daemon process for testing
class MockDaemonProcess extends EventEmitter {
  public stdout = new EventEmitter();
  public stderr = new EventEmitter();
  public pid = Math.floor(Math.random() * 10000);
  public killed = false;

  kill(signal?: string): void {
    this.killed = true;
    // Simulate process exit
    setTimeout(() => {
      this.emit('exit', signal === 'SIGKILL' ? 1 : 0);
    }, 10);
  }

  // Simulate daemon output
  simulateOutput(message: string, isError = false): void {
    const stream = isError ? this.stderr : this.stdout;
    stream.emit('data', Buffer.from(message));
  }

  // Simulate daemon crash
  simulateCrash(error?: Error): void {
    if (error) {
      this.emit('error', error);
    } else {
      this.emit('exit', 1); // Non-zero exit code
    }
  }
}

// Override spawn to return our mock
jest.mock('child_process', () => ({
  spawn: jest.fn(() => new MockDaemonProcess())
}));

describe('DaemonManager Comprehensive Tests', () => {
  let daemonManager: DaemonManager;
  let mockSpawn: jest.MockedFunction<any>;

  beforeEach(() => {
    const { spawn } = require('child_process');
    mockSpawn = spawn as jest.MockedFunction<any>;
    daemonManager = new DaemonManager();
  });

  afterEach(async () => {
    await daemonManager.stopAll();
    jest.clearAllMocks();
  });

  test('should start all configured daemons', async () => {
    await daemonManager.startAll();

    // Verify both daemons were started
    expect(mockSpawn).toHaveBeenCalledTimes(2);
    expect(mockSpawn).toHaveBeenCalledWith('npx', ['tsx', 'src/daemons/command-processor/CommandProcessorDaemon.ts'], expect.any(Object));
    expect(mockSpawn).toHaveBeenCalledWith('npx', ['tsx', 'src/integrations/websocket/WebSocketDaemon.ts'], expect.any(Object));

    // Check daemon status
    expect(daemonManager.getDaemonStatus('command-processor')).toBe('running');
    expect(daemonManager.getDaemonStatus('websocket-server')).toBe('running');
  });

  test('should handle daemon startup order with dependencies', async () => {
    await daemonManager.startAll();

    // command-processor should start first (no dependencies)
    // websocket-server should start second (depends on command-processor)
    const calls = mockSpawn.mock.calls;
    expect(calls[0][1][1]).toContain('CommandProcessorDaemon');
    expect(calls[1][1][1]).toContain('WebSocketDaemon');
  });

  test('should emit daemon events when daemons output messages', async () => {
    const healthyEventSpy = jest.fn();
    const errorEventSpy = jest.fn();
    
    daemonManager.on('daemon:healthy', healthyEventSpy);
    daemonManager.on('daemon:error', errorEventSpy);

    await daemonManager.startAll();

    // Get the mock processes
    const processes = mockSpawn.mock.results.map(result => result.value);

    // Simulate daemon reporting healthy status
    processes[0].simulateOutput('✅ Daemon started successfully');
    
    // Simulate daemon reporting error
    processes[1].simulateOutput('❌ Connection failed');

    // Wait for events to propagate
    await new Promise(resolve => setTimeout(resolve, 50));

    expect(healthyEventSpy).toHaveBeenCalledWith({ name: 'command-processor' });
    expect(errorEventSpy).toHaveBeenCalledWith({ 
      name: 'websocket-server', 
      error: '❌ Connection failed' 
    });
  });

  test('should self-heal by restarting critical daemons that crash', async () => {
    const restartSpy = jest.spyOn(daemonManager, 'startDaemon');
    
    await daemonManager.startAll();
    
    // Clear initial startup calls
    restartSpy.mockClear();

    // Get the command processor mock (critical daemon)
    const commandProcessor = mockSpawn.mock.results[0].value;

    // Simulate daemon crash
    commandProcessor.simulateCrash();

    // Wait for restart logic
    await new Promise(resolve => setTimeout(resolve, 100));

    // Should attempt to restart the critical daemon
    expect(restartSpy).toHaveBeenCalled();
  });

  test('should provide accurate daemon status information', () => {
    const status = daemonManager.getAllStatus();
    
    expect(status).toBeDefined();
    expect(typeof status).toBe('object');
    
    // Should be empty initially
    expect(Object.keys(status)).toHaveLength(0);
  });

  test('should stop daemons gracefully', async () => {
    await daemonManager.startAll();
    
    const processes = mockSpawn.mock.results.map(result => result.value);
    
    await daemonManager.stopAll();

    // Verify all processes were killed
    processes.forEach(process => {
      expect(process.killed).toBe(true);
    });

    // Status should show no running daemons
    expect(daemonManager.getDaemonStatus('command-processor')).toBeNull();
    expect(daemonManager.getDaemonStatus('websocket-server')).toBeNull();
  });

  test('should handle daemon startup failures gracefully', async () => {
    // Mock spawn to throw an error
    mockSpawn.mockImplementationOnce(() => {
      throw new Error('Failed to spawn process');
    });

    await expect(daemonManager.startDaemon({
      name: 'test-daemon',
      path: 'test/path',
      critical: true,
      autoRestart: true,
      dependencies: []
    })).rejects.toThrow('Failed to spawn process');
  });

  test('should respect daemon restart limits', async () => {
    await daemonManager.startAll();
    
    const commandProcessor = mockSpawn.mock.results[0].value;
    const restartSpy = jest.spyOn(daemonManager, 'startDaemon');

    // Simulate multiple crashes
    for (let i = 0; i < 5; i++) {
      commandProcessor.simulateCrash();
      await new Promise(resolve => setTimeout(resolve, 50));
    }

    // Should stop trying to restart after too many failures
    expect(restartSpy).toHaveBeenCalledTimes(3); // Max restart count
  });

  test('should handle stderr warnings appropriately', async () => {
    const warningSpy = jest.fn();
    daemonManager.on('daemon:warning', warningSpy);

    await daemonManager.startAll();

    const process = mockSpawn.mock.results[0].value;
    process.simulateOutput('Warning: Deprecated API used', true);

    await new Promise(resolve => setTimeout(resolve, 50));

    expect(warningSpy).toHaveBeenCalledWith({
      name: 'command-processor',
      warning: 'Warning: Deprecated API used'
    });
  });
});

describe('DaemonManager Integration with Real Components', () => {
  test('should work with actual WebSocket daemon configuration', () => {
    const manager = new DaemonManager();
    const configs = (manager as any).configs;

    // Verify configuration includes all daemons including RendererDaemon
    expect(configs.length).toBeGreaterThanOrEqual(4);
    
    const commandProcessor = configs.find((c: any) => c.name === 'command-processor');
    const websocketServer = configs.find((c: any) => c.name === 'websocket-server');
    const renderer = configs.find((c: any) => c.name === 'renderer');
    const browserManager = configs.find((c: any) => c.name === 'browser-manager');

    expect(commandProcessor).toBeDefined();
    expect(commandProcessor.critical).toBe(true);
    expect(commandProcessor.autoRestart).toBe(true);

    expect(websocketServer).toBeDefined();
    expect(websocketServer.critical).toBe(true);
    expect(websocketServer.dependencies).toContain('command-processor');

    expect(renderer).toBeDefined();
    expect(renderer.critical).toBe(false); // Not critical for basic operation
    expect(renderer.dependencies).toContain('websocket-server');

    expect(browserManager).toBeDefined();
    expect(browserManager.critical).toBe(false);
    expect(browserManager.dependencies).toContain('websocket-server');
  });
});

describe('Modular Architecture Integration Tests', () => {
  test('should support daemon route registration pattern', () => {
    // Test that our daemons follow the modular registration pattern
    const mockWebSocketDaemon = {
      registerRouteHandler: jest.fn(),
      registerApiHandler: jest.fn(),
      name: 'websocket-server'
    };

    // Mock RendererDaemon that implements the registration pattern
    const mockRendererDaemon = {
      name: 'renderer',
      registerWithWebSocketDaemon: function(wsD: any) {
        wsD.registerRouteHandler('/src/*', this, jest.fn());
        wsD.registerRouteHandler('/dist/*', this, jest.fn());
        wsD.registerRouteHandler('/', this, jest.fn());
        wsD.registerApiHandler('/api/agents', this, jest.fn());
        wsD.registerApiHandler('/api/personas', this, jest.fn());
      }
    };

    // Simulate the registration process
    mockRendererDaemon.registerWithWebSocketDaemon(mockWebSocketDaemon);

    // Verify routes were registered
    expect(mockWebSocketDaemon.registerRouteHandler).toHaveBeenCalledWith('/src/*', mockRendererDaemon, expect.any(Function));
    expect(mockWebSocketDaemon.registerRouteHandler).toHaveBeenCalledWith('/dist/*', mockRendererDaemon, expect.any(Function));
    expect(mockWebSocketDaemon.registerRouteHandler).toHaveBeenCalledWith('/', mockRendererDaemon, expect.any(Function));

    // Verify APIs were registered
    expect(mockWebSocketDaemon.registerApiHandler).toHaveBeenCalledWith('/api/agents', mockRendererDaemon, expect.any(Function));
    expect(mockWebSocketDaemon.registerApiHandler).toHaveBeenCalledWith('/api/personas', mockRendererDaemon, expect.any(Function));
  });

  test('should support any daemon registering routes and APIs', () => {
    const mockWebSocketDaemon = {
      routeHandlers: new Map(),
      apiHandlers: new Map(),
      registerRouteHandler: function(pattern: string, daemon: any, handler: Function) {
        this.routeHandlers.set(pattern, { daemon, handler });
      },
      registerApiHandler: function(endpoint: string, daemon: any, handler: Function) {
        this.apiHandlers.set(endpoint, { daemon, handler });
      }
    };

    // Test multiple daemons registering different routes
    const mockDaemon1 = { name: 'renderer' };
    const mockDaemon2 = { name: 'command-processor' };

    mockWebSocketDaemon.registerRouteHandler('/ui/*', mockDaemon1, jest.fn());
    mockWebSocketDaemon.registerApiHandler('/api/render', mockDaemon1, jest.fn());
    
    mockWebSocketDaemon.registerRouteHandler('/cmd/*', mockDaemon2, jest.fn());
    mockWebSocketDaemon.registerApiHandler('/api/commands', mockDaemon2, jest.fn());

    // Verify each daemon owns its routes
    expect(mockWebSocketDaemon.routeHandlers.get('/ui/*').daemon.name).toBe('renderer');
    expect(mockWebSocketDaemon.routeHandlers.get('/cmd/*').daemon.name).toBe('command-processor');
    expect(mockWebSocketDaemon.apiHandlers.get('/api/render').daemon.name).toBe('renderer');
    expect(mockWebSocketDaemon.apiHandlers.get('/api/commands').daemon.name).toBe('command-processor');
  });

  test('should maintain daemon independence and modularity', () => {
    // Test that daemons don't need to know about each other's internals
    const mockWebSocketDaemon = {
      registeredDaemons: new Map(),
      registerExternalDaemon: async function(name: string, daemon: any) {
        this.registeredDaemons.set(name, daemon);
        
        // Call registration method if it exists (modular pattern)
        if (daemon.registerWithWebSocketDaemon && typeof daemon.registerWithWebSocketDaemon === 'function') {
          daemon.registerWithWebSocketDaemon(this);
        }
      }
    };

    const mockRendererDaemon = {
      name: 'renderer',
      registerWithWebSocketDaemon: jest.fn()
    };

    const mockCommandDaemon = {
      name: 'command-processor'
      // No registerWithWebSocketDaemon method - should still work
    };

    // Both should register successfully
    expect(async () => {
      await mockWebSocketDaemon.registerExternalDaemon('renderer', mockRendererDaemon);
      await mockWebSocketDaemon.registerExternalDaemon('command-processor', mockCommandDaemon);
    }).not.toThrow();

    // Only renderer should have been asked to register routes
    expect(mockRendererDaemon.registerWithWebSocketDaemon).toHaveBeenCalled();
    
    // Both should be in registered daemons
    expect(mockWebSocketDaemon.registeredDaemons.has('renderer')).toBe(true);
    expect(mockWebSocketDaemon.registeredDaemons.has('command-processor')).toBe(true);
  });
});