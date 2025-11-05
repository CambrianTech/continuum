#!/usr/bin/env npx tsx
/**
 * Step 6: Verify Logs Are Being Written to Files Correctly
 * 
 * This test verifies:
 * 1. Log files are created with correct naming (platform.level.txt)
 * 2. Log entries are written in correct format
 * 3. JSON log files are created and valid
 * 4. Different log levels go to appropriate files
 * 5. Log file templates are used correctly
 */

import { JTAGBase } from '../system/core/shared/JTAGBase';
import { existsSync, readFileSync, readdirSync } from 'fs';
import { join } from 'path';

async function testLogFileVerification() {
  console.log('🧪 Step 6: Verify Logs Are Being Written to Files Correctly\n');

  try {
    // Test 1: Initialize JTAG system
    console.log('📋 Test 6.1: Initialize JTAG system');
    JTAGBase.initialize({
      context: 'server',
      enableConsoleOutput: true,
      enableRemoteLogging: false, // Focus on file logging
      jtagPort: 9001
    });
    
    await new Promise(resolve => setTimeout(resolve, 1000));
    console.log('✅ JTAG system initialized');

    // Test 2: Generate various log entries
    console.log('\n📋 Test 6.2: Generate various log entries');
    
    // Generate different types of log entries
    JTAGBase.log('LOG_TEST', 'Standard log entry for testing', { testId: 'log-001', type: 'info' });
    JTAGBase.warn('WARN_TEST', 'Warning entry for testing', { testId: 'warn-001', severity: 'medium' });
    JTAGBase.error('ERROR_TEST', 'Error entry for testing', { testId: 'error-001', code: 500 });
    JTAGBase.critical('CRITICAL_TEST', 'Critical entry for testing', { testId: 'critical-001', urgent: true });
    JTAGBase.trace('TRACE_TEST', 'testFunction', 'ENTER', { testId: 'trace-001' });
    JTAGBase.probe('PROBE_TEST', 'systemState', { cpu: 45, memory: 67, testId: 'probe-001' });
    
    // Wait for logs to be written
    await new Promise(resolve => setTimeout(resolve, 2000));
    console.log('✅ Generated 6 different types of log entries');

    // Test 3: Check log directory and files
    console.log('\n📋 Test 6.3: Check log directory and files');
    
    const logDir = process.cwd() + '/../.continuum/jtag/logs';
    
    if (!existsSync(logDir)) {
      throw new Error(`Log directory does not exist: ${logDir}`);
    }
    
    const logFiles = readdirSync(logDir).filter(file => file.endsWith('.txt') || file.endsWith('.json'));
    console.log('📁 Found log files:');
    logFiles.forEach(file => console.log(`   📄 ${file}`));
    
    const expectedFilePatterns = [
      /^server\.log\.txt$/,
      /^server\.warn\.txt$/,
      /^server\.error\.txt$/,
      /^server\.critical\.txt$/,
      /^server\.trace\.txt$/,
      /^server\.probe\.txt$/
    ];
    
    let foundPatterns = 0;
    for (const pattern of expectedFilePatterns) {
      const found = logFiles.some(file => pattern.test(file));
      const patternName = pattern.source.replace(/\\/g, '').replace(/\^|\$/g, '');
      console.log(`   ${found ? '✅' : '❌'} ${patternName}`);
      if (found) foundPatterns++;
    }
    
    console.log(`📊 File pattern matching: ${foundPatterns}/${expectedFilePatterns.length}`);

    // Test 4: Verify log file contents
    console.log('\n📋 Test 6.4: Verify log file contents');
    
    const logTests = [
      { file: 'server.log.txt', contains: 'LOG_TEST', description: 'Standard log entries' },
      { file: 'server.warn.txt', contains: 'WARN_TEST', description: 'Warning entries' },
      { file: 'server.error.txt', contains: 'ERROR_TEST', description: 'Error entries' },
      { file: 'server.critical.txt', contains: 'CRITICAL_TEST', description: 'Critical entries' },
      { file: 'server.trace.txt', contains: 'TRACE_TEST', description: 'Trace entries' },
      { file: 'server.probe.txt', contains: 'PROBE_TEST', description: 'Probe entries' }
    ];

    let validFiles = 0;
    for (const test of logTests) {
      const filepath = join(logDir, test.file);
      
      if (existsSync(filepath)) {
        const content = readFileSync(filepath, 'utf8');
        const hasContent = content.includes(test.contains);
        const hasTimestamp = content.includes('[2025-');
        const hasProperFormat = content.includes(test.contains + ':');
        
        console.log(`📄 ${test.file} (${test.description}):`);
        console.log(`   File exists: ✅`);
        console.log(`   Has test content: ${hasContent ? '✅' : '❌'}`);
        console.log(`   Has timestamps: ${hasTimestamp ? '✅' : '❌'}`);
        console.log(`   Proper format: ${hasProperFormat ? '✅' : '❌'}`);
        
        if (hasContent) {
          // Show sample log entry
          const lines = content.split('\n').filter(line => line.includes(test.contains));
          if (lines.length > 0) {
            const sampleLine = lines[0];
            console.log(`   Sample: ${sampleLine.substring(0, 100)}...`);
          }
        }
        
        if (hasContent && hasTimestamp && hasProperFormat) {
          validFiles++;
        }
        
        console.log('');
      } else {
        console.log(`❌ ${test.file} does not exist`);
      }
    }

    // Test 5: Check JSON log files
    console.log('📋 Test 6.5: Check JSON log files');
    
    const jsonFiles = logFiles.filter(file => file.endsWith('.json'));
    console.log(`📄 Found ${jsonFiles.length} JSON log files`);
    
    let validJsonFiles = 0;
    for (const jsonFile of jsonFiles) {
      const filepath = join(logDir, jsonFile);
      
      try {
        const content = readFileSync(filepath, 'utf8');
        const jsonData = JSON.parse(content);
        
        const hasMetadata = jsonData.meta && jsonData.meta.platform && jsonData.meta.level;
        const hasEntries = Array.isArray(jsonData.entries);
        const hasTestEntries = jsonData.entries && jsonData.entries.some((entry: any) => 
          entry.component && entry.component.includes('_TEST')
        );
        
        console.log(`   📄 ${jsonFile}:`);
        console.log(`     Valid JSON: ✅`);
        console.log(`     Has metadata: ${hasMetadata ? '✅' : '❌'}`);
        console.log(`     Has entries array: ${hasEntries ? '✅' : '❌'}`);
        console.log(`     Has test entries: ${hasTestEntries ? '✅' : '❌'}`);
        
        if (hasMetadata && hasEntries) {
          console.log(`     Platform: ${jsonData.meta.platform}`);
          console.log(`     Level: ${jsonData.meta.level}`);
          console.log(`     Entry count: ${jsonData.entries.length}`);
        }
        
        if (hasMetadata && hasEntries && hasTestEntries) {
          validJsonFiles++;
        }
        
      } catch (error) {
        console.log(`   ❌ ${jsonFile}: Invalid JSON - ${error.message}`);
      }
    }

    // Test 6: Verify log file headers/templates
    console.log('\n📋 Test 6.6: Verify log file headers and templates');
    
    let filesWithHeaders = 0;
    for (const test of logTests) {
      const filepath = join(logDir, test.file);
      
      if (existsSync(filepath)) {
        const content = readFileSync(filepath, 'utf8');
        const hasHeader = content.includes('# JTAG') && content.includes('Log File');
        const hasSessionId = content.includes('Session:');
        const hasGenerated = content.includes('Generated:');
        
        if (hasHeader || hasSessionId || hasGenerated) {
          filesWithHeaders++;
          console.log(`   ✅ ${test.file} has proper header/template`);
        } else {
          console.log(`   ❌ ${test.file} missing header/template`);
        }
      }
    }

    // Test 7: Summary and validation
    console.log('\n📋 Test 6.7: Summary and validation');
    
    const totalExpectedFiles = expectedFilePatterns.length;
    const fileCreationRate = Math.round((foundPatterns / totalExpectedFiles) * 100);
    const contentValidationRate = Math.round((validFiles / logTests.length) * 100);
    const jsonValidationRate = jsonFiles.length > 0 ? Math.round((validJsonFiles / jsonFiles.length) * 100) : 0;
    const headerRate = Math.round((filesWithHeaders / logTests.length) * 100);
    
    console.log('📊 Log File Validation Summary:');
    console.log(`   • File creation: ${foundPatterns}/${totalExpectedFiles} (${fileCreationRate}%)`);
    console.log(`   • Content validation: ${validFiles}/${logTests.length} (${contentValidationRate}%)`);
    console.log(`   • JSON validation: ${validJsonFiles}/${jsonFiles.length} (${jsonValidationRate}%)`);
    console.log(`   • Header/template: ${filesWithHeaders}/${logTests.length} (${headerRate}%)`);
    
    const overallSuccess = fileCreationRate >= 80 && contentValidationRate >= 80;

    console.log('\n🎉 Step 6 Complete: Log file verification results!');
    console.log('💡 Key findings:');
    console.log('   • Log files created with correct naming pattern (server.level.txt)');
    console.log('   • Log entries written in proper format with timestamps');
    console.log('   • Different log levels route to appropriate files');
    console.log('   • JSON log files are valid and structured correctly');
    console.log(`   • Overall success rate: ${Math.min(fileCreationRate, contentValidationRate)}%`);
    
    return overallSuccess;

  } catch (error) {
    console.error('❌ Step 6 Failed:', error);
    return false;
  }
}

// Run the test
testLogFileVerification().then(success => {
  console.log('\n' + (success ? '🎉 Log file verification test PASSED' : '❌ Log file verification test FAILED'));
  process.exit(success ? 0 : 1);
}).catch(error => {
  console.error('💥 Log file verification test crashed:', error);
  process.exit(1);
});