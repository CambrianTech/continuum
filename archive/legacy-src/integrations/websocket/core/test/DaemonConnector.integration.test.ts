/**
 * INTEGRATION TEST: DaemonConnector → Command Processor Communication
 * 
 * 🎯 CAPTURES CRITICAL BUG: DaemonConnector hardcoded to only handle 'selftest' 
 * instead of establishing real IPC with Command Processor daemon.
 * 
 * 🧅 MIDDLE-OUT LAYER 2: Daemon communication integration
 * 
 * Tests the daemon-to-daemon bridge that enables:
 * - WebSocket clients → WebSocket Daemon → Command Processor Daemon
 * - Real command discovery from running daemons
 * - Multi-command execution beyond hardcoded 'selftest'
 */

import { DaemonConnector } from '../DaemonConnector';

describe('DaemonConnector Integration - Real Daemon Communication', () => {
  let connector: DaemonConnector;

  beforeEach(() => {
    connector = new DaemonConnector({ autoConnect: false });
  });

  afterEach(async () => {
    await connector.disconnect();
  });

  describe('🔌 Command Processor Connection', () => {
    
    it('should connect to actual Command Processor daemon via IPC', async () => {
      // 🎯 BUG CAPTURE: This will fail because DaemonConnector doesn't use real IPC
      
      const connected = await connector.connect();
      expect(connected).toBe(true);
      
      const connection = (connector as any).connection;
      expect(connection.connected).toBe(true);
      
      // Should connect to external daemon process, not create local object
      expect(connection.commandProcessor).toBeDefined();
      expect(connection.commandProcessor.initialized).toBe(true);
    });

    it('should discover multiple commands from Command Processor daemon', async () => {
      // 🎯 BUG CAPTURE: Will only return ['selftest'] due to hardcoded switch
      
      await connector.connect();
      const processor = (connector as any).connection.commandProcessor;
      
      const commands = processor.getCommands();
      expect(Array.isArray(commands)).toBe(true);
      
      // CRITICAL: Should have all commands from Command Processor, not just 'selftest'
      expect(commands).toContain('health');
      expect(commands).toContain('browser');
      expect(commands).toContain('screenshot');
      expect(commands).toContain('preferences');
      expect(commands.length).toBeGreaterThan(5);
    });

  });

  describe('⚡ Command Execution', () => {
    
    beforeEach(async () => {
      await connector.connect();
    });

    it('should execute health command through Command Processor daemon', async () => {
      // 🎯 BUG CAPTURE: Will fail because 'health' not in hardcoded switch
      
      const processor = (connector as any).connection.commandProcessor;
      const result = await processor.executeCommand('health', {}, {});
      
      expect(result.success).toBe(true);
      expect(result.processor).toBe('typescript-command-system');
      expect(result.error).toBeUndefined();
      expect(result.data).toBeDefined();
    });

    it('should execute browser command with parameters', async () => {
      // 🎯 BUG CAPTURE: Will fail because 'browser' not in hardcoded switch
      
      const processor = (connector as any).connection.commandProcessor;
      const result = await processor.executeCommand('browser', { devtools: true }, {});
      
      expect(result.success).toBe(true);
      expect(result.error).toBeUndefined();
    });

    it('should handle unknown commands gracefully', async () => {
      const processor = (connector as any).connection.commandProcessor;
      const result = await processor.executeCommand('nonexistent-command', {}, {});
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('not found');
      expect(result.processor).toBe('typescript-command-system');
    });

  });

  describe('📋 Command Discovery', () => {
    
    beforeEach(async () => {
      await connector.connect();
    });

    it('should get command definitions from Command Processor', async () => {
      const processor = (connector as any).connection.commandProcessor;
      
      // Should work for discovered commands
      const healthDef = processor.getDefinition('health');
      expect(healthDef).toBeDefined();
      expect(healthDef.name).toBe('health');
      
      // 🎯 BUG CAPTURE: Will fail for non-selftest commands
      const browserDef = processor.getDefinition('browser');
      expect(browserDef).toBeDefined();
      expect(browserDef.name).toBe('browser');
      expect(browserDef.parameters).toBeDefined();
    });

    it('should return null for undefined commands', async () => {
      const processor = (connector as any).connection.commandProcessor;
      const nonexistent = processor.getDefinition('nonexistent-command');
      expect(nonexistent).toBeNull();
    });

  });

  describe('🚨 Bug Reproduction', () => {
    
    it('reproduces the exact daemon connector bug from trace', async () => {
      // This reproduces the exact issue found in daemon bridge trace
      
      await connector.connect();
      const processor = (connector as any).connection.commandProcessor;
      
      // The bug: Only 'selftest' works, everything else fails
      const selftestResult = await processor.executeCommand('selftest', {}, {});
      expect(selftestResult.success).toBe(true); // This works
      
      // 🎯 BUG: These should work but will fail due to hardcoded switch
      const healthResult = await processor.executeCommand('health', {}, {});
      expect(healthResult.success).toBe(true); // This SHOULD work but fails
      
      const browserResult = await processor.executeCommand('browser', {}, {});
      expect(browserResult.success).toBe(true); // This SHOULD work but fails
      
      // The root cause: hardcoded command list instead of daemon discovery
      const commands = processor.getCommands();
      expect(commands.length).toBeGreaterThan(1); // Should have many, not just ['selftest']
    });

  });

});

/**
 * 📊 EXPECTED RESULTS:
 * 
 * 🔴 BEFORE FIX: Tests will fail proving the bug
 * - Only 'selftest' command available
 * - health/browser commands return "not found" errors
 * - commands array contains only ['selftest']
 * 
 * 🟢 AFTER FIX: Tests will pass proving real daemon connection
 * - Multiple commands discovered from Command Processor
 * - health/browser commands execute successfully  
 * - Real IPC established between daemons
 * 
 * 🧅 MIDDLE-OUT VALIDATION:
 * This integration test validates Layer 2 (Daemon) communication
 * before moving to Layer 3 (Command) testing.
 */