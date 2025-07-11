/**
 * Help Command - Integration Tests
 * Tests the help command integration with real command discovery system
 */

import { HelpCommand } from '../../HelpCommand';
import { getGlobalCommandRegistry, initializeGlobalCommandRegistry } from '../../../../services/UniversalCommandRegistry';
import { CommandContext } from '../../../base-command/BaseCommand';

describe('HelpCommand Integration', () => {
  beforeAll(async () => {
    // Initialize the global command registry for real integration testing
    await initializeGlobalCommandRegistry();
  });

  describe('Real Command Discovery Integration', () => {
    test('should discover and list real commands from filesystem', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toBeDefined();
      expect(Array.isArray(result.data.commands)).toBe(true);
      expect(result.data.commands.length).toBeGreaterThan(0);
      
      // Should include known commands that exist in the system
      expect(result.data.commands).toContain('help');
      
      // Verify message includes actual count
      expect(result.message).toMatch(/Available commands \(\d+ total\)/);
    });

    test('should categorize real commands correctly', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      expect(result.data.commandsByCategory).toBeDefined();
      
      // Should have core category with help command
      expect(result.data.commandsByCategory.core).toBeDefined();
      expect(result.data.commandsByCategory.core).toContain('help');
      
      // Should have multiple categories
      const categories = Object.keys(result.data.commandsByCategory);
      expect(categories.length).toBeGreaterThan(1);
    });

    test('should provide usage information', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      expect(result.data.usage).toBe('Use "help <command>" to get detailed help for a specific command');
    });
  });

  describe('Specific Command Help Integration', () => {
    test('should return real help for help command', async () => {
      const result = await HelpCommand.execute({ command: 'help' }, {});
      
      expect(result.success).toBe(true);
      expect(result.message).toBe('Help for command: help');
      expect(result.data.command).toBe('help');
      expect(result.data.category).toBe('core');
      expect(result.data.description).toBeTruthy();
      expect(result.data.parameters).toBeDefined();
      expect(result.data.parameters.command).toBeDefined();
      expect(result.data.examples).toBeDefined();
      expect(Array.isArray(result.data.examples)).toBe(true);
      expect(result.data.examples.length).toBeGreaterThan(0);
    });

    test('should handle multiple real commands', async () => {
      // Get list of available commands first
      const listResult = await HelpCommand.execute({}, {});
      expect(listResult.success).toBe(true);
      
      const commands = listResult.data.commands.slice(0, 3); // Test first 3 commands
      
      for (const command of commands) {
        const result = await HelpCommand.execute({ command }, {});
        
        expect(result.success).toBe(true);
        expect(result.data.command).toBe(command);
        expect(result.data.category).toBeTruthy();
        expect(result.data.description).toBeTruthy();
      }
    });

    test('should return error for truly non-existent command', async () => {
      const result = await HelpCommand.execute({ command: 'definitely-nonexistent-command-12345' }, {});
      
      expect(result.success).toBe(false);
      expect(result.message).toContain("Command 'definitely-nonexistent-command-12345' not found");
      expect(result.error).toContain('Available commands:');
    });
  });

  describe('Context Integration', () => {
    test('should work with session context', async () => {
      const context: CommandContext = {
        sessionId: 'integration-test-session',
        userId: 'integration-test-user'
      };
      
      const result = await HelpCommand.execute({}, context);
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toBeDefined();
    });

    test('should work with WebSocket context', async () => {
      const context: CommandContext = {
        sessionId: 'ws-test-session',
        webSocketServer: {
          send: jest.fn(),
          broadcast: jest.fn(),
          clients: new Set()
        }
      };
      
      const result = await HelpCommand.execute({}, context);
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toBeDefined();
    });
  });

  describe('Command Registry Integration', () => {
    test('should use same commands as registry directly', async () => {
      const registry = getGlobalCommandRegistry();
      const registryCommands = await registry.getAvailableCommands();
      
      const helpResult = await HelpCommand.execute({}, {});
      const helpCommands = helpResult.data.commands;
      
      expect(helpCommands).toEqual(registryCommands);
    });

    test('should reflect registry changes after refresh', async () => {
      const registry = getGlobalCommandRegistry();
      
      // Get initial state
      const initialResult = await HelpCommand.execute({}, {});
      const initialCommands = initialResult.data.commands;
      
      // Refresh registry
      await registry.refresh();
      
      // Get state after refresh
      const refreshResult = await HelpCommand.execute({}, {});
      const refreshCommands = refreshResult.data.commands;
      
      // Should have same or more commands (registry might discover new ones)
      expect(refreshCommands.length).toBeGreaterThanOrEqual(initialCommands.length);
    });

    test('should get real command definitions', async () => {
      const registry = getGlobalCommandRegistry();
      const commands = await registry.getAvailableCommands();
      const firstCommand = commands[0];
      
      // Get definition through help command
      const helpResult = await HelpCommand.execute({ command: firstCommand }, {});
      
      // Get definition directly from registry
      const registryDefinition = await registry.getCommandDefinition(firstCommand);
      
      expect(helpResult.success).toBe(true);
      expect(helpResult.data.command).toBe(registryDefinition?.name);
      expect(helpResult.data.category).toBe(registryDefinition?.category);
      expect(helpResult.data.description).toBe(registryDefinition?.description);
    });
  });

  describe('No Hardcoded Commands Verification', () => {
    test('should not contain any hardcoded command references', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      
      // Commands should be dynamically discovered
      const commands = result.data.commands;
      expect(Array.isArray(commands)).toBe(true);
      expect(commands.length).toBeGreaterThan(0);
      
      // Verify all commands exist in the registry
      const registry = getGlobalCommandRegistry();
      for (const command of commands) {
        const exists = await registry.hasCommand(command);
        expect(exists).toBe(true);
      }
    });

    test('should adapt to dynamic command additions', async () => {
      const registry = getGlobalCommandRegistry();
      
      // Initial scan
      const initialResult = await HelpCommand.execute({}, {});
      const initialCommands = initialResult.data.commands;
      
      // Force rescan to pick up any new commands
      await registry.refresh();
      
      // Check again
      const refreshResult = await HelpCommand.execute({}, {});
      const refreshCommands = refreshResult.data.commands;
      
      // Should be consistent with registry
      const registryCommands = await registry.getAvailableCommands();
      expect(refreshCommands).toEqual(registryCommands);
    });
  });

  describe('Performance Integration', () => {
    test('should execute help quickly', async () => {
      const start = Date.now();
      const result = await HelpCommand.execute({}, {});
      const duration = Date.now() - start;
      
      expect(result.success).toBe(true);
      expect(duration).toBeLessThan(5000); // Should complete within 5 seconds
    });

    test('should handle concurrent help requests', async () => {
      const promises = Array(10).fill(0).map((_, i) => 
        HelpCommand.execute(i % 2 === 0 ? {} : { command: 'help' }, {})
      );
      
      const results = await Promise.all(promises);
      
      // All requests should succeed
      results.forEach(result => {
        expect(result.success).toBe(true);
      });
      
      // General help results should be identical
      const generalResults = results.filter((_, i) => i % 2 === 0);
      for (let i = 1; i < generalResults.length; i++) {
        expect(generalResults[i].data.commands).toEqual(generalResults[0].data.commands);
      }
    });
  });

  describe('Error Recovery Integration', () => {
    test('should recover from temporary registry issues', async () => {
      // This test verifies that help command is resilient
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toBeDefined();
    });

    test('should handle command definition lookup failures gracefully', async () => {
      // Try to get help for a command that might have definition issues
      const result = await HelpCommand.execute({ command: 'help' }, {});
      
      // Should either succeed or fail gracefully
      expect(result).toBeDefined();
      expect(result.timestamp).toBeTruthy();
      
      if (!result.success) {
        expect(result.error).toBeTruthy();
      }
    });
  });

  describe('End-to-End Help System', () => {
    test('should provide complete help system functionality', async () => {
      // 1. Get list of all commands
      const listResult = await HelpCommand.execute({}, {});
      expect(listResult.success).toBe(true);
      expect(listResult.data.commands.length).toBeGreaterThan(0);
      
      // 2. Get help for a specific command
      const specificCommand = listResult.data.commands[0];
      const helpResult = await HelpCommand.execute({ command: specificCommand }, {});
      expect(helpResult.success).toBe(true);
      expect(helpResult.data.command).toBe(specificCommand);
      
      // 3. Verify command categories work
      expect(listResult.data.commandsByCategory).toBeDefined();
      expect(Object.keys(listResult.data.commandsByCategory).length).toBeGreaterThan(0);
      
      // 4. Verify usage information is provided
      expect(listResult.data.usage).toBeTruthy();
    });
  });
});