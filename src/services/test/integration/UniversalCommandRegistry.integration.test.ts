/**
 * Universal Command Registry - Integration Tests
 * Tests the integration with the real command system and filesystem
 */

import { UniversalCommandRegistry, getGlobalCommandRegistry } from '../../UniversalCommandRegistry';
import { DaemonConnector } from '../../../integrations/websocket/core/DaemonConnector';

describe('UniversalCommandRegistry Integration', () => {
  let registry: UniversalCommandRegistry;

  beforeAll(async () => {
    registry = getGlobalCommandRegistry();
    await registry.initialize();
  });

  describe('Real Command Discovery', () => {
    test('should discover actual commands from filesystem', async () => {
      const commands = await registry.getAvailableCommands();
      
      expect(commands.length).toBeGreaterThan(0);
      
      // Should discover common commands that we know exist
      const expectedCommands = ['help', 'health', 'screenshot'];
      const foundExpected = expectedCommands.filter(cmd => commands.includes(cmd));
      expect(foundExpected.length).toBeGreaterThan(0);
    });

    test('should discover commands with proper metadata', async () => {
      const commands = await registry.getAvailableCommands();
      
      for (const command of commands.slice(0, 5)) { // Test first 5 commands
        const metadata = await registry.getCommandMetadata(command);
        
        expect(metadata).toBeTruthy();
        expect(metadata?.name).toBe(command);
        expect(metadata?.filePath).toBeTruthy();
        expect(metadata?.className).toBeTruthy();
        expect(metadata?.lastModified).toBeInstanceOf(Date);
      }
    });

    test('should categorize real commands correctly', async () => {
      const categories = await registry.getCommandsByCategory();
      
      expect(Object.keys(categories).length).toBeGreaterThan(0);
      
      // Should have various categories
      const expectedCategories = ['core', 'system', 'browser', 'file'];
      const foundCategories = expectedCategories.filter(cat => categories[cat]);
      expect(foundCategories.length).toBeGreaterThan(0);
    });
  });

  describe('Real Command Execution', () => {
    test('should execute help command and return real data', async () => {
      const result = await registry.executeCommand('help', {}, {});
      
      expect(result.success).toBe(true);
      expect(result.data).toBeDefined();
      expect(result.data.commands).toBeDefined();
      expect(Array.isArray(result.data.commands)).toBe(true);
      expect(result.data.commands.length).toBeGreaterThan(0);
    });

    test('should execute help for specific command', async () => {
      const result = await registry.executeCommand('help', { command: 'help' }, {});
      
      expect(result.success).toBe(true);
      expect(result.data).toBeDefined();
      expect(result.data.command).toBe('help');
      expect(result.data.description).toBeTruthy();
    });

    test('should handle command execution errors gracefully', async () => {
      const result = await registry.executeCommand('help', { command: 'nonexistent' }, {});
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('not found');
    });
  });

  describe('Command Definition Lookup', () => {
    test('should return real command definitions', async () => {
      const definition = await registry.getCommandDefinition('help');
      
      expect(definition).toBeTruthy();
      expect(definition?.name).toBe('help');
      expect(definition?.category).toBe('core');
      expect(definition?.description).toBeTruthy();
      expect(definition?.parameters).toBeDefined();
      expect(definition?.examples).toBeDefined();
      expect(Array.isArray(definition?.examples)).toBe(true);
    });

    test('should load definitions for multiple commands', async () => {
      const commands = await registry.getAvailableCommands();
      const sampleCommands = commands.slice(0, 3); // Test first 3 commands
      
      for (const command of sampleCommands) {
        const definition = await registry.getCommandDefinition(command);
        
        expect(definition).toBeTruthy();
        expect(definition?.name).toBe(command);
        expect(definition?.category).toBeTruthy();
        expect(definition?.description).toBeTruthy();
      }
    });
  });

  describe('Performance and Caching', () => {
    test('should cache command list for quick access', async () => {
      const start1 = Date.now();
      const commands1 = await registry.getAvailableCommands();
      const time1 = Date.now() - start1;
      
      const start2 = Date.now();
      const commands2 = await registry.getAvailableCommands();
      const time2 = Date.now() - start2;
      
      expect(commands1).toEqual(commands2);
      // Second call should be faster (cached)
      expect(time2).toBeLessThanOrEqual(time1);
    });

    test('should handle concurrent command discovery', async () => {
      const promises = Array(5).fill(0).map(() => registry.getAvailableCommands());
      const results = await Promise.all(promises);
      
      // All results should be identical
      for (let i = 1; i < results.length; i++) {
        expect(results[i]).toEqual(results[0]);
      }
    });
  });
});

describe('DaemonConnector Integration', () => {
  let connector: DaemonConnector;

  beforeEach(() => {
    connector = new DaemonConnector();
  });

  afterEach(async () => {
    await connector.disconnect();
  });

  test('should connect to UniversalCommandRegistry successfully', async () => {
    const connected = await connector.connect();
    
    expect(connected).toBe(true);
    expect(connector.isConnected()).toBe(true);
  });

  test('should provide same commands as registry', async () => {
    await connector.connect();
    
    const connectorCommands = connector.getAvailableCommands();
    const registryCommands = await getGlobalCommandRegistry().getAvailableCommands();
    
    expect(connectorCommands.length).toBeGreaterThan(0);
    expect(connectorCommands.length).toBe(registryCommands.length);
    
    // Commands should match
    for (const cmd of connectorCommands) {
      expect(registryCommands).toContain(cmd);
    }
  });

  test('should execute commands through connector', async () => {
    await connector.connect();
    
    const result = await connector.executeCommand('help', {}, {});
    
    expect(result.success).toBe(true);
    expect(result.processor).toBe('universal-command-registry');
  });

  test('should get command definitions through connector', async () => {
    await connector.connect();
    
    const definition = await connector.getCommandDefinition('help');
    
    expect(definition).toBeTruthy();
    expect(definition.name).toBe('help');
  });
});

describe('End-to-End Command Flow', () => {
  test('should discover, define, and execute commands end-to-end', async () => {
    const registry = getGlobalCommandRegistry();
    await registry.initialize();
    
    // 1. Discover commands
    const commands = await registry.getAvailableCommands();
    expect(commands.length).toBeGreaterThan(0);
    
    // 2. Get definition for first command
    const firstCommand = commands[0];
    const definition = await registry.getCommandDefinition(firstCommand);
    expect(definition).toBeTruthy();
    
    // 3. Execute the command
    const result = await registry.executeCommand(firstCommand, {}, {});
    expect(result).toBeDefined();
    expect(result.timestamp).toBeTruthy();
  });

  test('should handle command parameters correctly', async () => {
    const registry = getGlobalCommandRegistry();
    
    // Test help command with specific parameter
    const result = await registry.executeCommand('help', { command: 'help' }, {});
    
    expect(result.success).toBe(true);
    expect(result.data).toBeDefined();
  });

  test('should maintain command state across multiple operations', async () => {
    const registry = getGlobalCommandRegistry();
    
    // Multiple operations should work consistently
    const commands1 = await registry.getAvailableCommands();
    const result = await registry.executeCommand('help', {}, {});
    const commands2 = await registry.getAvailableCommands();
    
    expect(commands1).toEqual(commands2);
    expect(result.success).toBe(true);
  });
});

describe('Error Recovery and Resilience', () => {
  test('should recover from command execution failures', async () => {
    const registry = getGlobalCommandRegistry();
    
    // Execute invalid command
    const badResult = await registry.executeCommand('invalid-command', {}, {});
    expect(badResult.success).toBe(false);
    
    // Should still work for valid commands
    const goodResult = await registry.executeCommand('help', {}, {});
    expect(goodResult.success).toBe(true);
  });

  test('should handle refresh after errors', async () => {
    const registry = getGlobalCommandRegistry();
    
    // Execute invalid command
    await registry.executeCommand('invalid-command', {}, {});
    
    // Refresh should still work
    await registry.refresh();
    
    const commands = await registry.getAvailableCommands();
    expect(commands.length).toBeGreaterThan(0);
  });
});

describe('No Hardcoded Commands Verification', () => {
  test('should not contain any hardcoded command lists', async () => {
    const registry = getGlobalCommandRegistry();
    const commands = await registry.getAvailableCommands();
    
    // Commands should be discovered dynamically from filesystem
    // If this test finds hardcoded commands, it indicates a problem
    expect(commands.length).toBeGreaterThan(0);
    
    // Verify commands come from actual files
    for (const command of commands.slice(0, 3)) {
      const metadata = await registry.getCommandMetadata(command);
      expect(metadata?.filePath).toBeTruthy();
      
      // Verify file exists
      const fs = require('fs');
      expect(fs.existsSync(metadata?.filePath)).toBe(true);
    }
  });

  test('should discover new commands automatically', async () => {
    const registry = getGlobalCommandRegistry();
    
    // Get initial commands
    const initialCommands = await registry.getAvailableCommands();
    
    // Refresh to discover any new commands
    await registry.refresh();
    const refreshedCommands = await registry.getAvailableCommands();
    
    // Should have same or more commands (never hardcoded subset)
    expect(refreshedCommands.length).toBeGreaterThanOrEqual(initialCommands.length);
  });
});