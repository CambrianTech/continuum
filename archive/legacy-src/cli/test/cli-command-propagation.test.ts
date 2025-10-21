/**
 * Test for CLI command propagation to daemon
 * Tests that commands passed to CLI properly reach the daemon with correct names
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { ThinContinuumCLI } from '../continuum-cli';

describe('CLI Command Propagation', () => {
  let cli: ThinContinuumCLI;
  let mockCommandProcessor: any;
  let originalConsoleLog: any;
  let originalConsoleError: any;

  beforeEach(() => {
    cli = new ThinContinuumCLI();
    
    // Mock console to capture JTAG logs
    originalConsoleLog = console.log;
    originalConsoleError = console.error;
    console.log = vi.fn();
    console.error = vi.fn();
    
    // Mock CommandProcessorDaemon
    mockCommandProcessor = {
      executeCommand: vi.fn()
    };
  });

  afterEach(() => {
    console.log = originalConsoleLog;
    console.error = originalConsoleError;
  });

  it('should propagate command name correctly to daemon', async () => {
    // Setup mock to return success
    mockCommandProcessor.executeCommand.mockResolvedValue({
      success: true,
      data: 'test result'
    });

    // Override the CLI's command processor with our mock
    (cli as any).commandProcessor = mockCommandProcessor;

    // Test that 'connect' command is properly propagated
    await cli.executeCommand('connect', []);

    // Verify the command name was passed correctly
    expect(mockCommandProcessor.executeCommand).toHaveBeenCalledWith(
      'connect',  // Command name should be 'connect', NOT undefined
      { args: [] }
    );
  });

  it('should fail fast when command name is undefined', async () => {
    // Test the scenario where command becomes undefined
    let errorThrown = false;
    
    try {
      await cli.executeCommand(undefined as any, []);
    } catch (error) {
      errorThrown = true;
      expect(error.message).toContain('Invalid command name');
      expect(error.message).toContain('undefined');
    }
    
    expect(errorThrown).toBe(true);
  });

  it('should fail fast when command name is empty string', async () => {
    let errorThrown = false;
    
    try {
      await cli.executeCommand('', []);
    } catch (error) {
      errorThrown = true;
      expect(error.message).toContain('Invalid command name');
    }
    
    expect(errorThrown).toBe(true);
  });

  it('should parse screenshot command correctly', () => {
    const result = cli.parseArgs(['screenshot', '--selector=body', '--filename=test.png']);
    
    expect(result.command).toBe('screenshot');
    expect(result.rawArgs).toEqual(['--selector=body', '--filename=test.png']);
  });

  it('should parse flag commands correctly', () => {
    const result = cli.parseArgs(['--screenshot', '--selector=body']);
    
    expect(result.command).toBe('screenshot');
    expect(result.rawArgs).toEqual(['--selector=body']);
  });

  it('should build proper payload for daemon', () => {
    const payload = cli.buildRequestPayload(['--selector=body', '--filename=test.png']);
    
    expect(payload).toEqual({
      args: ['--selector=body', '--filename=test.png']
    });
  });

  it('should demonstrate the undefined command bug', async () => {
    // This test demonstrates the current bug where 'connect' becomes 'undefined'
    // This test should FAIL until we fix the issue
    
    mockCommandProcessor.executeCommand.mockImplementation((commandName, payload) => {
      // Log what the daemon actually receives
      console.log(`🔬 TEST: Daemon received command: "${commandName}" (type: ${typeof commandName})`);
      console.log(`🔬 TEST: Daemon received payload:`, payload);
      
      if (commandName === undefined || commandName === 'undefined') {
        throw new Error(`Command 'undefined' not found`);
      }
      
      return Promise.resolve({ success: true, data: 'test result' });
    });

    (cli as any).commandProcessor = mockCommandProcessor;

    // This should work but currently fails because 'connect' becomes 'undefined'
    let errorThrown = false;
    try {
      await cli.executeCommand('connect', []);
    } catch (error) {
      errorThrown = true;
      console.log(`🔬 TEST: Error caught: ${error.message}`);
    }
    
    // This test documents the current bug - it should NOT throw an error
    // But it will until we fix the command propagation issue
    expect(errorThrown).toBe(false, 'Command should not become undefined');
  });
});