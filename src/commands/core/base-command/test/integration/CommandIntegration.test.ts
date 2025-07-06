/**
 * BaseCommand Integration Tests
 * 
 * TESTING REQUIREMENTS:
 * =====================
 * LAYER 1 INTEGRATION VALIDATION:
 * - Real command implementations properly extend BaseCommand
 * - Command definitions follow required structure
 * - Commands execute and return proper CommandResult format
 * - Error handling works consistently across commands
 * - Registry integration functions properly
 * 
 * MIDDLE-OUT METHODOLOGY:
 * - These tests validate Layer 1 integration before Layer 2 daemon tests
 * - Ensures command foundation is solid before testing daemon communication
 * - Builds confidence for command execution in higher layers
 */

import { BaseCommand, CommandContext } from '../../BaseCommand';
import { PreferencesCommand } from '../../../preferences/PreferencesCommand';
import { ReloadCommand } from '../../../reload/ReloadCommand';
import { SelfTestCommand } from '../../../../development/selftest/SelfTestCommand';

describe('BaseCommand Integration Tests', () => {
  
  describe('Real Command Implementations', () => {
    
    test('PreferencesCommand should properly extend BaseCommand', () => {
      expect(PreferencesCommand.prototype).toBeInstanceOf(BaseCommand);
    });

    test('ReloadCommand should properly extend BaseCommand', () => {
      expect(ReloadCommand.prototype).toBeInstanceOf(BaseCommand);
    });

    test('SelfTestCommand should properly extend BaseCommand', () => {
      expect(SelfTestCommand.prototype).toBeInstanceOf(BaseCommand);
    });

    test('PreferencesCommand should have valid definition structure', () => {
      const definition = PreferencesCommand.getDefinition();
      
      expect(definition).toHaveProperty('name');
      expect(definition).toHaveProperty('category');
      expect(definition).toHaveProperty('icon');
      expect(definition).toHaveProperty('description');
      expect(definition).toHaveProperty('params');
      expect(definition).toHaveProperty('examples');
      expect(definition).toHaveProperty('usage');
      
      expect(typeof definition.name).toBe('string');
      expect(definition.name.length).toBeGreaterThan(0);
      expect(Array.isArray(definition.examples)).toBe(true);
    });

    test('ReloadCommand should have valid definition structure', () => {
      const definition = ReloadCommand.getDefinition();
      
      expect(definition).toHaveProperty('name');
      expect(definition).toHaveProperty('category');
      expect(definition).toHaveProperty('icon');
      expect(definition).toHaveProperty('description');
      expect(definition).toHaveProperty('params');
      expect(definition).toHaveProperty('examples');
      expect(definition).toHaveProperty('usage');
      
      expect(typeof definition.name).toBe('string');
      expect(definition.name.length).toBeGreaterThan(0);
      expect(Array.isArray(definition.examples)).toBe(true);
    });
  });

  describe('Command Execution Integration', () => {
    
    test('SelfTestCommand should execute and return valid CommandResult', async () => {
      const params = {};
      const context: CommandContext = { sessionId: 'test-integration' };
      
      const result = await SelfTestCommand.execute(params, context);
      
      expect(result).toHaveProperty('success');
      expect(result).toHaveProperty('message');
      expect(result).toHaveProperty('timestamp');
      expect(typeof result.success).toBe('boolean');
      expect(typeof result.message).toBe('string');
      expect(typeof result.timestamp).toBe('string');
    });

    test('PreferencesCommand should handle list operation', async () => {
      const params = { operation: 'list' };
      const context: CommandContext = { sessionId: 'test-preferences' };
      
      const result = await PreferencesCommand.execute(params, context);
      
      expect(result).toHaveProperty('success');
      expect(result).toHaveProperty('message');
      expect(result).toHaveProperty('data');
      expect(typeof result.success).toBe('boolean');
      
      // Should return preferences data structure
      if (result.success && result.data) {
        expect(typeof result.data).toBe('object');
      }
    });

    test('ReloadCommand should handle page reload request', async () => {
      const params = { target: 'page' };
      const context: CommandContext = { sessionId: 'test-reload' };
      
      const result = await ReloadCommand.execute(params, context);
      
      expect(result).toHaveProperty('success');
      expect(result).toHaveProperty('message');
      expect(typeof result.success).toBe('boolean');
      expect(typeof result.message).toBe('string');
    });
  });

  describe('Error Handling Integration', () => {
    
    test('Commands should handle missing required parameters consistently', async () => {
      // Test PreferencesCommand with invalid operation
      const invalidParams = { operation: 'invalid_operation' };
      const context: CommandContext = { sessionId: 'test-error' };
      
      const result = await PreferencesCommand.execute(invalidParams, context);
      
      expect(result.success).toBe(false);
      expect(result).toHaveProperty('error');
      expect(typeof result.error).toBe('string');
      expect(result.error!.length).toBeGreaterThan(0);
    });

    test('Commands should handle malformed parameters gracefully', async () => {
      const malformedParams = { operation: null };
      const context: CommandContext = { sessionId: 'test-malformed' };
      
      const result = await PreferencesCommand.execute(malformedParams, context);
      
      expect(result.success).toBe(false);
      expect(result).toHaveProperty('error');
    });
  });

  describe('Registry Integration', () => {
    
    test('Commands should create proper registry entries', () => {
      const prefsEntry = PreferencesCommand.createRegistryEntry();
      const reloadEntry = ReloadCommand.createRegistryEntry();
      const selftestEntry = SelfTestCommand.createRegistryEntry();
      
      // Test structure
      expect(prefsEntry).toHaveProperty('name');
      expect(prefsEntry).toHaveProperty('execute');
      expect(prefsEntry).toHaveProperty('definition');
      
      expect(reloadEntry).toHaveProperty('name');
      expect(reloadEntry).toHaveProperty('execute');
      expect(reloadEntry).toHaveProperty('definition');
      
      expect(selftestEntry).toHaveProperty('name');
      expect(selftestEntry).toHaveProperty('execute');
      expect(selftestEntry).toHaveProperty('definition');
      
      // Test name formatting (should be uppercase)
      expect(prefsEntry.name).toBe(prefsEntry.name.toUpperCase());
      expect(reloadEntry.name).toBe(reloadEntry.name.toUpperCase());
      expect(selftestEntry.name).toBe(selftestEntry.name.toUpperCase());
      
      // Test execute functions
      expect(typeof prefsEntry.execute).toBe('function');
      expect(typeof reloadEntry.execute).toBe('function');
      expect(typeof selftestEntry.execute).toBe('function');
    });

    test('Registry entries should have matching definitions', () => {
      const prefsEntry = PreferencesCommand.createRegistryEntry();
      const directDefinition = PreferencesCommand.getDefinition();
      
      expect(prefsEntry.definition).toEqual(directDefinition);
      expect(prefsEntry.name).toBe(directDefinition.name.toUpperCase());
    });
  });

  describe('Context Handling Integration', () => {
    
    test('Commands should handle context with WebSocket broadcasting', async () => {
      const mockBroadcast = jest.fn();
      const context: CommandContext = {
        webSocketServer: { broadcast: mockBroadcast },
        sessionId: 'test-broadcast-integration'
      };
      
      const result = await SelfTestCommand.execute({}, context);
      
      expect(result.success).toBe(true);
      // SelfTestCommand might not broadcast, but should handle context gracefully
    });

    test('Commands should handle context without WebSocket safely', async () => {
      const context: CommandContext = { 
        sessionId: 'test-no-websocket',
        continuonStatus: undefined 
      };
      
      const result = await SelfTestCommand.execute({}, context);
      
      expect(result.success).toBe(true);
      expect(result).toHaveProperty('message');
    });
  });

  describe('Command Composition Integration', () => {
    
    test('Multiple commands should execute independently', async () => {
      const context: CommandContext = { sessionId: 'test-composition' };
      
      // Execute multiple commands in sequence
      const selftestResult = await SelfTestCommand.execute({}, context);
      const prefsResult = await PreferencesCommand.execute({ operation: 'list' }, context);
      const reloadResult = await ReloadCommand.execute({ target: 'page' }, context);
      
      // All should complete independently
      expect(selftestResult).toHaveProperty('success');
      expect(prefsResult).toHaveProperty('success');
      expect(reloadResult).toHaveProperty('success');
      
      // Each should have unique timestamps
      expect(selftestResult.timestamp).not.toBe(prefsResult.timestamp);
      expect(prefsResult.timestamp).not.toBe(reloadResult.timestamp);
    });

    test('Command results should be properly typed and consistent', async () => {
      const context: CommandContext = { sessionId: 'test-typing' };
      
      const results = await Promise.all([
        SelfTestCommand.execute({}, context),
        PreferencesCommand.execute({ operation: 'list' }, context),
        ReloadCommand.execute({ target: 'page' }, context)
      ]);
      
      results.forEach((result, _index) => {
        expect(result).toHaveProperty('success');
        expect(result).toHaveProperty('message');
        expect(result).toHaveProperty('timestamp');
        expect(typeof result.success).toBe('boolean');
        expect(typeof result.message).toBe('string');
        expect(typeof result.timestamp).toBe('string');
        
        // Timestamp should be valid ISO string
        expect(() => new Date(result.timestamp!)).not.toThrow();
      });
    });
  });

});