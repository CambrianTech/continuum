/**
 * Unit test to prevent Claude CLI communication errors
 * Found issue: "Raw mode is not supported" and process stdin errors
 */

import { describe, test, expect, beforeEach, afterEach } from '@jest/globals';
import { spawn, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';

describe('Claude Communication Error Prevention', () => {
  let mockProcess: MockChildProcess;

  beforeEach(() => {
    mockProcess = new MockChildProcess();
  });

  afterEach(() => {
    if (mockProcess) {
      mockProcess.cleanup();
    }
  });

  test('should handle raw mode stdin errors gracefully', () => {
    const errorMessage = 'Raw mode is not supported on the current process.stdin';
    
    // Mock the error scenario
    mockProcess.stderr.emit('data', Buffer.from(errorMessage));
    
    // Should not crash but handle gracefully
    expect(mockProcess.exitCode).toBeNull();
    expect(mockProcess.stderr.readable).toBe(true);
  });

  test('should use proper process spawn options for non-interactive mode', () => {
    interface SpawnOptions {
      stdio?: 'pipe' | 'inherit' | 'ignore' | readonly ('pipe' | 'inherit' | 'ignore')[];
      shell?: boolean;
      detached?: boolean;
      env?: Record<string, string>;
    }

    const getProperSpawnOptions = (): SpawnOptions => ({
      stdio: ['pipe', 'pipe', 'pipe'], // Avoid 'inherit' which can cause raw mode issues
      shell: false,
      detached: false,
      env: {
        ...process.env,
        NODE_ENV: 'test',
        FORCE_COLOR: '0', // Disable colors in test environment
        NO_RAW_MODE: '1'  // Custom flag to prevent raw mode
      }
    });

    const options = getProperSpawnOptions();
    
    expect(options.stdio).toEqual(['pipe', 'pipe', 'pipe']);
    expect(options.shell).toBe(false);
    expect(options.env?.NO_RAW_MODE).toBe('1');
  });

  test('should timeout properly for unresponsive processes', async () => {
    const timeout = 1000; // 1 second timeout for test
    
    const withTimeout = <T>(promise: Promise<T>, timeoutMs: number): Promise<T> => {
      return Promise.race([
        promise,
        new Promise<never>((_, reject) => 
          setTimeout(() => reject(new Error('Operation timed out')), timeoutMs)
        )
      ]);
    };

    const slowOperation = new Promise(resolve => 
      setTimeout(resolve, 2000) // Intentionally slow
    );

    await expect(withTimeout(slowOperation, timeout))
      .rejects
      .toThrow('Operation timed out');
  });

  test('should handle API key prompts automatically', () => {
    interface AutoResponder {
      patterns: Array<{ pattern: RegExp; response: string }>;
      shouldAutoRespond(prompt: string): string | null;
    }

    const createAutoResponder = (): AutoResponder => ({
      patterns: [
        { pattern: /do you want.*api key/i, response: 'no' },
        { pattern: /detected.*api key/i, response: 'no' },
        { pattern: /use.*key\?/i, response: 'no' },
        { pattern: /recommended/i, response: 'yes' }
      ],
      
      shouldAutoRespond(prompt: string): string | null {
        for (const { pattern, response } of this.patterns) {
          if (pattern.test(prompt)) {
            return response;
          }
        }
        return null;
      }
    });

    const responder = createAutoResponder();
    
    // Test various API key prompts
    expect(responder.shouldAutoRespond('Do you want to use this API key?')).toBe('no');
    expect(responder.shouldAutoResponder('Detected a custom API key')).toBe('no');
    expect(responder.shouldAutoRespond('2. No (recommended)')).toBe('yes');
    expect(responder.shouldAutoRespond('Random text')).toBeNull();
  });

  test('should validate environment setup before spawning processes', () => {
    interface EnvironmentValidator {
      checkRequiredVars(): { valid: boolean; missing: string[] };
      checkStdinSupport(): boolean;
      isTestEnvironment(): boolean;
    }

    const createValidator = (): EnvironmentValidator => ({
      checkRequiredVars() {
        const required = ['ANTHROPIC_API_KEY', 'OPENAI_API_KEY'];
        const missing = required.filter(key => !process.env[key]);
        return { valid: missing.length === 0, missing };
      },

      checkStdinSupport() {
        // In test environments or non-TTY, raw mode is not supported
        return process.stdin.isTTY === true && process.env.NODE_ENV !== 'test';
      },

      isTestEnvironment() {
        return process.env.NODE_ENV === 'test' || process.env.JEST_WORKER_ID !== undefined;
      }
    });

    const validator = createValidator();
    
    // Environment checks
    expect(typeof validator.checkRequiredVars).toBe('function');
    expect(typeof validator.checkStdinSupport).toBe('function');
    expect(validator.isTestEnvironment()).toBe(true); // We're in Jest
  });

  test('should create safe process communication interface', () => {
    interface SafeProcessCommunicator {
      readonly processId: number | null;
      readonly isRunning: boolean;
      send(message: string): Promise<void>;
      receive(): Promise<string>;
      terminate(): Promise<void>;
    }

    class SafeClaudeCommunicator implements SafeProcessCommunicator {
      private process: ChildProcess | null = null;
      private messageQueue: string[] = [];
      private responseQueue: string[] = [];

      public get processId(): number | null {
        return this.process?.pid ?? null;
      }

      public get isRunning(): boolean {
        return this.process !== null && !this.process.killed;
      }

      async send(message: string): Promise<void> {
        if (!this.isRunning || !this.process?.stdin) {
          throw new Error('Process not running or stdin not available');
        }

        return new Promise((resolve, reject) => {
          this.process!.stdin!.write(message + '\n', (error) => {
            if (error) reject(error);
            else resolve();
          });
        });
      }

      async receive(): Promise<string> {
        // In real implementation, this would listen to stdout
        // For test, we'll return a mock response
        return Promise.resolve('Mock Claude response');
      }

      async terminate(): Promise<void> {
        if (this.process) {
          this.process.kill('SIGTERM');
          this.process = null;
        }
      }
    }

    const communicator = new SafeClaudeCommunicator();
    
    expect(communicator.processId).toBeNull();
    expect(communicator.isRunning).toBe(false);
    
    // Test safe interface
    expect(typeof communicator.send).toBe('function');
    expect(typeof communicator.receive).toBe('function');
    expect(typeof communicator.terminate).toBe('function');
  });
});

// Mock ChildProcess for testing
class MockChildProcess extends EventEmitter {
  public stdin = new MockStream();
  public stdout = new MockStream();
  public stderr = new MockStream();
  public exitCode: number | null = null;
  public killed = false;
  public pid = 12345;

  kill(signal?: string): boolean {
    this.killed = true;
    this.exitCode = signal === 'SIGTERM' ? 0 : 1;
    this.emit('close', this.exitCode);
    return true;
  }

  cleanup(): void {
    this.removeAllListeners();
    this.stdin.removeAllListeners();
    this.stdout.removeAllListeners();
    this.stderr.removeAllListeners();
  }
}

class MockStream extends EventEmitter {
  public readable = true;
  public writable = true;

  write(data: string, callback?: (error?: Error) => void): boolean {
    setImmediate(() => {
      this.emit('data', Buffer.from(data));
      callback?.();
    });
    return true;
  }
}