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
import { BuildVersionDetector } from './BuildVersionDetector';
import { diagnostics } from './DiagnosticsLogger';

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
 * Auto-spawn wrapper for test functions with intelligent build detection
 * 
 * Now includes:
 * 1. Build version mismatch detection (source vs running system)
 * 2. Transport timeout detection (browser commands failing) 
 * 3. Automatic rebuild + redeploy when needed
 * 
 * Result: Tests become completely autonomous for AI development
 */
export async function withAutoSpawn<T>(testFunction: () => Promise<T>): Promise<T> {
  const operationId = `auto-spawn-${Date.now()}`;
  const context = diagnostics.startOperation(operationId, 'Auto-Spawn Test Execution', 300000); // 5 minute timeout
  
  try {
    // STEP 1: Check if rebuild is needed before running test
    diagnostics.addDetail(operationId, 'phase', 'build_detection');
    const buildDetector = new BuildVersionDetector();
    
    let buildCheck: { rebuild: boolean; reason: string };
    
    try {
      buildCheck = await Promise.race([
        buildDetector.shouldRebuildForTesting(),
        new Promise<{ rebuild: boolean; reason: string }>((_, reject) => {
          setTimeout(() => reject(new Error('Build detection timeout after 30 seconds')), 30000);
        })
      ]);
    } catch (buildError) {
      const errorMessage = buildError instanceof Error ? buildError.message : String(buildError);
      diagnostics.addWarning(operationId, `Build detection failed: ${errorMessage}, assuming rebuild needed`);
      buildCheck = { rebuild: true, reason: `Build detection failed: ${errorMessage}` };
    }
    
    diagnostics.addDetail(operationId, 'buildCheck', buildCheck);
    
    if (buildCheck.rebuild) {
      console.log('');
      console.log('🔄 BUILD VERSION MISMATCH DETECTED - Auto-rebuilding...');
      console.log(`📋 Reason: ${buildCheck.reason}`);
      console.log('🚀 Running smart build + deployment with fresh browser...');
      console.log('');
      
      diagnostics.addDetail(operationId, 'phase', 'auto_rebuild_spawn');
      const currentFile = getCurrentTestFile();
      diagnostics.addDetail(operationId, 'testFile', currentFile);
      
      console.log(`🔧 EMERGENCY SPAWN: ${currentFile}`);
      console.log('📋 Environment: JTAG_FORCE_BUILD=true, JTAG_DEPLOY_BROWSER=true');
      
      // Spawn with build + deployment with enhanced error handling
      const child = spawn('./scripts/run-categorized-tests.sh', ['single-test', currentFile], {
        stdio: 'inherit',
        env: {
          ...process.env,
          JTAG_DEPLOY_BROWSER: 'true',        // Force browser deployment
          JTAG_FORCE_BROWSER_LAUNCH: 'true',  // Force fresh browser
          JTAG_FORCE_BUILD: 'true'            // Force rebuild first
        }
      });
      
      // Set up spawn timeout protection
      const spawnTimeout = setTimeout(() => {
        diagnostics.addError(operationId, 'Auto-rebuild spawn timeout after 4 minutes');
        console.error('⏰ AUTO-REBUILD TIMEOUT - killing spawned process');
        child.kill('SIGTERM');
        setTimeout(() => child.kill('SIGKILL'), 5000);
        diagnostics.failOperation(operationId, 'Spawn timeout');
        process.exit(1);
      }, 240000); // 4 minute timeout
      
      child.on('close', (code) => {
        clearTimeout(spawnTimeout);
        diagnostics.addDetail(operationId, 'childExitCode', code);
        
        if (code === 0) {
          diagnostics.completeOperation(operationId);
        } else {
          diagnostics.failOperation(operationId, `Child process failed with exit code ${code}`);
        }
        
        process.exit(code || 0);
      });
      
      child.on('error', (spawnError) => {
        clearTimeout(spawnTimeout);
        const errorMessage = spawnError.message;
        diagnostics.addError(operationId, `Auto-rebuild spawn error: ${errorMessage}`);
        console.error('❌ Auto-rebuild spawn failed:', errorMessage);
        console.error('🔍 Check if ./scripts/run-categorized-tests.sh exists and is executable');
        diagnostics.failOperation(operationId, errorMessage);
        process.exit(1);
      });
      
      // Keep process alive while child runs
      return new Promise(() => {});
    }
  } catch (buildError) {
    const errorMessage = buildError instanceof Error ? buildError.message : String(buildError);
    diagnostics.addWarning(operationId, `Build detection failed: ${errorMessage}`);
    console.warn('⚠️ Build version detection failed, proceeding with test:', errorMessage);
  }
  
  // STEP 2: Try to run the test normally with timeout protection
  diagnostics.addDetail(operationId, 'phase', 'test_execution');
  try {
    const testResult = await Promise.race([
      testFunction(),
      new Promise<T>((_, reject) => {
        setTimeout(() => reject(new Error('Test function timeout after 2 minutes')), 120000);
      })
    ]);
    
    diagnostics.completeOperation(operationId);
    return testResult;
    
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    diagnostics.addDetail(operationId, 'testError', errorMessage);
    
    if (isTransportTimeout(error)) {
      diagnostics.addDetail(operationId, 'phase', 'transport_recovery');
      diagnostics.addError(operationId, `Transport timeout: ${errorMessage}`);
      
      const currentFile = getCurrentTestFile();
      diagnostics.addDetail(operationId, 'recoveryFile', currentFile);
      
      console.log('');
      console.log('🔧 TRANSPORT TIMEOUT DETECTED - Auto-spawning with browser deployment...');
      console.log(`📋 Original error: ${errorMessage}`);
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
      
      // Set up transport recovery timeout
      const recoveryTimeout = setTimeout(() => {
        diagnostics.addError(operationId, 'Transport recovery timeout after 3 minutes');
        console.error('⏰ TRANSPORT RECOVERY TIMEOUT - killing spawned process');
        child.kill('SIGTERM');
        setTimeout(() => child.kill('SIGKILL'), 5000);
        diagnostics.failOperation(operationId, 'Recovery timeout');
        process.exit(1);
      }, 180000); // 3 minute timeout
      
      child.on('close', (code) => {
        clearTimeout(recoveryTimeout);
        diagnostics.addDetail(operationId, 'recoveryExitCode', code);
        
        if (code === 0) {
          diagnostics.completeOperation(operationId);
        } else {
          diagnostics.failOperation(operationId, `Recovery process failed with exit code ${code}`);
        }
        
        process.exit(code || 0);
      });
      
      child.on('error', (spawnError) => {
        clearTimeout(recoveryTimeout);
        const spawnErrorMessage = spawnError.message;
        diagnostics.addError(operationId, `Transport recovery spawn error: ${spawnErrorMessage}`);
        console.error('❌ Auto-spawn failed:', spawnErrorMessage);
        console.error('🔍 Check if ./scripts/run-categorized-tests.sh exists and is executable');
        diagnostics.failOperation(operationId, spawnErrorMessage);
        process.exit(1);
      });
      
      // Keep process alive while child runs
      return new Promise(() => {});
      
    } else {
      // Not a transport issue - re-throw original error with diagnostics
      diagnostics.addError(operationId, `Test failed with non-transport error: ${errorMessage}`);
      diagnostics.failOperation(operationId, errorMessage);
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