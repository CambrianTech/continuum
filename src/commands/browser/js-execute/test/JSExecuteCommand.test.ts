/**
 * Unit Test: JavaScript Execute Command
 * 
 * Tests Layer 7: JavaScript execution with session logging integration
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { JSExecuteCommand } from '../JSExecuteCommand.js';

describe('JSExecuteCommand', () => {
  let command: JSExecuteCommand;

  beforeEach(() => {
    command = new JSExecuteCommand();
    
    // Mock console.log to capture session logging
    vi.spyOn(console, 'log').mockImplementation(() => {});
    vi.spyOn(console, 'warn').mockImplementation(() => {});
  });

  describe('Command Definition', () => {
    it('should have proper command definition', () => {
      const definition = JSExecuteCommand.getDefinition();
      
      expect(definition.name).toBe('js-execute');
      expect(definition.category).toBe('browser');
      expect(definition.parameters.script.required).toBe(true);
      expect(definition.parameters.generateUUID.default).toBe(true);
      expect(definition.examples).toHaveLength(2);
    });
  });

  describe('Basic Execution', () => {
    it('should execute simple JavaScript', async () => {
      const result = await command.execute({
        script: 'console.log("Hello World");',
        sessionId: 'test-session'
      });

      expect(result.success).toBe(true);
      expect(result.data?.executionUUID).toBeDefined();
      expect(result.data?.sessionId).toBe('test-session');
      expect(result.data?.script).toBe('console.log("Hello World");');
      expect(result.message).toContain('JavaScript executed successfully');
    });

    it('should generate UUID by default', async () => {
      const result = await command.execute({
        script: 'console.log("test");'
      });

      expect(result.success).toBe(true);
      expect(result.data?.executionUUID).toMatch(/^exec-\d+-[a-z0-9]+$/);
    });

    it('should skip UUID generation when disabled', async () => {
      const result = await command.execute({
        script: 'console.log("test");',
        generateUUID: false
      });

      expect(result.success).toBe(true);
      expect(result.data?.executionUUID).toBeNull();
    });

    it('should auto-detect session ID when not provided', async () => {
      const result = await command.execute({
        script: 'console.log("test");'
      });

      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe('auto-detected');
    });
  });

  describe('Script Wrapping', () => {
    it('should wrap script with UUID logging', async () => {
      const result = await command.execute({
        script: 'const x = 1;',
        generateUUID: true,
        logExecution: true
      });

      expect(result.success).toBe(true);
      expect(result.data?.executionUUID).toBeDefined();
      
      // Should have logged execution start and completion
      expect(console.log).toHaveBeenCalledWith(
        expect.stringContaining('🚀 JavaScript execution started')
      );
      expect(console.log).toHaveBeenCalledWith(
        expect.stringContaining('✅ JavaScript execution completed')
      );
    });

    it('should handle execution without logging', async () => {
      const result = await command.execute({
        script: 'const x = 1;',
        generateUUID: false,
        logExecution: false
      });

      expect(result.success).toBe(true);
      expect(result.data?.executionUUID).toBeNull();
    });

    it('should truncate long scripts in result', async () => {
      const longScript = 'a'.repeat(200);
      
      const result = await command.execute({
        script: longScript
      });

      expect(result.success).toBe(true);
      expect(result.data?.script).toHaveLength(103); // 100 chars + '...'
      expect(result.data?.script).toMatch(/\.\.\.$/); // ends with ...
    });
  });

  describe('Error Handling', () => {
    it('should handle execution errors gracefully', async () => {
      // Mock execution to throw error
      const originalExecute = command['executeInBrowser'];
      command['executeInBrowser'] = vi.fn().mockRejectedValue(new Error('Browser connection failed'));

      const result = await command.execute({
        script: 'console.log("test");',
        sessionId: 'test-session'
      });

      expect(result.success).toBe(false);
      expect(result.error).toContain('JavaScript execution failed: Browser connection failed');
      expect(result.data?.sessionId).toBe('test-session');
      
      // Should have logged error to session
      expect(console.log).toHaveBeenCalledWith(
        expect.stringContaining('❌ JavaScript execution failed')
      );

      // Restore original method
      command['executeInBrowser'] = originalExecute;
    });

    it('should handle session logging errors gracefully', async () => {
      // Mock session logging to throw
      const originalLogToSession = command['logToSession'];
      command['logToSession'] = vi.fn().mockImplementation(async () => {
        throw new Error('Session logging failed');
      });

      const result = await command.execute({
        script: 'console.log("test");'
      });

      // Should still succeed even if session logging fails
      expect(result.success).toBe(true);
      
      // Restore original method
      command['logToSession'] = originalLogToSession;
    });
  });

  describe('Configuration Options', () => {
    it('should respect timeout setting', async () => {
      const result = await command.execute({
        script: 'console.log("test");',
        timeout: 5000
      });

      expect(result.success).toBe(true);
      // Timeout is passed to executeInBrowser (tested via integration)
    });

    it('should handle waitForResult setting', async () => {
      const result = await command.execute({
        script: 'console.log("test");',
        waitForResult: false
      });

      expect(result.success).toBe(true);
      // waitForResult affects execution behavior (tested via integration)
    });

    it('should use default values correctly', async () => {
      const result = await command.execute({
        script: 'console.log("test");'
      });

      expect(result.success).toBe(true);
      expect(result.data?.executionUUID).toBeDefined(); // generateUUID defaults to true
    });
  });

  describe('Git Hook Pattern Integration', () => {
    it('should generate trackable UUID format', async () => {
      const result = await command.execute({
        script: 'console.log("validation test");',
        sessionId: 'validation-session'
      });

      expect(result.success).toBe(true);
      const uuid = result.data?.executionUUID;
      expect(uuid).toMatch(/^exec-\d+-[a-z0-9]+$/);
      
      // UUID should be trackable in session logs
      expect(console.log).toHaveBeenCalledWith(
        expect.stringContaining(`[UUID: ${uuid}]`)
      );
    });

    it('should support verification workflow', async () => {
      // Simulate git hook verification pattern
      const testUUID = `verification-${Date.now()}`;
      
      const result = await command.execute({
        script: `console.log('🎯 VERIFICATION_${testUUID}_MARKER');`,
        sessionId: 'git-hook-session',
        generateUUID: true,
        logExecution: true
      });

      expect(result.success).toBe(true);
      expect(result.data?.executionUUID).toBeDefined();
      
      // Should generate trackable logs for verification
      expect(console.log).toHaveBeenCalledWith(
        expect.stringContaining('JavaScript execution started')
      );
      expect(console.log).toHaveBeenCalledWith(
        expect.stringContaining('JavaScript execution completed')
      );
    });
  });
});