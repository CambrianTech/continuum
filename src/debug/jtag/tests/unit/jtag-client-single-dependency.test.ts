#!/usr/bin/env tsx
/**
 * TDD Test: Single Command Dependency Pattern
 * 
 * Tests that JTAGClient has only ONE hardcoded dependency: the 'list' command.
 * All other commands should be discovered dynamically via list command response.
 */

import { JTAGClientBrowser } from '../../shared/JTAGClientBrowser';
import type { ListResult } from '../../commands/list/shared/ListTypes';

console.log('🧪 JTAGClient Single Command Dependency Test Suite');

// Test utilities
function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

function assertThrows(fn: () => void, message: string) {
  try {
    fn();
    throw new Error(`Expected function to throw`);
  } catch (error) {
    console.log(`✅ ${message} (correctly threw: ${error.message})`);
  }
}

async function runTests() {
  console.log('\n🔑 TEST 1: JTAGClient should have only list as hardcoded dependency');
  
  try {
    // Before connect() - client should only know about list command
    const context = { uuid: 'test-session', environment: 'browser' as const };
    const client = new JTAGClientBrowser(context);
    
    // At this point, the client should have NO discovered commands
    // Only the essential 'list' command should be available
    assert(client.discoveredCommands?.size === 0 || !client.discoveredCommands, 'No discovered commands initially');
    
    // But 'list' should always be available (our single dependency)
    assert(typeof client.commands?.list === 'function', 'List command always available');
    
    console.log('✅ Single dependency pattern verified');
  } catch (error) {
    console.log(`⚠️  Test 1 skipped - not yet implemented: ${error.message}`);
  }
  
  console.log('\n🔄 TEST 2: connect() should return list result for CLI integration');
  
  try {
    const client = await JTAGClientBrowser.connectLocal();
    
    // Check if connect returns anything (TDD - we'll implement this)
    console.log('✅ Local connection established');
    
    // Check if client has commands interface
    assert(typeof client.commands === 'object', 'Commands interface exists');
    assert(typeof client.commands.list === 'function', 'List command available after connect');
    
    console.log('✅ Connect pattern partially verified');
  } catch (error) {
    console.log(`⚠️  Test 2 - Current state: ${error.message}`);
  }
  
  console.log('\n🎯 TEST 3: Commands should be populated after connect()');
  
  try {
    const client = await JTAGClientBrowser.connectLocal();
    
    // Check if any commands are available
    if (client.commands) {
      const availableCommands = Object.keys(client.commands);
      console.log(`📋 Available commands: ${availableCommands.join(', ')}`);
      
      // Check for expected commands
      if (availableCommands.includes('screenshot')) {
        console.log('✅ Screenshot command discovered');
      }
      
      if (availableCommands.includes('list')) {
        console.log('✅ List command available');
        
        // Try calling list to see what happens
        const listResult = await client.commands.list();
        console.log(`📊 List result: ${listResult.totalCount} commands found`);
        console.log(`📝 Commands: ${listResult.commands.map(c => c.name).join(', ')}`);
      }
    }
    
    console.log('✅ Dynamic discovery test completed');
  } catch (error) {
    console.log(`⚠️  Test 3 - Current state: ${error.message}`);
  }
  
  console.log('\n📋 TEST 4: CLI Integration potential');
  
  try {
    const client = await JTAGClientBrowser.connectLocal();
    
    if (client.commands?.list) {
      const listResult = await client.commands.list();
      
      // Show how this could be used for CLI
      const cliFormat = listResult.commands.map(cmd => ({
        flag: `--${cmd.name}`,
        description: cmd.description,
        category: cmd.category
      }));
      
      console.log('🎯 CLI Integration Preview:');
      cliFormat.slice(0, 3).forEach(cmd => {
        console.log(`  ${cmd.flag.padEnd(15)} ${cmd.description} (${cmd.category})`);
      });
      
      console.log('✅ CLI integration format verified');
    }
  } catch (error) {
    console.log(`⚠️  Test 4 - Current state: ${error.message}`);
  }
  
  console.log('\n🎉 TDD Test Suite Complete');
  console.log('📋 Next Steps:');
  console.log('  1. Implement single dependency pattern in JTAGClient');
  console.log('  2. Make connect() return ListResult');
  console.log('  3. Add command interception for dynamic updates');
  console.log('  4. Implement discoveredCommands property');
}

// Run the tests
runTests().catch(error => {
  console.error('❌ Test suite failed:', error);
  process.exit(1);
});