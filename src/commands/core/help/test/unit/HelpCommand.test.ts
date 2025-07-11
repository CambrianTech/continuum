/**
 * Help Command - Unit Tests
 * Tests the dynamic help system without hardcoded command lists
 */

import { HelpCommand } from '../../HelpCommand';
import { CommandContext, CommandResult } from '../../../base-command/BaseCommand';

// Mock the UniversalCommandRegistry
jest.mock('../../../../services/UniversalCommandRegistry', () => ({
  getGlobalCommandRegistry: jest.fn(() => ({
    getAvailableCommands: jest.fn().mockResolvedValue(['help', 'health', 'screenshot', 'js-execute']),
    getCommandDefinition: jest.fn().mockImplementation((command: string) => {
      if (command === 'help') {
        return Promise.resolve({
          name: 'help',
          category: 'core',
          description: 'Show help for commands',
          parameters: { command: { type: 'string', description: 'Command to get help for', required: false } },
          examples: [{ description: 'Show all commands', command: 'help' }],
          usage: 'Get help for available commands'
        });
      }
      if (command === 'health') {
        return Promise.resolve({
          name: 'health',
          category: 'system',
          description: 'Check system health',
          parameters: {},
          examples: [{ description: 'Check health', command: 'health' }],
          usage: 'Check system health status'
        });
      }
      return Promise.resolve(null);
    })
  }))
}));

describe('HelpCommand', () => {
  describe('getDefinition', () => {
    test('should return correct command definition', () => {
      const definition = HelpCommand.getDefinition();
      
      expect(definition.name).toBe('help');
      expect(definition.category).toBe('core');
      expect(definition.description).toBeTruthy();
      expect(definition.parameters.command).toBeDefined();
      expect(definition.parameters.command.type).toBe('string');
      expect(definition.parameters.command.required).toBe(false);
      expect(Array.isArray(definition.examples)).toBe(true);
      expect(definition.examples.length).toBeGreaterThan(0);
    });
  });

  describe('execute - general help', () => {
    test('should return list of all available commands', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      expect(result.message).toContain('Available commands');
      expect(result.data).toBeDefined();
      expect(result.data.commands).toBeDefined();
      expect(Array.isArray(result.data.commands)).toBe(true);
      expect(result.data.commands).toContain('help');
      expect(result.data.commands).toContain('health');
      expect(result.data.commandsByCategory).toBeDefined();
      expect(result.data.usage).toBeTruthy();
    });

    test('should categorize commands correctly', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      expect(result.data.commandsByCategory).toBeDefined();
      expect(result.data.commandsByCategory.core).toContain('help');
      expect(result.data.commandsByCategory.system).toContain('health');
    });

    test('should include total command count', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      expect(result.message).toContain('(4 total)'); // Based on mocked commands
    });
  });

  describe('execute - specific command help', () => {
    test('should return help for specific existing command', async () => {
      const result = await HelpCommand.execute({ command: 'help' }, {});
      
      expect(result.success).toBe(true);
      expect(result.message).toContain('Help for command: help');
      expect(result.data).toBeDefined();
      expect(result.data.command).toBe('help');
      expect(result.data.category).toBe('core');
      expect(result.data.description).toBeTruthy();
      expect(result.data.parameters).toBeDefined();
      expect(result.data.examples).toBeDefined();
      expect(result.data.usage).toBeDefined();
    });

    test('should return help for another existing command', async () => {
      const result = await HelpCommand.execute({ command: 'health' }, {});
      
      expect(result.success).toBe(true);
      expect(result.message).toContain('Help for command: health');
      expect(result.data.command).toBe('health');
      expect(result.data.category).toBe('system');
    });

    test('should return error for non-existent command', async () => {
      const result = await HelpCommand.execute({ command: 'nonexistent' }, {});
      
      expect(result.success).toBe(false);
      expect(result.message).toContain("Command 'nonexistent' not found");
      expect(result.error).toContain('Available commands:');
      expect(result.error).toContain('help');
      expect(result.error).toContain('health');
    });
  });

  describe('parameter parsing', () => {
    test('should parse string parameters', async () => {
      const result = await HelpCommand.execute('{"command": "help"}', {});
      
      expect(result.success).toBe(true);
      expect(result.data.command).toBe('help');
    });

    test('should handle object parameters', async () => {
      const result = await HelpCommand.execute({ command: 'help' }, {});
      
      expect(result.success).toBe(true);
      expect(result.data.command).toBe('help');
    });

    test('should handle no parameters', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toBeDefined();
    });

    test('should handle null parameters', async () => {
      const result = await HelpCommand.execute(null, {});
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toBeDefined();
    });
  });

  describe('context handling', () => {
    test('should work without context', async () => {
      const result = await HelpCommand.execute({});
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toBeDefined();
    });

    test('should work with context', async () => {
      const context: CommandContext = {
        sessionId: 'test-session',
        userId: 'test-user'
      };
      
      const result = await HelpCommand.execute({}, context);
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toBeDefined();
    });
  });

  describe('error handling', () => {
    test('should handle command discovery errors gracefully', async () => {
      // Mock the registry to throw an error
      const { getGlobalCommandRegistry } = require('../../../../services/UniversalCommandRegistry');
      getGlobalCommandRegistry.mockReturnValueOnce({
        getAvailableCommands: jest.fn().mockRejectedValue(new Error('Discovery failed')),
        getCommandDefinition: jest.fn().mockRejectedValue(new Error('Definition failed'))
      });

      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(false);
      expect(result.message).toBe('Failed to get help information');
      expect(result.error).toContain('Discovery failed');
    });

    test('should handle definition lookup errors gracefully', async () => {
      // Mock the registry to throw an error for definition lookup
      const { getGlobalCommandRegistry } = require('../../../../services/UniversalCommandRegistry');
      getGlobalCommandRegistry.mockReturnValueOnce({
        getAvailableCommands: jest.fn().mockResolvedValue(['help']),
        getCommandDefinition: jest.fn().mockRejectedValue(new Error('Definition lookup failed'))
      });

      const result = await HelpCommand.execute({ command: 'help' }, {});
      
      expect(result.success).toBe(false);
      expect(result.message).toBe('Failed to get help information');
      expect(result.error).toContain('Definition lookup failed');
    });
  });

  describe('dynamic command discovery', () => {
    test('should not contain hardcoded command lists', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      
      // Commands should come from the mocked registry, not hardcoded lists
      const commands = result.data.commands;
      expect(commands).toEqual(['help', 'health', 'screenshot', 'js-execute']);
    });

    test('should adapt to different available commands', async () => {
      // Mock different available commands
      const { getGlobalCommandRegistry } = require('../../../../services/UniversalCommandRegistry');
      getGlobalCommandRegistry.mockReturnValueOnce({
        getAvailableCommands: jest.fn().mockResolvedValue(['help', 'different-command']),
        getCommandDefinition: jest.fn().mockImplementation((command: string) => {
          if (command === 'different-command') {
            return Promise.resolve({
              name: 'different-command',
              category: 'test',
              description: 'A different command',
              parameters: {},
              examples: [],
              usage: 'Test command'
            });
          }
          return Promise.resolve(null);
        })
      });

      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      expect(result.data.commands).toEqual(['help', 'different-command']);
      expect(result.data.commandsByCategory.test).toContain('different-command');
    });
  });

  describe('base command functionality', () => {
    test('should include timestamp in results', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.timestamp).toBeDefined();
      expect(new Date(result.timestamp!).getTime()).toBeCloseTo(Date.now(), -3); // Within 1 second
    });

    test('should use createSuccessResult format', async () => {
      const result = await HelpCommand.execute({}, {});
      
      expect(result.success).toBe(true);
      expect(result.message).toBeDefined();
      expect(result.data).toBeDefined();
      expect(result.timestamp).toBeDefined();
      expect(result.error).toBeUndefined();
    });

    test('should use createErrorResult format for errors', async () => {
      const result = await HelpCommand.execute({ command: 'nonexistent' }, {});
      
      expect(result.success).toBe(false);
      expect(result.message).toBeDefined();
      expect(result.error).toBeDefined();
      expect(result.timestamp).toBeDefined();
    });
  });
});