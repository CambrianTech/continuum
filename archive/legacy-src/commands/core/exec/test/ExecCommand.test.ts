/**
 * Unit Test: Exec Command
 * 
 * Tests strongly typed command execution interface
 * Following the middle-out testing methodology for Layer 3: Command System
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { ExecCommand } from '../ExecCommand.js';
import type { ContinuumContext } from '../../../../types/shared/core/ContinuumTypes.js';

describe('ExecCommand', () => {
  let mockContext: ContinuumContext;

  beforeEach(() => {
    mockContext = {
      sessionId: 'test-session',
      environment: 'server' as const
    };
  });

  describe('Command Definition', () => {
    it('should have proper command metadata', () => {
      expect(ExecCommand.definition.name).toBe('exec');
      expect(ExecCommand.definition.category).toBe('core');
      expect(ExecCommand.definition.description).toContain('strongly typed');
    });

    it('should define expected parameters', () => {
      const params = ExecCommand.definition.parameters;
      expect(params.command).toBeDefined();
      expect(params.args).toBeDefined();
      expect(params.execution).toBeDefined();
    });
  });

  describe('Parameter Parsing', () => {
    it('should parse CLI args with command parameter', async () => {
      const params = { args: ['--command=help'] };
      
      const result = await ExecCommand.execute(params, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.execution.command).toBe('help');
    });

    it('should parse object parameters directly', async () => {
      const params = { command: 'test-cmd', args: ['arg1'] };
      
      const result = await ExecCommand.execute(params, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.execution.command).toBe('test-cmd');
      expect(result.data.execution.args).toEqual(['arg1']);
    });

    it('should handle missing command parameter', async () => {
      const params = { args: [] };
      
      const result = await ExecCommand.execute(params, mockContext);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Must provide either command name or execution object');
    });
  });

  describe('Strongly Typed Interface', () => {
    it('should create CommandExecution with proper structure', async () => {
      const params = { command: 'test', args: ['arg1'] };
      
      const result = await ExecCommand.execute(params, mockContext);
      
      expect(result.success).toBe(true);
      
      const execution = result.data.execution;
      expect(execution).toMatchObject({
        command: 'test',
        args: ['arg1'],
        context: {
          source: 'api',
          transport: 'http'
        },
        timestamp: expect.any(String),
        requestId: expect.any(String)
      });
    });

    it('should include execution metadata', async () => {
      const params = { command: 'help' };
      
      const result = await ExecCommand.execute(params, mockContext);
      
      expect(result.data.metadata).toMatchObject({
        executionTime: expect.any(Number),
        processor: 'exec-command',
        timestamp: expect.any(String)
      });
    });
  });

  describe('Error Handling', () => {
    it('should handle null parameters gracefully', async () => {
      const result = await ExecCommand.execute(null, mockContext);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Parameters must be a non-null object');
    });

    it('should handle undefined parameters gracefully', async () => {
      const result = await ExecCommand.execute(undefined, mockContext);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Parameters must be a non-null object');
    });
  });
});