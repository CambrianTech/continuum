/**
 * Integration test for daemon system
 * Tests that all daemons start, register, and communicate properly
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { ContinuumSystem } from '../../system/startup/ContinuumSystemStartup';
import { DAEMON_EVENT_BUS } from '../../daemons/base/DaemonEventBus';

describe('Daemon System Integration Tests', () => {
  let system: ContinuumSystem;
  
  before(async () => {
    system = new ContinuumSystem();
  });
  
  after(async () => {
    if (system) {
      await system.stop();
    }
  });
  
  describe('System Startup', () => {
    it('should start all required daemons', async () => {
      await system.start();
      
      // Check health endpoint
      const health = await fetch('http://localhost:9000/api/health');
      assert.strictEqual(health.status, 200, 'Health endpoint should return 200');
      
      const healthData = await health.json();
      assert.strictEqual(healthData.status, 'healthy');
      
      // Verify all critical daemons are registered
      const requiredDaemons = [
        'renderer',
        'command-processor', 
        'static-file',
        'session-manager',
        'browser-manager'
      ];
      
      for (const daemon of requiredDaemons) {
        assert(
          healthData.daemons.includes(daemon),
          `${daemon} should be in registered daemons list`
        );
      }
    });
  });
  
  describe('Inter-Daemon Communication', () => {
    it('should emit and receive events through global event bus', async () => {
      let eventReceived = false;
      
      // Listen for test event
      DAEMON_EVENT_BUS.onEvent('session_created', (event) => {
        eventReceived = true;
        assert.strictEqual(event.sessionType, 'test');
      });
      
      // Emit test event
      DAEMON_EVENT_BUS.emitEvent('session_created', {
        sessionId: 'test-123',
        sessionType: 'test',
        owner: 'test-user'
      });
      
      // Give time for event to propagate
      await new Promise(resolve => setTimeout(resolve, 100));
      
      assert(eventReceived, 'Event should be received through global bus');
    });
    
    it('should route messages between daemons', async () => {
      // Test command execution through the system
      const response = await fetch('http://localhost:9000/api/commands/health', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ args: [] })
      });
      
      assert.strictEqual(response.status, 200);
      const data = await response.json();
      assert(data.status || data.healthy, 'Health command should return status');
    });
  });
  
  describe('Session Management', () => {
    it('should create and join sessions properly', async () => {
      // Test connect command
      const response = await fetch('http://localhost:9000/api/commands/connect', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ 
          args: [],
          sessionType: 'test',
          owner: 'integration-test'
        })
      });
      
      assert.strictEqual(response.status, 200);
      const data = await response.json();
      
      // Validate response structure
      assert(data.sessionId, 'Should return sessionId');
      assert(data.version, 'Should return version');
      assert(['created_new', 'joined_existing'].includes(data.action), 'Should return valid action');
      assert(data.logs?.browser, 'Should return browser log path');
      assert(data.logs?.server, 'Should return server log path');
    });
  });
  
  describe('Daemon Discovery', () => {
    it('should discover all daemons with proper package.json', async () => {
      const { DaemonDiscovery } = await import('../../system/daemon-discovery/DaemonDiscovery');
      const discovery = new DaemonDiscovery();
      
      const daemons = await discovery.discoverDaemons();
      
      // Should discover daemons
      assert(daemons.length > 0, 'Should discover daemons');
      
      // Each daemon should have required metadata
      for (const daemon of daemons) {
        assert.strictEqual(
          daemon.packageJson.continuum?.type,
          'daemon',
          `${daemon.name} should have continuum.type = "daemon"`
        );
      }
    });
  });
});