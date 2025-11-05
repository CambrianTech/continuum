/**
 * CLI Client Parser Integration Tests
 * 
 * Tests the CLI parser with real command execution and output formatting.
 * These tests validate the parser works with the existing screenshot command
 * and other Continuum commands.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { CLIClientParser } from '../../client/CLIClientParser';
import { CLIInputFormat, CLIOutputFormat } from '../../shared/CLIParserTypes';

describe('CLI Client Parser Integration', () => {
  let parser: CLIClientParser;
  let consoleSpy: any;

  beforeEach(() => {
    parser = new CLIClientParser();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    consoleSpy.mockRestore();
  });

  describe('Input Parsing', () => {
    it('should parse CLI arguments with flags and values', () => {
      const input: CLIInputFormat = {
        args: ['--querySelector=body', '--filename=test.png', '--width=800', '--height=600']
      };

      const result = parser.parseInput(input);

      expect(result).toEqual({
        querySelector: 'body',
        filename: 'test.png',
        width: '800',
        height: '600'
      });
    });

    it('should parse CLI arguments with boolean flags', () => {
      const input: CLIInputFormat = {
        args: ['--verbose', '--json', '--help']
      };

      const result = parser.parseInput(input);

      expect(result).toEqual({
        verbose: true,
        json: true,
        help: true
      });
    });

    it('should parse CLI arguments with positional arguments', () => {
      const input: CLIInputFormat = {
        args: ['screenshot', 'body', '--filename=test.png']
      };

      const result = parser.parseInput(input);

      expect(result).toEqual({
        args: ['screenshot', 'body'],
        filename: 'test.png'
      });
    });

    it('should handle mixed argument formats', () => {
      const input: CLIInputFormat = {
        args: ['--command', 'screenshot', '--selector=body', '--verbose', 'extra-arg']
      };

      const result = parser.parseInput(input);

      expect(result).toEqual({
        command: 'screenshot',
        selector: 'body',
        verbose: true,
        args: ['extra-arg']
      });
    });
  });

  describe('Output Formatting', () => {
    it('should format screenshot command output', () => {
      const output: CLIOutputFormat = {
        success: true,
        data: {
          broadcastSent: true,
          connectionCount: 1
        },
        executionTime: 1500
      };

      parser.formatOutput(output, 'screenshot');

      expect(consoleSpy).toHaveBeenCalledWith('\n📸 SCREENSHOT CAPTURED');
      expect(consoleSpy).toHaveBeenCalledWith('✅ Status: Success');
      expect(consoleSpy).toHaveBeenCalledWith('📡 Broadcast sent to 1 browser connection(s)');
      expect(consoleSpy).toHaveBeenCalledWith('💾 Screenshot saved to session directory');
      expect(consoleSpy).toHaveBeenCalledWith('⏱️ Execution time: 1.5s');
    });

    it('should format screenshot command output with file details', () => {
      const output: CLIOutputFormat = {
        success: true,
        data: {
          filename: 'test.png',
          filePath: '/path/to/test.png',
          width: 800,
          height: 600
        }
      };

      parser.formatOutput(output, 'screenshot');

      expect(consoleSpy).toHaveBeenCalledWith('📁 File: test.png');
      expect(consoleSpy).toHaveBeenCalledWith('💾 Saved to: /path/to/test.png');
      expect(consoleSpy).toHaveBeenCalledWith('📏 Dimensions: 800x600px');
    });

    it('should format chat command output', () => {
      const output: CLIOutputFormat = {
        success: true,
        data: {
          response: 'Hello, how can I help you?'
        }
      };

      parser.formatOutput(output, 'chat');

      expect(consoleSpy).toHaveBeenCalledWith('Hello, how can I help you?');
    });

    it('should format generic command output', () => {
      const output: CLIOutputFormat = {
        success: true,
        data: {
          message: 'Command executed successfully',
          result: 'some-result'
        }
      };

      parser.formatOutput(output, 'generic');

      expect(consoleSpy).toHaveBeenCalledWith('\n⚡ GENERIC RESULT');
      expect(consoleSpy).toHaveBeenCalledWith('✅ Status: Success');
    });

    it('should format error output', () => {
      const output: CLIOutputFormat = {
        success: false,
        error: 'Command failed to execute'
      };

      parser.formatOutput(output, 'test');

      expect(consoleSpy).toHaveBeenCalledWith('❌ Status: Failed');
      expect(consoleSpy).toHaveBeenCalledWith('❌ Error: Command failed to execute');
    });
  });

  describe('Validation', () => {
    it('should validate correct input format', () => {
      const input: CLIInputFormat = {
        args: ['--test=value']
      };

      const result = parser.validateInput(input);

      expect(result.success).toBe(true);
    });

    it('should reject invalid input format', () => {
      const input = null as any;

      const result = parser.validateInput(input);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Invalid input format');
    });

    it('should validate correct output format', () => {
      const output: CLIOutputFormat = {
        success: true,
        data: { test: 'value' }
      };

      const result = parser.validateOutput(output);

      expect(result.success).toBe(true);
    });

    it('should reject invalid output format', () => {
      const output = { success: 'invalid' } as any;

      const result = parser.validateOutput(output);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Success field must be boolean');
    });
  });

  describe('Parser Capabilities', () => {
    it('should provide parser information', () => {
      const info = parser.getParserInfo();

      expect(info.name).toBe('CLI Client Parser');
      expect(info.version).toBe('1.0.0');
      expect(info.supportedCommands).toEqual(['*']);
    });

    it('should handle CLI input format', () => {
      const input: CLIInputFormat = {
        args: ['--test=value']
      };

      expect(parser.canHandle(input)).toBe(true);
    });

    it('should reject non-CLI input format', () => {
      const input = { notArgs: 'value' } as any;

      expect(parser.canHandle(input)).toBe(false);
    });

    it('should have high priority for CLI integration', () => {
      expect(parser.getPriority()).toBe(100);
    });
  });

  describe('Real Command Integration', () => {
    it('should work with screenshot command parameters', () => {
      const input: CLIInputFormat = {
        args: ['--querySelector=body', '--filename=integration-test.png', '--width=1024', '--height=768']
      };

      const params = parser.parseInput(input);

      // These are the exact parameters that screenshot command expects
      expect(params.querySelector).toBe('body');
      expect(params.filename).toBe('integration-test.png');
      expect(params.width).toBe('1024');
      expect(params.height).toBe('768');
    });

    it('should work with chat command parameters', () => {
      const input: CLIInputFormat = {
        args: ['--message=Hello AI', '--model=claude-3', '--temperature=0.7']
      };

      const params = parser.parseInput(input);

      expect(params.message).toBe('Hello AI');
      expect(params.model).toBe('claude-3');
      expect(params.temperature).toBe('0.7');
    });

    it('should work with help command parameters', () => {
      const input: CLIInputFormat = {
        args: ['screenshot', '--verbose']
      };

      const params = parser.parseInput(input);

      expect(params.args).toEqual(['screenshot']);
      expect(params.verbose).toBe(true);
    });
  });
});