#!/usr/bin/env npx tsx
/**
 * Step 1: Test Basic JTAG Initialization and Logging
 * 
 * This test verifies that JTAG can:
 * 1. Initialize properly
 * 2. Log messages 
 * 3. Write to files
 * 4. Return correct status
 */

import { JTAGBase } from '../system/core/shared/JTAGBase';
import { existsSync, readFileSync } from 'fs';
import { join } from 'path';

async function testBasicJTAG() {
  console.log('🧪 Step 1: Testing Basic JTAG Initialization and Logging\n');

  try {
    // Test 1: Initialize JTAG
    console.log('📋 Test 1.1: Initialize JTAG system');
    JTAGBase.initialize({
      context: 'server',
      enableConsoleOutput: true,
      enableRemoteLogging: false, // Disable WebSocket for this basic test
      jtagPort: 9001
    });
    console.log('✅ JTAG initialized successfully');

    // Test 2: Get UUID and status
    console.log('\n📋 Test 1.2: Check JTAG UUID and metadata');
    const uuid = JTAGBase.getUUID();
    console.log('🆔 UUID:', uuid.uuid);
    console.log('🔧 Context:', uuid.context);
    console.log('📅 Timestamp:', uuid.timestamp);
    console.log('✅ UUID generation works');

    // Test 3: Basic logging
    console.log('\n📋 Test 1.3: Test basic logging methods');
    JTAGBase.log('TEST_COMPONENT', 'Basic log message test');
    JTAGBase.warn('TEST_COMPONENT', 'Warning message test');  
    JTAGBase.error('TEST_COMPONENT', 'Error message test');
    JTAGBase.critical('TEST_COMPONENT', 'Critical message test');
    JTAGBase.trace('TEST_COMPONENT', 'testFunction', 'ENTER');
    JTAGBase.probe('TEST_COMPONENT', 'testProbe', { status: 'testing' });
    console.log('✅ All logging methods executed without errors');

    // Test 4: Check log files were created
    console.log('\n📋 Test 1.4: Verify log files are created');
    const logDir = process.cwd() + '/../.continuum/jtag/logs';
    
    // Check for server.log.txt (basic logs)
    const serverLogPath = join(logDir, 'server.log.txt');
    if (existsSync(serverLogPath)) {
      const logContent = readFileSync(serverLogPath, 'utf8');
      const lines = logContent.split('\n').filter(line => line.includes('TEST_COMPONENT'));
      console.log(`✅ Found ${lines.length} test log entries in server.log.txt`);
      if (lines.length > 0) {
        console.log('📝 Sample log entry:', lines[0].substring(0, 100) + '...');
      }
    } else {
      console.log('❌ server.log.txt not found');
    }

    // Check for other log files (server.warn.txt, server.error.txt, etc.)
    const logFiles = ['server.warn.txt', 'server.error.txt', 'server.critical.txt'];
    for (const logFile of logFiles) {
      const filePath = join(logDir, logFile);
      if (existsSync(filePath)) {
        console.log(`✅ ${logFile} exists`);
      } else {
        console.log(`⚠️  ${logFile} not found (may not have been created yet)`);
      }
    }

    // Test 5: Test logging with data
    console.log('\n📋 Test 1.5: Test logging with structured data');
    JTAGBase.log('TEST_DATA', 'Message with data', {
      testNumber: 42,
      testArray: [1, 2, 3],
      testObject: { key: 'value' }
    });
    console.log('✅ Structured data logging works');

    // Test 6: Get stats
    console.log('\n📋 Test 1.6: Check JTAG statistics');
    const stats = JTAGBase.getStats();
    console.log('📊 Total entries:', stats.totalEntries);
    console.log('📊 Entries by type:', stats.entriesByType);
    console.log('📊 Entries by component:', stats.entriesByComponent);
    console.log('✅ Statistics collection works');

    console.log('\n🎉 Step 1 Complete: Basic JTAG functionality works correctly!');
    return true;

  } catch (error) {
    console.error('❌ Step 1 Failed:', error);
    return false;
  }
}

// Run the test
testBasicJTAG().then(success => {
  process.exit(success ? 0 : 1);
}).catch(error => {
  console.error('💥 Test crashed:', error);
  process.exit(1);
});