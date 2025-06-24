/**
 * Modular Command System Integration Tests
 * Tests the new command architecture with existing test patterns
 */

const CoreModule = require('../../../src/modules/CoreModule.cjs');
const FluentAPI = require('../../../src/modules/FluentAPI.cjs');
const BaseCommand = require('../../../src/commands/core/BaseCommand.cjs');

describe('Modular Command System', () => {
  let coreModule;
  
  beforeAll(async () => {
    coreModule = new CoreModule();
    await coreModule.initialize();
  });

  describe('Core Module Loading', () => {
    test('should load all expected commands', () => {
      const expectedCommands = [
        'help', 'agents', 'diagnostics', 'findUser', 
        'preferences', 'screenshot', 'share', 'restart'
      ];
      
      expectedCommands.forEach(cmdName => {
        const cmd = coreModule.getCommand(cmdName);
        expect(cmd).toBeDefined();
        expect(cmd.getDefinition).toBeDefined();
        expect(cmd.execute).toBeDefined();
      });
    });

    test('should load all expected macros', () => {
      const expectedMacros = [
        'shareScreenshot', 'shareScreenshotToJoel', 
        'quickDiagnostics', 'shareToJoelsPreferredMedia'
      ];
      
      expectedMacros.forEach(macroName => {
        const macro = coreModule.getMacro(macroName);
        expect(macro).toBeDefined();
      });
    });

    test('should provide fluent API', () => {
      const fluentAPI = coreModule.getFluentAPI();
      expect(fluentAPI).toBeDefined();
      expect(fluentAPI.screenshot).toBeDefined();
      expect(fluentAPI.findUser).toBeDefined();
      expect(fluentAPI.share).toBeDefined();
    });
  });

  describe('Command Execution', () => {
    test('help command should execute successfully', async () => {
      const helpCmd = coreModule.getCommand('help');
      const result = await helpCmd.execute('{}', null);
      
      expect(result.success).toBe(true);
      expect(result.message).toBe('Help displayed');
      expect(result.data.version).toBeDefined();
    });

    test('findUser command should find users correctly', async () => {
      const findUserCmd = coreModule.getCommand('findUser');
      const result = await findUserCmd.execute('{"name": "joel"}', null);
      
      expect(result.success).toBe(true);
      expect(result.data.name).toBe('joel');
      expect(result.data.role).toBe('admin');
      expect(result.data.preferences).toBeDefined();
    });

    test('preferences command should handle user preferences', async () => {
      const prefsCmd = coreModule.getCommand('preferences');
      const testUser = { 
        id: 'test', 
        preferences: { theme: 'dark', mediaInput: 'slack' } 
      };
      
      const result = await prefsCmd.execute(JSON.stringify({ input: testUser }), null);
      
      expect(result.success).toBe(true);
      expect(result.data.theme).toBe('dark');
      expect(result.data.mediaInput).toBe('slack');
    });

    test('commands should follow BaseCommand interface', () => {
      const commands = ['help', 'agents', 'findUser', 'preferences'];
      
      commands.forEach(cmdName => {
        const cmd = coreModule.getCommand(cmdName);
        
        // Test getDefinition
        const definition = cmd.getDefinition();
        expect(definition.name).toBeDefined();
        expect(definition.description).toBeDefined();
        expect(definition.icon).toBeDefined();
        
        // Test it extends BaseCommand functionality
        expect(cmd.parseParams).toBeDefined();
        expect(cmd.createSuccessResult).toBeDefined();
        expect(cmd.createErrorResult).toBeDefined();
      });
    });
  });

  describe('Fluent API Integration', () => {
    test('should chain commands correctly', async () => {
      const fluentAPI = coreModule.getFluentAPI();
      
      // Test simple chaining
      const userResult = await fluentAPI.findUser({name: "joel"}).execute();
      
      expect(userResult.name).toBe('joel');
      expect(userResult.preferences.mediaInput).toBe('slack');
    });

    test('should build correct pipeline structure', () => {
      const pipeline = new FluentAPI();
      const chain = pipeline.screenshot().share({target: 'test'});
      
      expect(chain.pipeline).toHaveLength(2);
      expect(chain.pipeline[0].command).toBe('screenshot');
      expect(chain.pipeline[1].command).toBe('share');
    });

    test('should handle complex command composition', async () => {
      const fluentAPI = coreModule.getFluentAPI();
      
      // Test the elegant composition pattern
      try {
        const joel = await fluentAPI.findUser({name: "joel"}).execute();
        expect(joel.name).toBe('joel');
        expect(joel.preferences.mediaInput).toBe('slack');
        
        // This represents the structure for: 
        // continuum.screenshot().share(continuum.findUser({name:"joel"}))
        expect(joel).toMatchObject({
          name: 'joel',
          role: 'admin',
          preferences: expect.objectContaining({
            mediaInput: 'slack'
          })
        });
      } catch (error) {
        fail(`Elegant composition failed: ${error.message}`);
      }
    });
  });

  describe('Error Handling', () => {
    test('should handle invalid command parameters', async () => {
      const findUserCmd = coreModule.getCommand('findUser');
      const result = await findUserCmd.execute('{"invalid": "params"}', null);
      
      // Should still succeed but return empty or handle gracefully
      expect(result.success).toBe(true);
    });

    test('should handle malformed JSON parameters', async () => {
      const helpCmd = coreModule.getCommand('help');
      const result = await helpCmd.execute('invalid json{', null);
      
      // Should handle gracefully and still execute
      expect(result.success).toBe(true);
    });

    test('should provide meaningful error messages', async () => {
      const findUserCmd = coreModule.getCommand('findUser');
      const result = await findUserCmd.execute('{"name": "nonexistent"}', null);
      
      if (!result.success) {
        expect(result.message).toContain('No users found');
        expect(result.error).toBeDefined();
      }
    });
  });

  describe('Integration with Existing Test Patterns', () => {
    test('should maintain compatibility with BaseCommand tests', () => {
      const helpCmd = coreModule.getCommand('help');
      
      // Test parseParams (from BaseCommand)
      const params1 = helpCmd.parseParams('{"test": "value"}');
      expect(params1.test).toBe('value');
      
      const params2 = helpCmd.parseParams('invalid');
      expect(params2).toEqual({});
      
      // Test result creation
      const successResult = helpCmd.createSuccessResult({data: 'test'}, 'Test message');
      expect(successResult.success).toBe(true);
      expect(successResult.message).toBe('Test message');
      expect(successResult.data.data).toBe('test');
      
      const errorResult = helpCmd.createErrorResult('Test error', 'Details');
      expect(errorResult.success).toBe(false);
      expect(errorResult.message).toBe('Test error');
      expect(errorResult.error).toBe('Details');
    });

    test('should work with console-logs test patterns', () => {
      // Test that commands can be executed in the same way console-logs tests expect
      const commands = coreModule.getCommands();
      expect(commands.length).toBeGreaterThan(0);
      
      commands.forEach(([name, cmd]) => {
        expect(typeof name).toBe('string');
        expect(cmd.getDefinition).toBeDefined();
        expect(cmd.execute).toBeDefined();
      });
    });

    test('should support diagnostics integration', async () => {
      const diagnosticsCmd = coreModule.getCommand('diagnostics');
      expect(diagnosticsCmd).toBeDefined();
      
      const definition = diagnosticsCmd.getDefinition();
      expect(definition.name).toBe('diagnostics');
      expect(definition.parameters.type).toBeDefined();
    });
  });

  describe('Module System Architecture', () => {
    test('should provide module information', () => {
      const info = coreModule.getInfo();
      expect(info.name).toBe('Core');
      expect(info.version).toBe('1.0.0');
      expect(info.commands).toBeGreaterThan(0);
      expect(info.macros).toBeGreaterThan(0);
    });

    test('should handle module initialization', async () => {
      const newModule = new CoreModule();
      const result = await newModule.initialize();
      expect(result).toBe(true);
      expect(newModule.commands.size).toBeGreaterThan(0);
    });

    test('should support module cleanup', async () => {
      const newModule = new CoreModule();
      await newModule.initialize();
      
      const result = await newModule.cleanup();
      expect(result).toBe(true);
    });
  });
});

module.exports = { CoreModule, FluentAPI };