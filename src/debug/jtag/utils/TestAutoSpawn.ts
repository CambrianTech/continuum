/**
 * Test Auto-Spawn Utility
 * 
 * Provides intelligent fallback for tests that need browser deployment.
 * When transport timeouts occur (indicating browser commands failing),
 * automatically spawns the test through the proper framework with browser deployment.
 * 
 * Usage: Wrap any test's main function with withAutoSpawn()
 * Result: Tests become self-healing - they know how to get proper deployment
 */

import { spawn } from 'child_process';
import path from 'path';

interface TransportError extends Error {
  message: string;
}

/**
 * Detects if an error is a transport timeout (browser commands failing)
 */
function isTransportTimeout(error: any): boolean {
  if (!error || typeof error.message !== 'string') return false;
  
  const message = error.message.toLowerCase();
  
  // Common transport timeout patterns
  const timeoutPatterns = [
    'request timeout after',
    'diagnostic timeout after', 
    'screenshot failed with timeout',
    'browser command.*timeout',
    'websocket.*timeout',
    'browser.*server response broken',
    'non-response message type.*request',
    'widget cropping error.*request timeout',
    'timeout after.*ms',
    'transport layer issue confirmed',
    'bidirectional websocket.*broken'
  ];
  
  return timeoutPatterns.some(pattern => {
    const regex = new RegExp(pattern);
    return regex.test(message);
  });
}

/**
 * Gets the current test file path relative to project root
 */
function getCurrentTestFile(): string {
  const currentFile = process.argv[1];
  const projectRoot = path.resolve(__dirname, '..');
  return path.relative(projectRoot, currentFile);
}

/**
 * Auto-spawn wrapper for test functions
 * 
 * Tries to run the test normally, but if transport timeouts occur,
 * automatically spawns the test through the proper framework with browser deployment.
 */
export async function withAutoSpawn<T>(testFunction: () => Promise<T>): Promise<T> {
  try {
    return await testFunction();
  } catch (error) {
    if (isTransportTimeout(error)) {
      const currentFile = getCurrentTestFile();
      
      console.log('');
      console.log('🔧 TRANSPORT TIMEOUT DETECTED - Auto-spawning with browser deployment...');
      console.log(`📋 Original error: ${error.message}`);
      console.log(`🚀 Running: ./scripts/run-categorized-tests.sh single-test ${currentFile}`);
      console.log('✨ Browser deployment will ensure proper test context');
      console.log('');
      
      // Spawn the test through proper framework with FORCED browser deployment
      const child = spawn('./scripts/run-categorized-tests.sh', ['single-test', currentFile], {
        stdio: 'inherit',
        env: {
          ...process.env,
          JTAG_DEPLOY_BROWSER: 'true',        // Force browser deployment
          JTAG_FORCE_BROWSER_LAUNCH: 'true'   // Force fresh browser launch
        }
      });
      
      child.on('close', (code) => {
        process.exit(code || 0);
      });
      
      child.on('error', (spawnError) => {
        console.error('❌ Auto-spawn failed:', spawnError.message);
        process.exit(1);
      });
      
      // Keep process alive while child runs
      return new Promise(() => {});
      
    } else {
      // Not a transport issue - re-throw original error
      throw error;
    }
  }
}

/**
 * Convenience function for wrapping test main functions
 */
export function autoSpawnTest(testFunction: () => Promise<void>): void {
  withAutoSpawn(testFunction).catch(error => {
    console.error('💥 Test failed:', error.message);
    process.exit(1);
  });
}