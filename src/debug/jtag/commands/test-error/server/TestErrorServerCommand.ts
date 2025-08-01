/**
 * Test Error Command - Server Implementation
 * 
 * Command that intentionally generates different types of errors
 * to validate error handling flow and session mapping
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { type TestErrorParams, type TestErrorResult, createTestErrorResult } from '../shared/TestErrorTypes';

export class TestErrorServerCommand extends CommandBase<TestErrorParams, TestErrorResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('test-error', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<TestErrorResult> {
    const testParams = params as TestErrorParams;
    
    console.log(`🧪 SERVER: Test error command starting with type: ${testParams.errorType || 'generic'}`);
    console.log(`🧪 SERVER: Should throw: ${testParams.shouldThrow !== false}`);
    
    const errorType = testParams.errorType || 'generic';
    const customMessage = testParams.errorMessage;
    
    // If shouldThrow is explicitly false, return success instead
    if (testParams.shouldThrow === false) {
      console.log(`✅ SERVER: Test error command completed successfully (no throw requested)`);
      return createTestErrorResult(testParams.context, testParams.sessionId, {
        success: true,
        errorType: 'generic',
        message: 'Test completed successfully - no error thrown'
      });
    }
    
    // Add delay if specified
    if (testParams.delay && testParams.delay > 0) {
      console.log(`⏱️  SERVER: Delaying error for ${testParams.delay}ms`);
      await new Promise(resolve => setTimeout(resolve, testParams.delay));
    }
    
    // Generate different types of server-specific errors
    switch (errorType) {
      case 'validation-error':
        console.error(`❌ SERVER: Validation error about to be thrown`);
        throw new Error(customMessage || 'Server validation failed: Invalid test parameters provided');
        
      case 'network-error':
        console.error(`❌ SERVER: Network error about to be thrown`);
        throw new Error(customMessage || 'Server network error: Failed to connect to external service');
        
      case 'permission-error':
        console.error(`❌ SERVER: Permission error about to be thrown`);
        throw new Error(customMessage || 'Server permission denied: Insufficient privileges for operation');
        
      case 'environment-error':
        console.error(`❌ SERVER: Environment error about to be thrown`);
        throw new Error(customMessage || 'Server environment error: Required Node.js module not available');
        
      case 'timeout-error':
        console.error(`❌ SERVER: Timeout error about to be thrown`);
        throw new Error(customMessage || 'Server timeout error: Operation exceeded time limit');
        
      case 'async-error':
        console.error(`❌ SERVER: Async error about to be thrown`);
        await new Promise((_, reject) => setTimeout(() => reject(new Error(customMessage || 'Server async error: Rejected promise')), 10));
        
      case 'json-error':
        console.error(`❌ SERVER: JSON parsing error about to be thrown`);
        try {
          JSON.parse('{invalid: json, missing: "quotes"}');
        } catch (e) {
          const error = e instanceof Error ? e : new Error(String(e));
          throw new Error(customMessage || `Server JSON error: ${error.message}`);
        }
        
      case 'custom-error':
        console.error(`❌ SERVER: Custom error about to be thrown`);
        throw new Error(customMessage || 'Server custom error: User-defined error condition');
        
      case 'execution-error':
        console.error(`❌ SERVER: Execution error about to be thrown`);
        // Simulate a runtime execution error
        const invalidFunction: any = null;
        invalidFunction(); // This will throw
        
      case 'generic':
      default:
        console.error(`❌ SERVER: Generic error about to be thrown`);
        throw new Error(customMessage || 'Server test error: This is an intentional error for testing error handling');
    }
  }
}