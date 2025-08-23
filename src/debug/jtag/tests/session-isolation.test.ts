#!/usr/bin/env npx tsx
/**
 * SESSION MANAGEMENT PER-PROJECT ISOLATION TESTS
 * 
 * Verifies session management respects per-project .continuum isolation,
 * creates session directories in the correct context, and properly isolates
 * sessions between different project contexts.
 */

import fs from 'fs/promises';
import path from 'path';
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';
import { createTypedErrorInfo } from '../system/core/types/ErrorTypes';

interface SessionIsolationTestResult {
  readonly success: boolean;
  readonly testName: string;
  readonly details: string[];
  readonly error?: string;
}

interface SessionDirectoryStructure {
  readonly baseDir: string;
  readonly sessionDir: string;
  readonly logsDir: string;
  readonly screenshotsDir: string;
  readonly signalsDir: string;
}

async function testSessionDirectoryIsolation(): Promise<SessionIsolationTestResult> {
  const testName = "Session Directory Per-Project Isolation";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    
    // Test different project contexts
    const contexts = [
      'examples/widget-ui',
      'examples/test-bench'
    ];
    
    const sessionPaths: Record<string, SessionDirectoryStructure> = {};
    
    for (const context of contexts) {
      WorkingDirConfig.setWorkingDir(context);
      
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const sessionStructure: SessionDirectoryStructure = {
        baseDir: continuumPath,
        sessionDir: path.join(continuumPath, 'jtag', 'sessions'),
        logsDir: path.join(continuumPath, 'jtag', 'logs'),
        screenshotsDir: path.join(continuumPath, 'jtag', 'screenshots'),
        signalsDir: path.join(continuumPath, 'jtag', 'signals')
      };
      
      sessionPaths[context] = sessionStructure;
      
      details.push(`✅ Context ${context}:`);
      details.push(`   Base: ${sessionStructure.baseDir}`);
      details.push(`   Sessions: ${sessionStructure.sessionDir}`);
      details.push(`   Logs: ${sessionStructure.logsDir}`);
      details.push(`   Screenshots: ${sessionStructure.screenshotsDir}`);
      details.push(`   Signals: ${sessionStructure.signalsDir}`);
    }
    
    // Verify isolation - different contexts should have different paths
    const widgetPaths = sessionPaths['examples/widget-ui'];
    const testBenchPaths = sessionPaths['examples/test-bench'];
    
    if (widgetPaths.sessionDir === testBenchPaths.sessionDir) {
      throw new Error('Session isolation failed - same session directory for different contexts');
    }
    
    if (widgetPaths.baseDir === testBenchPaths.baseDir) {
      throw new Error('Base directory isolation failed - same base directory for different contexts');
    }
    
    details.push(`✅ Session directory isolation verified`);
    details.push(`   Widget-UI sessions: ${widgetPaths.sessionDir}`);
    details.push(`   Test-Bench sessions: ${testBenchPaths.sessionDir}`);
    
    // Restore original context
    WorkingDirConfig.setWorkingDir(originalContext);
    details.push(`✅ Context restored to: ${originalContext}`);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testSessionMetadataPersistence(): Promise<SessionIsolationTestResult> {
  const testName = "Session Metadata Per-Project Persistence";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    
    // Test session metadata isolation between contexts
    const contexts = ['examples/widget-ui', 'examples/test-bench'];
    const sessionFiles: Record<string, string> = {};
    
    for (const context of contexts) {
      WorkingDirConfig.setWorkingDir(context);
      
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const sessionMetadataFile = path.join(continuumPath, 'jtag', 'sessions', 'metadata.json');
      sessionFiles[context] = sessionMetadataFile;
      
      // Ensure session directory exists
      const sessionDir = path.dirname(sessionMetadataFile);
      await fs.mkdir(sessionDir, { recursive: true });
      
      // Create mock session metadata for testing
      const mockSessionMetadata = {
        projectContext: context,
        activeSessions: [],
        lastSessionId: null,
        created: new Date().toISOString()
      };
      
      await fs.writeFile(sessionMetadataFile, JSON.stringify(mockSessionMetadata, null, 2));
      details.push(`✅ Session metadata created for ${context}: ${sessionMetadataFile}`);
      
      // Verify file exists and can be read
      const readMetadata = JSON.parse(await fs.readFile(sessionMetadataFile, 'utf-8'));
      if (readMetadata.projectContext !== context) {
        throw new Error(`Session metadata mismatch: expected ${context}, got ${readMetadata.projectContext}`);
      }
      details.push(`   ✅ Metadata verified: ${readMetadata.projectContext}`);
    }
    
    // Verify isolation - different contexts should have different metadata files
    const widgetFile = sessionFiles['examples/widget-ui'];
    const testBenchFile = sessionFiles['examples/test-bench'];
    
    if (widgetFile === testBenchFile) {
      throw new Error('Session metadata isolation failed - same file for different contexts');
    }
    
    details.push(`✅ Session metadata isolation verified`);
    details.push(`   Different metadata files for different contexts`);
    
    // Cleanup test files
    for (const [context, file] of Object.entries(sessionFiles)) {
      try {
        await fs.unlink(file);
        details.push(`✅ Cleaned up test metadata for ${context}`);
      } catch (error) {
        // File might not exist, ignore cleanup errors
      }
    }
    
    // Restore original context
    WorkingDirConfig.setWorkingDir(originalContext);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testConcurrentSessionAccess(): Promise<SessionIsolationTestResult> {
  const testName = "Concurrent Session Access Isolation";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    
    // Simulate concurrent access to different project contexts
    const contexts = ['examples/widget-ui', 'examples/test-bench'];
    const concurrentOperations: Promise<string>[] = [];
    
    for (let i = 0; i < contexts.length; i++) {
      const context = contexts[i];
      const operation = new Promise<string>((resolve) => {
        setTimeout(() => {
          WorkingDirConfig.setWorkingDir(context);
          const continuumPath = WorkingDirConfig.getContinuumPath();
          const sessionPath = path.join(continuumPath, 'jtag', 'sessions');
          resolve(`${context}:${sessionPath}`);
        }, i * 10); // Stagger operations slightly
      });
      concurrentOperations.push(operation);
    }
    
    const results = await Promise.all(concurrentOperations);
    
    // Verify each operation got the correct context-specific path
    for (let i = 0; i < results.length; i++) {
      const [context, sessionPath] = results[i].split(':');
      const expectedContext = contexts[i];
      
      if (context !== expectedContext) {
        throw new Error(`Concurrent access context mismatch: expected ${expectedContext}, got ${context}`);
      }
      
      if (!sessionPath.includes(context.split('/')[1])) {
        throw new Error(`Session path doesn't match context: ${sessionPath} for ${context}`);
      }
      
      details.push(`✅ Concurrent operation ${i + 1}: ${context} -> ${sessionPath}`);
    }
    
    details.push(`✅ Concurrent session access isolation verified`);
    details.push(`   All operations maintained proper context-specific paths`);
    
    // Restore original context
    WorkingDirConfig.setWorkingDir(originalContext);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testSessionPathGeneration(): Promise<SessionIsolationTestResult> {
  const testName = "Session Path Generation Consistency";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    
    // Test path generation consistency across multiple calls
    const context = 'examples/widget-ui';
    WorkingDirConfig.setWorkingDir(context);
    
    const paths: string[] = [];
    for (let i = 0; i < 5; i++) {
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const sessionPath = path.join(continuumPath, 'jtag', 'sessions');
      paths.push(sessionPath);
    }
    
    // All paths should be identical
    const uniquePaths = new Set(paths);
    if (uniquePaths.size !== 1) {
      throw new Error(`Path generation inconsistency: got ${uniquePaths.size} different paths`);
    }
    
    const consistentPath = Array.from(uniquePaths)[0];
    details.push(`✅ Path generation consistency verified`);
    details.push(`   All 5 calls returned: ${consistentPath}`);
    
    // Test switching contexts and returning to original
    WorkingDirConfig.setWorkingDir('examples/test-bench');
    const testBenchPath = path.join(WorkingDirConfig.getContinuumPath(), 'jtag', 'sessions');
    
    WorkingDirConfig.setWorkingDir(context);
    const returnedPath = path.join(WorkingDirConfig.getContinuumPath(), 'jtag', 'sessions');
    
    if (returnedPath !== consistentPath) {
      throw new Error(`Context switching affected path generation: ${returnedPath} !== ${consistentPath}`);
    }
    
    details.push(`✅ Context switching path generation verified`);
    details.push(`   Path remains consistent after context switches`);
    
    // Restore original context
    WorkingDirConfig.setWorkingDir(originalContext);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function runSessionIsolationTests(): Promise<void> {
  console.log('🏷️ SESSION MANAGEMENT PER-PROJECT ISOLATION TESTS');
  console.log('═'.repeat(60));
  
  const tests = [
    testSessionDirectoryIsolation,
    testSessionMetadataPersistence,
    testConcurrentSessionAccess,
    testSessionPathGeneration
  ];
  
  const results: SessionIsolationTestResult[] = [];
  
  for (const test of tests) {
    console.log(`\n▶️ Running: ${test.name.replace(/([A-Z])/g, ' $1').trim()}...`);
    const result = await test();
    results.push(result);
    
    if (result.success) {
      console.log(`✅ PASSED: ${result.testName}`);
      for (const detail of result.details) {
        console.log(`   ${detail}`);
      }
    } else {
      console.log(`❌ FAILED: ${result.testName}`);
      for (const detail of result.details) {
        console.log(`   ${detail}`);
      }
      if (result.error) {
        console.log(`   ❌ Error: ${result.error}`);
      }
    }
  }
  
  // Summary
  const passed = results.filter(r => r.success).length;
  const total = results.length;
  
  console.log('\n' + '═'.repeat(60));
  console.log(`📊 SESSION ISOLATION TESTS SUMMARY: ${passed}/${total} PASSED`);
  
  if (passed === total) {
    console.log('🎉 ALL TESTS PASSED!');
    console.log('✅ Session management respects per-project .continuum isolation');
    console.log('✅ Session directories are properly isolated between contexts');
    console.log('✅ Concurrent session access maintains proper isolation');
    console.log('✅ Session path generation is consistent and reliable');
    process.exit(0);
  } else {
    console.log(`❌ ${total - passed} TESTS FAILED`);
    console.log('🔍 Address the issues above before deployment');
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runSessionIsolationTests().catch((error: unknown) => {
    const typedError = createTypedErrorInfo(error);
    console.error('❌ Test execution failed:', typedError.message);
    process.exit(1);
  });
}

export { 
  runSessionIsolationTests, 
  testSessionDirectoryIsolation, 
  testSessionMetadataPersistence,
  testConcurrentSessionAccess,
  testSessionPathGeneration
};