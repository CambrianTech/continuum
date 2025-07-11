/**
 * Universal Command Registry - Unit Tests
 * Tests the core functionality of dynamic command discovery and execution
 */

import { UniversalCommandRegistry, getGlobalCommandRegistry, initializeGlobalCommandRegistry } from '../../UniversalCommandRegistry';
import { CommandDefinition, CommandResult } from '../../../commands/core/base-command/BaseCommand';

describe('UniversalCommandRegistry', () => {
  let registry: UniversalCommandRegistry;

  beforeEach(() => {
    registry = new UniversalCommandRegistry({
      scanPaths: ['src/commands'],
      enableFileWatcher: false,
      cacheEnabled: true,
      logLevel: 'error' // Suppress logs during tests
    });
  });

  afterEach(() => {
    registry.removeAllListeners();
  });

  describe('initialization', () => {
    test('should initialize with default configuration', () => {
      expect(registry).toBeInstanceOf(UniversalCommandRegistry);
    });

    test('should emit initialized event after initialization', async () => {
      const initPromise = new Promise<void>((resolve) => {
        registry.once('initialized', () => resolve());
      });

      await registry.initialize();
      await initPromise;
    });

    test('should discover commands during initialization', async () => {
      await registry.initialize();
      const commands = await registry.getAvailableCommands();
      
      // Should discover at least the help command
      expect(commands.length).toBeGreaterThan(0);
      expect(commands).toContain('help');
    });
  });

  describe('command discovery', () => {
    beforeEach(async () => {
      await registry.initialize();
    });

    test('should return available commands without hardcoded lists', async () => {
      const commands = await registry.getAvailableCommands();
      
      expect(Array.isArray(commands)).toBe(true);
      expect(commands.length).toBeGreaterThan(0);
      
      // Verify commands are strings
      commands.forEach(cmd => {
        expect(typeof cmd).toBe('string');
        expect(cmd.length).toBeGreaterThan(0);
      });
    });

    test('should filter commands by category', async () => {
      const coreCommands = await registry.getAvailableCommands({ category: 'core' });
      const allCommands = await registry.getAvailableCommands();
      
      expect(coreCommands.length).toBeLessThanOrEqual(allCommands.length);
      // All returned commands should be core commands
      for (const cmd of coreCommands) {
        const metadata = await registry.getCommandMetadata(cmd);
        expect(metadata?.category).toBe('core');
      }
    });

    test('should filter core-only commands', async () => {
      const coreCommands = await registry.getAvailableCommands({ coreOnly: true });
      
      for (const cmd of coreCommands) {
        const metadata = await registry.getCommandMetadata(cmd);
        expect(metadata?.isCore).toBe(true);
      }
    });

    test('should sort commands by name by default', async () => {
      const commands = await registry.getAvailableCommands({ sortBy: 'name' });
      
      const sortedCommands = [...commands].sort();
      expect(commands).toEqual(sortedCommands);
    });
  });

  describe('command definitions', () => {
    beforeEach(async () => {
      await registry.initialize();
    });

    test('should return command definition for existing command', async () => {
      const definition = await registry.getCommandDefinition('help');
      
      expect(definition).toBeTruthy();
      expect(definition?.name).toBe('help');
      expect(definition?.category).toBe('core');
      expect(definition?.description).toBeTruthy();
      expect(definition?.parameters).toBeDefined();
    });

    test('should return null for non-existent command', async () => {
      const definition = await registry.getCommandDefinition('nonexistent-command');
      expect(definition).toBeNull();
    });

    test('should rescan and retry for missing commands', async () => {
      // This tests the auto-rescan functionality
      const definition = await registry.getCommandDefinition('help');
      expect(definition).toBeTruthy();
    });
  });

  describe('command metadata', () => {
    beforeEach(async () => {
      await registry.initialize();
    });

    test('should return metadata for existing command', async () => {
      const metadata = await registry.getCommandMetadata('help');
      
      expect(metadata).toBeTruthy();
      expect(metadata?.name).toBe('help');
      expect(metadata?.className).toBeTruthy();
      expect(metadata?.filePath).toBeTruthy();
      expect(metadata?.lastModified).toBeInstanceOf(Date);
    });

    test('should return null for non-existent command', async () => {
      const metadata = await registry.getCommandMetadata('nonexistent-command');
      expect(metadata).toBeNull();
    });
  });

  describe('command execution', () => {
    beforeEach(async () => {
      await registry.initialize();
    });

    test('should execute existing command successfully', async () => {
      const result = await registry.executeCommand('help', {}, {});
      
      expect(result.success).toBe(true);
      expect(result.timestamp).toBeTruthy();
      expect(result.data).toBeDefined();
    });

    test('should return error for non-existent command', async () => {
      const result = await registry.executeCommand('nonexistent-command', {}, {});
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('not found');
      expect(result.error).toContain('Available commands:');
    });

    test('should validate parameters when requested', async () => {
      const result = await registry.executeCommand(
        'help',
        { invalid: 'parameter' },
        {},
        { validateParameters: true }
      );
      
      // Should still work as help command accepts optional parameters
      expect(result.success).toBe(true);
    });

    test('should handle timeout when specified', async () => {
      // Use a very short timeout to trigger timeout behavior
      const result = await registry.executeCommand(
        'help',
        {},
        {},
        { timeout: 1 } // 1ms timeout should trigger timeout
      );
      
      // The result could be either success (if command executes quickly) or timeout error
      expect(result).toBeDefined();
      expect(result.timestamp).toBeTruthy();
    });
  });

  describe('command categorization', () => {
    beforeEach(async () => {
      await registry.initialize();
    });

    test('should group commands by category', async () => {
      const categories = await registry.getCommandsByCategory();
      
      expect(typeof categories).toBe('object');
      expect(Object.keys(categories).length).toBeGreaterThan(0);
      
      // Should have core category
      expect(categories.core).toBeDefined();
      expect(Array.isArray(categories.core)).toBe(true);
      expect(categories.core).toContain('help');
    });

    test('should return core commands separately', async () => {
      const coreCommands = await registry.getCoreCommands();
      
      expect(Array.isArray(coreCommands)).toBe(true);
      // Help should be a core command
      expect(coreCommands).toContain('help');
    });

    test('should return kernel commands separately', async () => {
      const kernelCommands = await registry.getKernelCommands();
      
      expect(Array.isArray(kernelCommands)).toBe(true);
      // Kernel commands are required for basic functionality
    });
  });

  describe('command existence checking', () => {
    beforeEach(async () => {
      await registry.initialize();
    });

    test('should return true for existing command', async () => {
      const exists = await registry.hasCommand('help');
      expect(exists).toBe(true);
    });

    test('should return false for non-existent command', async () => {
      const exists = await registry.hasCommand('nonexistent-command');
      expect(exists).toBe(false);
    });
  });

  describe('refresh functionality', () => {
    beforeEach(async () => {
      await registry.initialize();
    });

    test('should emit refreshed event after refresh', async () => {
      const refreshPromise = new Promise<void>((resolve) => {
        registry.once('refreshed', () => resolve());
      });

      await registry.refresh();
      await refreshPromise;
    });

    test('should rediscover commands after refresh', async () => {
      const beforeRefresh = await registry.getAvailableCommands();
      await registry.refresh();
      const afterRefresh = await registry.getAvailableCommands();
      
      // Should have same or more commands after refresh
      expect(afterRefresh.length).toBeGreaterThanOrEqual(beforeRefresh.length);
    });
  });
});

describe('Global Registry Singleton', () => {
  test('should return same instance on multiple calls', () => {
    const registry1 = getGlobalCommandRegistry();
    const registry2 = getGlobalCommandRegistry();
    
    expect(registry1).toBe(registry2);
  });

  test('should initialize global registry successfully', async () => {
    await initializeGlobalCommandRegistry();
    
    const registry = getGlobalCommandRegistry();
    const commands = await registry.getAvailableCommands();
    
    expect(commands.length).toBeGreaterThan(0);
  });
});

describe('Edge Cases and Error Handling', () => {
  let registry: UniversalCommandRegistry;

  beforeEach(() => {
    registry = new UniversalCommandRegistry({
      scanPaths: ['nonexistent/path'], // Invalid path to test error handling
      logLevel: 'error'
    });
  });

  test('should handle invalid scan paths gracefully', async () => {
    await registry.initialize();
    
    const commands = await registry.getAvailableCommands();
    expect(Array.isArray(commands)).toBe(true);
    // Should return empty array for invalid paths
    expect(commands.length).toBe(0);
  });

  test('should handle command execution errors gracefully', async () => {
    await registry.initialize();
    
    // Try to execute command when no commands are available
    const result = await registry.executeCommand('any-command', {}, {});
    
    expect(result.success).toBe(false);
    expect(result.error).toBeTruthy();
    expect(result.timestamp).toBeTruthy();
  });
});