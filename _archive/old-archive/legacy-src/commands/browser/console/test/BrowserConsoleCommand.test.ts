/**
 * Unit Test: Browser Console Command
 * 
 * Tests Layer 7: Browser console interaction with session integration
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { BrowserConsoleCommand } from '../BrowserConsoleCommand.js';

describe('BrowserConsoleCommand', () => {
  let command: BrowserConsoleCommand;

  beforeEach(() => {
    command = new BrowserConsoleCommand();
    
    // Mock console methods
    vi.spyOn(console, 'log').mockImplementation(() => {});
    vi.spyOn(console, 'warn').mockImplementation(() => {});
  });

  describe('Command Definition', () => {
    it('should have proper command definition', () => {
      const definition = BrowserConsoleCommand.getDefinition();
      
      expect(definition.name).toBe('browser-console');
      expect(definition.category).toBe('browser');
      expect(definition.parameters.action.required).toBe(true);
      expect(definition.parameters.action.enum).toEqual(['read', 'clear', 'execute', 'monitor']);
      expect(definition.examples).toHaveLength(3);
    });
  });

  describe('Read Action', () => {
    it('should read console output from session', async () => {
      const result = await command.execute({
        action: 'read',
        sessionId: 'test-session',
        lines: 10,
        filter: 'all'
      });

      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe('test-session');
      expect(result.data?.filter).toBe('all');
      expect(result.data?.output).toBeDefined();
      expect(Array.isArray(result.data?.output)).toBe(true);
    });

    it('should filter console output by level', async () => {
      const result = await command.execute({
        action: 'read',
        filter: 'error'
      });

      expect(result.success).toBe(true);
      expect(result.data?.filter).toBe('error');
    });

    it('should limit output lines', async () => {
      const result = await command.execute({
        action: 'read',
        lines: 5
      });

      expect(result.success).toBe(true);
      expect(result.data?.lines).toBeLessThanOrEqual(5);
    });
  });

  describe('Clear Action', () => {
    it('should clear browser console', async () => {
      const result = await command.execute({
        action: 'clear',
        sessionId: 'test-session'
      });

      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe('test-session');
      expect(result.data?.clearUUID).toMatch(/^clear-\d+-[a-z0-9]+$/);
      expect(result.message).toContain('Console cleared');
    });
  });

  describe('Execute Action', () => {
    it('should execute console script with UUID tracking', async () => {
      const result = await command.execute({
        action: 'execute',
        script: 'console.log("test message");',
        sessionId: 'test-session'
      });

      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe('test-session');
      expect(result.data?.executionUUID).toMatch(/^console-exec-\d+-[a-z0-9]+$/);
      expect(result.data?.script).toBe('console.log("test message");');
    });

    it('should require script parameter for execute action', async () => {
      const result = await command.execute({
        action: 'execute'
        // Missing script parameter
      });

      expect(result.success).toBe(false);
      expect(result.error).toContain('Script parameter required');
    });

    it('should truncate long scripts in result', async () => {
      const longScript = 'console.log("' + 'a'.repeat(200) + '");';
      
      const result = await command.execute({
        action: 'execute',
        script: longScript
      });

      expect(result.success).toBe(true);
      expect(result.data?.script.length).toBeLessThanOrEqual(103); // Truncated
    });
  });

  describe('Monitor Action', () => {
    it('should start console monitoring', async () => {
      const result = await command.execute({
        action: 'monitor',
        sessionId: 'test-session',
        filter: 'error',
        follow: true
      });

      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe('test-session');
      expect(result.data?.monitorId).toMatch(/^monitor-\d+-[a-z0-9]+$/);
      expect(result.data?.filter).toBe('error');
      expect(result.data?.follow).toBe(true);
      expect(result.data?.monitoring).toBe('active');
    });

    it('should take monitoring snapshot when follow is false', async () => {
      const result = await command.execute({
        action: 'monitor',
        follow: false
      });

      expect(result.success).toBe(true);
      expect(result.data?.monitoring).toBe('snapshot');
    });
  });

  describe('Git Hook Pattern Integration', () => {
    it('should generate trackable UUIDs for operations', async () => {
      const clearResult = await command.execute({
        action: 'clear'
      });

      const executeResult = await command.execute({
        action: 'execute',
        script: 'console.log("test");'
      });

      expect(clearResult.data?.clearUUID).toBeDefined();
      expect(executeResult.data?.executionUUID).toBeDefined();
      
      // UUIDs should be unique
      expect(clearResult.data?.clearUUID).not.toBe(executeResult.data?.executionUUID);
    });

    it('should support session-based console output reading', async () => {
      const result = await command.execute({
        action: 'read',
        sessionId: 'validation-session'
      });

      expect(result.success).toBe(true);
      expect(result.data?.output).toBeDefined();
      
      // Should contain simulated git hook pattern output
      const output = result.data?.output;
      expect(output.some((line: string) => line.includes('🌐'))).toBe(true);
      expect(output.some((line: string) => line.includes('EXECUTION_UUID'))).toBe(true);
    });
  });

  describe('Error Handling', () => {
    it('should handle unknown actions', async () => {
      const result = await command.execute({
        action: 'unknown-action' as any
      });

      expect(result.success).toBe(false);
      expect(result.error).toContain('Unknown console action');
    });

    it('should handle execution errors gracefully', async () => {
      // Mock executeInBrowser to throw
      const originalExecute = command['executeInBrowser'];
      command['executeInBrowser'] = vi.fn().mockRejectedValue(new Error('Browser error'));

      const result = await command.execute({
        action: 'clear'
      });

      expect(result.success).toBe(false);
      expect(result.error).toContain('Failed to clear console');

      // Restore original method
      command['executeInBrowser'] = originalExecute;
    });
  });

  describe('Session Integration', () => {
    it('should auto-detect session when not provided', async () => {
      const result = await command.execute({
        action: 'read'
      });

      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe('auto-detected');
    });

    it('should use provided session ID', async () => {
      const result = await command.execute({
        action: 'read',
        sessionId: 'custom-session'
      });

      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe('custom-session');
    });
  });
});