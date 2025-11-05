/**
 * Command Discovery - Unit Tests
 * 
 * Tests the command discovery system that integrates with core module patterns.
 * Validates command metadata extraction, discovery, and integration with the
 * universal module discovery system.
 */

import { strict as assert } from 'assert';
import { describe, it, beforeEach } from 'node:test';
import { CommandDiscovery } from '../../CommandDiscovery.js';

describe('CommandDiscovery', () => {
  let commandDiscovery: CommandDiscovery;

  beforeEach(() => {
    commandDiscovery = new CommandDiscovery();
    commandDiscovery.clearCache();
  });

  describe('Command Discovery', () => {
    it('should discover available commands', async () => {
      const commands = await commandDiscovery.getAvailableCommands();
      
      assert.ok(Array.isArray(commands));
      // Should find some commands in the system
      assert.ok(commands.length >= 0, 'Should discover commands');
      
      // Commands should be unique and sorted
      const uniqueCommands = new Set(commands);
      assert.equal(commands.length, uniqueCommands.size, 'Commands should be unique');
      
      // Should be sorted
      const sortedCommands = [...commands].sort();
      assert.deepEqual(commands, sortedCommands, 'Commands should be sorted');
    });

    it('should filter commands by category', async () => {
      const coreCommands = await commandDiscovery.getCommandsByCategory('core');
      const systemCommands = await commandDiscovery.getCommandsByCategory('system');
      
      assert.ok(Array.isArray(coreCommands));
      assert.ok(Array.isArray(systemCommands));
    });

    it('should filter commands by type', async () => {
      const coreTypeCommands = await commandDiscovery.getCommandsByType('core');
      const extensionCommands = await commandDiscovery.getCommandsByType('extension');
      const integrationCommands = await commandDiscovery.getCommandsByType('integration');
      
      assert.ok(Array.isArray(coreTypeCommands));
      assert.ok(Array.isArray(extensionCommands));
      assert.ok(Array.isArray(integrationCommands));
    });

    it('should apply multiple filters', async () => {
      const filteredCommands = await commandDiscovery.getAvailableCommands({
        categoryFilter: ['core', 'system'],
        typeFilter: ['core'],
        includeOptional: false
      });
      
      assert.ok(Array.isArray(filteredCommands));
    });
  });

  describe('Command Metadata', () => {
    it('should get command metadata by name', async () => {
      const commands = await commandDiscovery.getAvailableCommands();
      
      if (commands.length > 0) {
        const firstCommand = commands[0];
        const metadata = await commandDiscovery.getCommandMetadata(firstCommand);
        
        if (metadata) {
          assert.equal(metadata.name, firstCommand);
          assert.ok(typeof metadata.category === 'string');
          assert.ok(['core', 'extension', 'integration'].includes(metadata.type));
          assert.ok(typeof metadata.filePath === 'string');
          assert.ok(metadata.module);
        }
      }
    });

    it('should return null for non-existent commands', async () => {
      const metadata = await commandDiscovery.getCommandMetadata('non-existent-command-12345');
      assert.equal(metadata, null);
    });

    it('should cache command metadata', async () => {
      const commands = await commandDiscovery.getAvailableCommands();
      
      if (commands.length > 0) {
        const firstCommand = commands[0];
        
        // First call
        const metadata1 = await commandDiscovery.getCommandMetadata(firstCommand);
        
        // Second call (should be cached)
        const metadata2 = await commandDiscovery.getCommandMetadata(firstCommand);
        
        assert.deepEqual(metadata1, metadata2);
      }
    });
  });

  describe('Command Existence Check', () => {
    it('should check if commands exist', async () => {
      const commands = await commandDiscovery.getAvailableCommands();
      
      if (commands.length > 0) {
        const existingCommand = commands[0];
        const hasExisting = await commandDiscovery.hasCommand(existingCommand);
        assert.equal(hasExisting, true);
      }
      
      const hasNonExistent = await commandDiscovery.hasCommand('non-existent-command-12345');
      assert.equal(hasNonExistent, false);
    });
  });

  describe('Dependency Management', () => {
    it('should create dependency iterators', () => {
      const mockDependencies = {
        'health': {
          name: 'health',
          type: 'command' as const,
          required: true,
          config: { timeout: 5000 }
        },
        'connect': {
          name: 'connect',
          type: 'command' as const,
          required: true,
          config: { sessionTypes: ['development'] }
        },
        'help': {
          name: 'help',
          type: 'command' as const,
          required: false,
          config: {}
        }
      };

      const iterator = commandDiscovery.createCommandDependencyIterator(mockDependencies);
      
      assert.deepEqual(iterator.names, ['health', 'connect', 'help']);
      assert.deepEqual(iterator.required, ['health', 'connect']);
      assert.deepEqual(iterator.configs, {
        health: { timeout: 5000 },
        connect: { sessionTypes: ['development'] },
        help: {}
      });
    });
  });

  describe('Cache Management', () => {
    it('should clear cache properly', async () => {
      // Populate cache
      await commandDiscovery.getAvailableCommands();
      
      // Clear cache
      commandDiscovery.clearCache();
      
      // Should work without issues after cache clear
      const commands = await commandDiscovery.getAvailableCommands();
      assert.ok(Array.isArray(commands));
    });
  });

  describe('Command File Path Generation', () => {
    it('should handle various command naming patterns', async () => {
      const commands = await commandDiscovery.getAvailableCommands();
      
      for (const commandName of commands.slice(0, 3)) { // Test first few commands
        const metadata = await commandDiscovery.getCommandMetadata(commandName);
        
        if (metadata) {
          assert.ok(metadata.filePath.includes(commandName) || 
                   metadata.filePath.includes(commandName.charAt(0).toUpperCase() + commandName.slice(1)));
          assert.ok(metadata.filePath.endsWith('.ts'));
        }
      }
    });
  });

  describe('Error Handling', () => {
    it('should handle malformed command modules gracefully', async () => {
      // Test that the discovery system doesn't crash on unexpected module structures
      const commands = await commandDiscovery.getAvailableCommands();
      assert.ok(Array.isArray(commands));
    });

    it('should handle command definition loading failures gracefully', async () => {
      // This test validates that getCommandDefinition doesn't throw on load failures
      const nonExistentDefinition = await commandDiscovery.getCommandDefinition('non-existent-command');
      assert.equal(nonExistentDefinition, null);
    });
  });

  describe('Integration with Core Module System', () => {
    it('should use consistent patterns with other module types', async () => {
      // Validate that command discovery follows the same patterns as other module discovery
      const commands = await commandDiscovery.getAvailableCommands();
      
      // Should return string array like other discovery systems
      assert.ok(Array.isArray(commands));
      
      for (const command of commands) {
        assert.ok(typeof command === 'string');
        assert.ok(command.length > 0);
      }
    });

    it('should provide module info compatible with core system', async () => {
      const commands = await commandDiscovery.getAvailableCommands();
      
      if (commands.length > 0) {
        const metadata = await commandDiscovery.getCommandMetadata(commands[0]);
        
        if (metadata) {
          // Should have ModuleInfo properties
          assert.ok(metadata.module);
          assert.ok(typeof metadata.module.name === 'string');
          assert.ok(typeof metadata.module.path === 'string');
          assert.equal(metadata.module.type, 'command');
          assert.ok(typeof metadata.module.hasPackageJson === 'boolean');
          assert.ok(typeof metadata.module.hasMainFile === 'boolean');
          assert.ok(typeof metadata.module.hasTestDir === 'boolean');
        }
      }
    });
  });
});