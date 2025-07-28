/**
 * Test Error Command - Browser Implementation
 * 
 * Browser-side error generation for testing cross-context error handling
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import { type TestErrorParams, type TestErrorResult, createTestErrorResult } from '../shared/TestErrorTypes';

export class TestErrorBrowserCommand extends CommandBase<TestErrorParams, TestErrorResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('test-error', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<TestErrorResult> {
    const testParams = params as TestErrorParams;
    
    console.log(`🧪 BROWSER: Test error command starting with type: ${testParams.errorType || 'generic'}`);
    console.log(`🧪 BROWSER: Should throw: ${testParams.shouldThrow !== false}`);
    
    const errorType = testParams.errorType || 'generic';
    const customMessage = testParams.errorMessage;
    
    // If shouldThrow is explicitly false, return success instead
    if (testParams.shouldThrow === false) {
      console.log(`✅ BROWSER: Test error command completed successfully (no throw requested)`);
      return createTestErrorResult(testParams.context, testParams.sessionId, {
        success: true,
        errorType: 'none',
        message: 'Test completed successfully - no error thrown'
      });
    }
    
    // Generate different types of browser-specific errors
    switch (errorType) {
      case 'validation':
        console.error(`❌ BROWSER: DOM validation error about to be thrown`);
        throw new Error(customMessage || 'DOM validation failed: Element not found or invalid');
        
      case 'network':
        console.error(`❌ BROWSER: Browser network error about to be thrown`);
        throw new Error(customMessage || 'Browser network error: Failed to load resource');
        
      case 'permission':
        console.error(`❌ BROWSER: Browser permission error about to be thrown`);
        throw new Error(customMessage || 'Browser permission denied: Cannot access required API');
        
      case 'generic':
      default:
        console.error(`❌ BROWSER: Generic browser error about to be thrown`);
        throw new Error(customMessage || 'Browser test error: This is an intentional browser error');
    }
  }
}