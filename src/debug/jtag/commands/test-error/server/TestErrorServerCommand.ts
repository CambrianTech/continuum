/**
 * Test Error Command - Server Implementation
 * 
 * Command that intentionally generates different types of errors
 * to validate error handling flow and session mapping
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
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
        errorType: 'none',
        message: 'Test completed successfully - no error thrown'
      });
    }
    
    // Generate different types of errors
    switch (errorType) {
      case 'validation':
        console.error(`❌ SERVER: Validation error about to be thrown`);
        throw new Error(customMessage || 'Validation failed: Invalid test parameters provided');
        
      case 'network':
        console.error(`❌ SERVER: Network error about to be thrown`);
        throw new Error(customMessage || 'Network error: Failed to connect to external service');
        
      case 'permission':
        console.error(`❌ SERVER: Permission error about to be thrown`);
        throw new Error(customMessage || 'Permission denied: Insufficient privileges for operation');
        
      case 'generic':
      default:
        console.error(`❌ SERVER: Generic error about to be thrown`);
        throw new Error(customMessage || 'Test error: This is an intentional error for testing error handling');
    }
  }
}