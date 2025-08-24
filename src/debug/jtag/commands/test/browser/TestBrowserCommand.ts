// ISSUES: 0 open, last updated 2025-08-24 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Test Command - Browser Implementation
 * 
 * Browser delegates test execution to server (can't run Node.js tests directly)
 */

import { type TestParams, type TestResult, createTestResult } from '../shared/TestTypes';
import { NetworkError } from '../../../system/core/types/ErrorTypes';
import { TestCommand } from '../shared/TestCommand';

export class TestBrowserCommand extends TestCommand {
  
  /**
   * Browser delegates test execution to server
   */
  async execute(params: TestParams): Promise<TestResult> {
    console.log(`🧪 BROWSER: Test execution → delegating to server`);

    try {
      // Browser always delegates test execution to server
      console.log(`🔀 BROWSER: Need Node.js environment → delegating to server`);
      
      if (params.file ?? params._?.[0]) {
        const testFile = params.file ?? params._?.[0];
        console.log(`🧪 BROWSER: Running test file "${testFile}"`);
      } else {
        console.log(`🧪 BROWSER: Running full npm test suite`);
      }
      
      return await this.remoteExecute(params);

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ BROWSER: Test execution delegation failed:`, errorMessage);
      const testError = error instanceof Error ? new NetworkError('server', error.message, { cause: error }) : new NetworkError('server', String(error));
      
      return createTestResult(params.context, params.sessionId, {
        success: false,
        output: `Browser delegation failed: ${errorMessage}`,
        duration: 0,
        command: params.file ? `npx tsx ${params.file}` : 'npm test',
        error: testError
      });
    }
  }
}