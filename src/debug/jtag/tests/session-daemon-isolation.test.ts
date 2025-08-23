#!/usr/bin/env npx tsx
/**
 * SESSION DAEMON PER-PROJECT ISOLATION TESTS
 * 
 * Verifies SessionDaemonServer properly respects per-project .continuum isolation,
 * persists session data to the correct context-specific directories, and maintains
 * session isolation between different project contexts.
 */

import fs from 'fs/promises';
import path from 'path';
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';
import { createTypedErrorInfo } from '../system/core/types/ErrorTypes';
import { SessionDaemonServer } from '../daemons/session-daemon/server/SessionDaemonServer';
import type { JTAGContext } from '../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../system/core/router/shared/JTAGRouter';

interface SessionDaemonIsolationTestResult {
  readonly success: boolean;
  readonly testName: string;
  readonly details: string[];
  readonly error?: string;
}

// Mock router for testing
const mockRouter: JTAGRouter = {
  getSubscriber: () => null,
  postMessage: async () => ({ success: false, error: 'Mock router' } as any),
  subscribe: () => {},
  unsubscribe: () => {},
  toString: () => 'MockRouter'
} as any;

// Mock context for testing
const createMockContext = (environment: 'browser' | 'server'): JTAGContext => ({
  environment,
  origin: 'test',
  sessionId: 'test-session-id',
  uuid: `test-uuid-${environment}-${Date.now()}`
});

async function testSessionDaemonContextSwitching(): Promise<SessionDaemonIsolationTestResult> {
  const testName = "SessionDaemon Context-Aware Persistence";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    
    // Test with widget-ui context
    WorkingDirConfig.setWorkingDir('examples/widget-ui');
    const widgetContext = createMockContext('server');
    const widgetSessionDaemon = new SessionDaemonServer(widgetContext, mockRouter);
    
    // Initialize daemon (loads existing sessions if any)
    await (widgetSessionDaemon as any).initialize();
    
    // Get the session metadata path for widget-ui
    const widgetMetadataPath = (widgetSessionDaemon as any).getSessionsMetadataPath();
    details.push(`✅ Widget-UI session daemon initialized`);
    details.push(`   Metadata path: ${widgetMetadataPath}`);
    
    // Test with test-bench context
    WorkingDirConfig.setWorkingDir('examples/test-bench');
    const testBenchContext = createMockContext('server');
    const testBenchSessionDaemon = new SessionDaemonServer(testBenchContext, mockRouter);
    
    // Initialize daemon (loads existing sessions if any)
    await (testBenchSessionDaemon as any).initialize();
    
    // Get the session metadata path for test-bench
    const testBenchMetadataPath = (testBenchSessionDaemon as any).getSessionsMetadataPath();
    details.push(`✅ Test-Bench session daemon initialized`);
    details.push(`   Metadata path: ${testBenchMetadataPath}`);
    
    // Verify different contexts have different metadata paths
    if (widgetMetadataPath === testBenchMetadataPath) {
      throw new Error('Session daemon isolation failed - same metadata path for different contexts');
    }
    
    details.push(`✅ Session daemon context isolation verified`);
    details.push(`   Different metadata paths for different contexts`);
    
    // Verify paths contain correct context identifiers
    if (!widgetMetadataPath.includes('widget-ui')) {
      throw new Error(`Widget-UI metadata path doesn't contain context: ${widgetMetadataPath}`);
    }
    
    if (!testBenchMetadataPath.includes('test-bench')) {
      throw new Error(`Test-Bench metadata path doesn't contain context: ${testBenchMetadataPath}`);
    }
    
    details.push(`✅ Context-specific path validation passed`);
    
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

async function testSessionPersistenceIsolation(): Promise<SessionDaemonIsolationTestResult> {
  const testName = "Session Persistence Per-Project Isolation";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    
    // Create mock session data for widget-ui
    WorkingDirConfig.setWorkingDir('examples/widget-ui');
    const widgetContext = createMockContext('server');
    const widgetSessionDaemon = new SessionDaemonServer(widgetContext, mockRouter);
    await (widgetSessionDaemon as any).initialize();
    
    // Create test session data
    const mockWidgetSession = {
      sourceContext: widgetContext,
      sessionId: 'widget-test-session-123',
      category: 'test',
      displayName: 'Widget UI Test Session',
      userId: 'test-user-widget',
      created: new Date(),
      lastActive: new Date(),
      isActive: true,
      isShared: false
    };
    
    // Add session and save
    (widgetSessionDaemon as any).sessions = [mockWidgetSession];
    await (widgetSessionDaemon as any).saveSessionsToFile();
    
    const widgetMetadataPath = (widgetSessionDaemon as any).getSessionsMetadataPath();
    details.push(`✅ Widget-UI session saved to: ${widgetMetadataPath}`);
    
    // Create mock session data for test-bench
    WorkingDirConfig.setWorkingDir('examples/test-bench');
    const testBenchContext = createMockContext('server');
    const testBenchSessionDaemon = new SessionDaemonServer(testBenchContext, mockRouter);
    await (testBenchSessionDaemon as any).initialize();
    
    const mockTestBenchSession = {
      sourceContext: testBenchContext,
      sessionId: 'test-bench-session-456',
      category: 'test',
      displayName: 'Test Bench Test Session',
      userId: 'test-user-testbench',
      created: new Date(),
      lastActive: new Date(),
      isActive: true,
      isShared: false
    };
    
    // Add session and save
    (testBenchSessionDaemon as any).sessions = [mockTestBenchSession];
    await (testBenchSessionDaemon as any).saveSessionsToFile();
    
    const testBenchMetadataPath = (testBenchSessionDaemon as any).getSessionsMetadataPath();
    details.push(`✅ Test-Bench session saved to: ${testBenchMetadataPath}`);
    
    // Verify files exist and contain correct data
    const widgetMetadata = JSON.parse(await fs.readFile(widgetMetadataPath, 'utf-8'));
    const testBenchMetadata = JSON.parse(await fs.readFile(testBenchMetadataPath, 'utf-8'));
    
    if (widgetMetadata.sessions.length !== 1 || widgetMetadata.sessions[0].sessionId !== 'widget-test-session-123') {
      throw new Error(`Widget-UI metadata incorrect: ${JSON.stringify(widgetMetadata.sessions)}`);
    }
    
    if (testBenchMetadata.sessions.length !== 1 || testBenchMetadata.sessions[0].sessionId !== 'test-bench-session-456') {
      throw new Error(`Test-Bench metadata incorrect: ${JSON.stringify(testBenchMetadata.sessions)}`);
    }
    
    details.push(`✅ Session persistence verification passed`);
    details.push(`   Widget-UI session: ${widgetMetadata.sessions[0].displayName}`);
    details.push(`   Test-Bench session: ${testBenchMetadata.sessions[0].displayName}`);
    
    // Verify project context is recorded correctly (could be relative or absolute path)
    if (!widgetMetadata.projectContext.includes('widget-ui')) {
      throw new Error(`Widget-UI project context incorrect: ${widgetMetadata.projectContext}`);
    }
    
    if (!testBenchMetadata.projectContext.includes('test-bench')) {
      throw new Error(`Test-Bench project context incorrect: ${testBenchMetadata.projectContext}`);
    }
    
    details.push(`✅ Project context recording verified`);
    
    // Cleanup test files
    try {
      await fs.unlink(widgetMetadataPath);
      await fs.unlink(testBenchMetadataPath);
      details.push(`✅ Test files cleaned up successfully`);
    } catch (error) {
      // Ignore cleanup errors
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

async function testSessionLoadingIsolation(): Promise<SessionDaemonIsolationTestResult> {
  const testName = "Session Loading Per-Project Isolation";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    
    // Create session files for both contexts
    const contexts = ['examples/widget-ui', 'examples/test-bench'];
    const sessionData = {
      'examples/widget-ui': {
        projectContext: 'examples/widget-ui',
        sessions: [{
          sessionId: 'widget-loaded-session',
          displayName: 'Widget Loaded Session',
          category: 'loaded',
          userId: 'loaded-user',
          isActive: true,
          created: new Date().toISOString(),
          lastActive: new Date().toISOString()
        }],
        lastUpdated: new Date().toISOString(),
        version: '1.0.0'
      },
      'examples/test-bench': {
        projectContext: 'examples/test-bench',
        sessions: [{
          sessionId: 'testbench-loaded-session',
          displayName: 'TestBench Loaded Session',
          category: 'loaded',
          userId: 'loaded-user',
          isActive: true,
          created: new Date().toISOString(),
          lastActive: new Date().toISOString()
        }],
        lastUpdated: new Date().toISOString(),
        version: '1.0.0'
      }
    };
    
    // Create metadata files
    const metadataFiles: string[] = [];
    for (const context of contexts) {
      WorkingDirConfig.setWorkingDir(context);
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const sessionDir = path.join(continuumPath, 'jtag', 'sessions');
      const metadataPath = path.join(sessionDir, 'metadata.json');
      
      await fs.mkdir(sessionDir, { recursive: true });
      await fs.writeFile(metadataPath, JSON.stringify(sessionData[context], null, 2));
      metadataFiles.push(metadataPath);
      
      details.push(`✅ Created test metadata for ${context}: ${metadataPath}`);
    }
    
    // Test loading sessions from widget-ui context
    WorkingDirConfig.setWorkingDir('examples/widget-ui');
    const widgetContext = createMockContext('server');
    const widgetSessionDaemon = new SessionDaemonServer(widgetContext, mockRouter);
    await (widgetSessionDaemon as any).initialize();
    
    const widgetSessions = (widgetSessionDaemon as any).sessions;
    if (widgetSessions.length !== 1 || widgetSessions[0].sessionId !== 'widget-loaded-session') {
      throw new Error(`Widget-UI session loading failed: ${JSON.stringify(widgetSessions)}`);
    }
    
    details.push(`✅ Widget-UI session loading verified: ${widgetSessions[0].displayName}`);
    
    // Test loading sessions from test-bench context
    WorkingDirConfig.setWorkingDir('examples/test-bench');
    const testBenchContext = createMockContext('server');
    const testBenchSessionDaemon = new SessionDaemonServer(testBenchContext, mockRouter);
    await (testBenchSessionDaemon as any).initialize();
    
    const testBenchSessions = (testBenchSessionDaemon as any).sessions;
    if (testBenchSessions.length !== 1 || testBenchSessions[0].sessionId !== 'testbench-loaded-session') {
      throw new Error(`Test-Bench session loading failed: ${JSON.stringify(testBenchSessions)}`);
    }
    
    details.push(`✅ Test-Bench session loading verified: ${testBenchSessions[0].displayName}`);
    
    // Verify isolation - each daemon only loaded its context-specific sessions
    if (widgetSessions.some((s: any) => s.sessionId === 'testbench-loaded-session')) {
      throw new Error('Session loading isolation failed - widget daemon loaded test-bench sessions');
    }
    
    if (testBenchSessions.some((s: any) => s.sessionId === 'widget-loaded-session')) {
      throw new Error('Session loading isolation failed - test-bench daemon loaded widget sessions');
    }
    
    details.push(`✅ Session loading isolation verified`);
    details.push(`   Each daemon only loaded its context-specific sessions`);
    
    // Cleanup test files
    for (const metadataFile of metadataFiles) {
      try {
        await fs.unlink(metadataFile);
      } catch (error) {
        // Ignore cleanup errors
      }
    }
    details.push(`✅ Test files cleaned up`);
    
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

async function testSessionDirectoryCreation(): Promise<SessionDaemonIsolationTestResult> {
  const testName = "Session Directory Creation Per-Project";
  const details: string[] = [];
  
  try {
    const originalContext = WorkingDirConfig.getWorkingDir();
    
    // Test directory creation for widget-ui
    WorkingDirConfig.setWorkingDir('examples/widget-ui');
    const widgetContext = createMockContext('server');
    const widgetSessionDaemon = new SessionDaemonServer(widgetContext, mockRouter);
    
    // Trigger directory creation
    await (widgetSessionDaemon as any).ensureSessionDirectories();
    
    const widgetContinuumPath = WorkingDirConfig.getContinuumPath();
    const expectedDirs = [
      path.join(widgetContinuumPath, 'jtag', 'sessions'),
      path.join(widgetContinuumPath, 'jtag', 'logs'),
      path.join(widgetContinuumPath, 'jtag', 'screenshots'),
      path.join(widgetContinuumPath, 'jtag', 'signals')
    ];
    
    for (const dir of expectedDirs) {
      const stats = await fs.stat(dir);
      if (!stats.isDirectory()) {
        throw new Error(`Expected directory not created: ${dir}`);
      }
      details.push(`✅ Widget-UI directory created: ${dir}`);
    }
    
    // Test directory creation for test-bench
    WorkingDirConfig.setWorkingDir('examples/test-bench');
    const testBenchContext = createMockContext('server');
    const testBenchSessionDaemon = new SessionDaemonServer(testBenchContext, mockRouter);
    
    await (testBenchSessionDaemon as any).ensureSessionDirectories();
    
    const testBenchContinuumPath = WorkingDirConfig.getContinuumPath();
    const testBenchExpectedDirs = [
      path.join(testBenchContinuumPath, 'jtag', 'sessions'),
      path.join(testBenchContinuumPath, 'jtag', 'logs'),
      path.join(testBenchContinuumPath, 'jtag', 'screenshots'),
      path.join(testBenchContinuumPath, 'jtag', 'signals')
    ];
    
    for (const dir of testBenchExpectedDirs) {
      const stats = await fs.stat(dir);
      if (!stats.isDirectory()) {
        throw new Error(`Expected directory not created: ${dir}`);
      }
      details.push(`✅ Test-Bench directory created: ${dir}`);
    }
    
    // Verify isolation - different base paths
    const widgetSessionDir = expectedDirs[0];
    const testBenchSessionDir = testBenchExpectedDirs[0];
    
    if (widgetSessionDir === testBenchSessionDir) {
      throw new Error('Directory creation isolation failed - same paths for different contexts');
    }
    
    details.push(`✅ Directory creation isolation verified`);
    details.push(`   Widget-UI sessions: ${widgetSessionDir}`);
    details.push(`   Test-Bench sessions: ${testBenchSessionDir}`);
    
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

async function runSessionDaemonIsolationTests(): Promise<void> {
  console.log('🏷️ SESSION DAEMON PER-PROJECT ISOLATION TESTS');
  console.log('═'.repeat(60));
  
  const tests = [
    testSessionDaemonContextSwitching,
    testSessionPersistenceIsolation,
    testSessionLoadingIsolation,
    testSessionDirectoryCreation
  ];
  
  const results: SessionDaemonIsolationTestResult[] = [];
  
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
  console.log(`📊 SESSION DAEMON ISOLATION TESTS SUMMARY: ${passed}/${total} PASSED`);
  
  if (passed === total) {
    console.log('🎉 ALL TESTS PASSED!');
    console.log('✅ SessionDaemonServer respects per-project .continuum isolation');
    console.log('✅ Session persistence uses context-specific directories');
    console.log('✅ Session loading maintains proper isolation between contexts');
    console.log('✅ Session directories are created in correct per-project locations');
    process.exit(0);
  } else {
    console.log(`❌ ${total - passed} TESTS FAILED`);
    console.log('🔍 Address the issues above before deployment');
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runSessionDaemonIsolationTests().catch((error: unknown) => {
    const typedError = createTypedErrorInfo(error);
    console.error('❌ Test execution failed:', typedError.message);
    process.exit(1);
  });
}

export { 
  runSessionDaemonIsolationTests, 
  testSessionDaemonContextSwitching, 
  testSessionPersistenceIsolation,
  testSessionLoadingIsolation,
  testSessionDirectoryCreation
};