// ISSUES: 0 open, last updated 2025-08-24 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Test Command - Server Implementation
 * 
 * Server can execute tests directly using Node.js child process
 * 
 * Usage:
 *   ./jtag test                           # Run npm test (full suite)
 *   ./jtag test tests/something.test.ts   # Run specific test file
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { type TestParams, type TestResult, createTestResult } from '../shared/TestTypes';
import { TestCommand } from '../shared/TestCommand';

const execAsync = promisify(exec);

export class TestServerCommand extends TestCommand {
  
  async execute(params: TestParams): Promise<TestResult> {
    const startTime = Date.now();
    
    // Determine what to run
    let command: string;
    if (params.file || (params._ && params._?.length > 0)) {
      const testFile = params.file ?? params._?.[0];
      command = `npx tsx ${testFile}`;
      console.log(`🧪 SERVER: Running test file: ${testFile}`);
    } else {
      command = 'npm test';
      console.log('🧪 SERVER: Running full test suite...');
    }
    
    try {
      const timeout = params.timeout ?? 300000; // 5 minutes
      console.log(`⚡ SERVER: Executing: ${command}`);
      
      const { stdout, stderr } = await execAsync(command, {
        timeout,
        maxBuffer: 1024 * 1024 * 10
      });
      
      const duration = Date.now() - startTime;
      const output = stdout + (stderr ? '\n' + stderr : '');
      
      console.log(`✅ SERVER: Test completed in ${duration}ms`);
      
      return createTestResult(params.context, params.sessionId, {
        success: true,
        output,
        duration,
        command
      });
      
    } catch (error: unknown) {
      const duration = Date.now() - startTime;
      
      // Type-safe error handling for exec errors
      interface ExecError extends Error {
        stdout?: string;
        stderr?: string;
        code?: string;
      }
      
      const execError = error as ExecError;
      const output = (execError.stdout ?? '') + (execError.stderr ?? '');
      
      if (execError.code === 'ETIMEDOUT') {
        console.error(`❌ TIMEOUT: Test failed to complete within ${params.timeout ?? 300000}ms - test cancelled`);
      } else {
        console.error(`❌ SERVER: Test failed (${duration}ms)`);
      }
      
      return createTestResult(params.context, params.sessionId, {
        success: false,
        output,
        duration,
        command,
        error: error instanceof Error ? error : new Error(String(error))
      });
    }
  }
}

