/**
 * Test Error Command - Browser Implementation
 * 
 * Browser-side error generation for testing cross-context error handling
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { type TestErrorParams, type TestErrorResult, createTestErrorResult } from '../shared/TestErrorTypes';

export class DebugErrorBrowserCommand extends CommandBase<TestErrorParams, TestErrorResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('debug/error', context, subpath, commander);
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
        errorType: 'generic',
        message: 'Test completed successfully - no error thrown'
      });
    }
    
    // Generate different types of browser-specific errors
    switch (errorType) {
      case 'validation-error':
        console.error(`❌ BROWSER: DOM validation error about to be thrown`);
        throw new Error(customMessage || 'DOM validation failed: Element not found or invalid');
        
      case 'network-error':
        console.error(`❌ BROWSER: Browser network error about to be thrown`);
        throw new Error(customMessage || 'Browser network error: Failed to load resource');
        
      case 'permission-error':
        console.error(`❌ BROWSER: Browser permission error about to be thrown`);
        throw new Error(customMessage || 'Browser permission denied: Cannot access required API');
        
      case 'environment-error':
        console.error(`❌ BROWSER: Browser environment error about to be thrown`);
        throw new Error(customMessage || 'Browser environment error: Required browser API not available');
        
      case 'timeout-error':
        console.error(`❌ BROWSER: Browser timeout error about to be thrown`);
        throw new Error(customMessage || 'Browser timeout error: Operation took too long');
        
      case 'async-error':
        console.error(`❌ BROWSER: Async error about to be thrown`);
        await new Promise(resolve => setTimeout(resolve, 10)); // Small async delay
        throw new Error(customMessage || 'Browser async error: Promise rejection');
        
      case 'json-error':
        console.error(`❌ BROWSER: JSON parsing error about to be thrown`);
        try {
          JSON.parse('{invalid json');
        } catch (e) {
          const error = e instanceof Error ? e : new Error(String(e));
          throw new Error(customMessage || `Browser JSON error: ${error.message}`);
        }
        
      case 'custom-error':
        console.error(`❌ BROWSER: Custom error about to be thrown`);
        throw new Error(customMessage || 'Browser custom error: User-defined error condition');
        
      case 'generic':
      default:
        console.error(`❌ BROWSER: Generic browser error about to be thrown`);
        throw new Error(customMessage || 'Browser test error: This is an intentional browser error');
    }
  }
}