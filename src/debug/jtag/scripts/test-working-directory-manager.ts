#!/usr/bin/env tsx
/**
 * Test Working Directory Manager - Demonstrate centralized path management
 */

import { WorkingDirectoryManager } from '../shared/managers/WorkingDirectoryManager';
import { v4 as uuidv4 } from 'uuid';

async function testWorkingDirectoryManager(): Promise<void> {
  console.log('📁 WORKING DIRECTORY MANAGER TEST');
  console.log('==================================');
  console.log('Testing centralized path management vs scattered directory creation');
  console.log();

  try {
    // Test 1: Auto-detection from environment
    console.log('🔍 Test 1: Auto-detection from environment');
    const autoManager = await WorkingDirectoryManager.createFromEnvironment();
    const autoStructure = await autoManager.getDirectoryStructure();
    
    console.log(`   Root detected: ${autoStructure.root}`);
    console.log(`   JTAG path: ${autoStructure.jtag}`);
    console.log(`   Current user: ${autoStructure.currentUser}`);
    console.log();

    // Test 2: Explicit example configuration
    console.log('🎯 Test 2: Explicit example configuration');
    const exampleManager = WorkingDirectoryManager.createForProject(
      '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag',
      'widget-ui'
    );
    const exampleStructure = await exampleManager.getDirectoryStructure();
    
    console.log(`   Root: ${exampleStructure.root}`);
    console.log(`   Logs: ${await exampleManager.getLogsPath()}`);
    console.log(`   Screenshots: ${await exampleManager.getScreenshotsPath()}`);
    console.log();

    // Test 3: Session-aware configuration
    console.log('🎪 Test 3: Session-aware configuration');
    const sessionId = uuidv4();
    const sessionManager = WorkingDirectoryManager.createForSession(
      '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag',
      sessionId,
      'test-bench'
    );
    
    console.log(`   Session ID: ${sessionId}`);
    console.log(`   Performance path: ${await sessionManager.getPerformancePath()}`);
    console.log(`   Data path: ${await sessionManager.getDataPath()}`);
    console.log();

    // Test 4: Detect existing sessions
    console.log('🕵️ Test 4: Detect existing sessions');
    const sessions = await autoManager.detectActiveSessions();
    console.log(`   Found ${sessions.length} existing sessions:`);
    sessions.forEach((session, i) => {
      console.log(`     ${i + 1}. ${session.sessionId} at ${session.path}`);
    });
    console.log();

    // Test 5: Current session detection
    console.log('🔗 Test 5: Current session detection');
    const currentSession = await autoManager.getCurrentSession();
    if (currentSession) {
      console.log(`   Current session: ${currentSession.sessionId}`);
      console.log(`   Path: ${currentSession.path}`);
    } else {
      console.log('   No current session symlink found');
    }
    console.log();

    console.log('✅ WORKING DIRECTORY MANAGER BENEFITS:');
    console.log('=====================================');
    console.log('🎯 Single source of truth for all paths');
    console.log('🔧 Automatic environment detection');  
    console.log('📁 Consistent directory structure');
    console.log('🔗 Proper symlink management');
    console.log('🕵️ Session discovery and management');
    console.log('⚡ Eliminates scattered path creation');
    console.log();
    console.log('🚀 NEXT STEP: Replace scattered path logic throughout codebase');

  } catch (error: any) {
    console.error('❌ Working directory manager test failed:', error.message);
    console.error(error.stack);
  }
}

// Run if executed directly
if (require.main === module) {
  testWorkingDirectoryManager().catch(error => {
    console.error('❌ Test crashed:', error);
    process.exit(1);
  });
}

export { testWorkingDirectoryManager };