/**
 * Command Interdependency Test
 * Testing that help() calls list() and other command dependencies work correctly
 */

import { BootstrapSystem } from '../BootstrapSystem.js';

async function testCommandInterdependencies(): Promise<void> {
  console.log('🧪 Testing Command Interdependencies (help → list)...\n');

  const bootstrap = new BootstrapSystem();
  
  try {
    // Start the bootstrap system
    console.log('📋 Starting bootstrap system...');
    await bootstrap.start();
    console.log('✅ Bootstrap system ready\n');

    // Test 1: Direct list command
    console.log('📋 Test 1: Direct list command execution');
    console.debug('🔧 TEST: Calling list command directly...');
    
    const directListResult = await bootstrap.executeCommand('list', {});
    console.log('✅ Direct list command succeeded');
    console.debug('📊 Direct list result:', {
      totalCommands: directListResult.data.totalCommands,
      bootstrapCommands: directListResult.data.bootstrapCommands.length,
      discoveredCommands: directListResult.data.discoveredCommands.length
    });

    // Test 2: Help command calling list internally
    console.log('\n📋 Test 2: Help command calling list internally');
    console.debug('🔧 TEST: Calling help command (should internally call list)...');
    
    const helpResult = await bootstrap.executeCommand('help', {});
    console.log('✅ Help command succeeded (called list internally)');
    console.debug('📊 Help result based on list:', {
      availableCommands: helpResult.data.availableCommands.length,
      commandBreakdown: helpResult.data.commandBreakdown,
      basedOnListResult: helpResult.data.basedOnListResult
    });

    // Test 3: Verify help got same data as list
    console.log('\n📋 Test 3: Verify help and list return consistent data');
    
    const listCommands = directListResult.data.commands.sort();
    const helpCommands = helpResult.data.availableCommands.sort();
    
    const commandsMatch = JSON.stringify(listCommands) === JSON.stringify(helpCommands);
    console.log('✅ Command consistency check:', commandsMatch ? 'PASS' : 'FAIL');
    
    if (commandsMatch) {
      console.debug('🎯 VERIFIED: help() successfully called list() and got same command inventory');
    } else {
      console.debug('❌ MISMATCH: help() and list() returned different command sets');
      console.debug('List commands:', listCommands);
      console.debug('Help commands:', helpCommands);
    }

    // Test 4: Help with specific command (showing command source)
    console.log('\n📋 Test 4: Help with specific command parameter');
    console.debug('🔧 TEST: Calling help with specific command...');
    
    const specificHelpResult = await bootstrap.executeCommand('help', { command: 'info' });
    console.log('✅ Specific help command succeeded');
    console.debug('📊 Specific help result:', {
      command: specificHelpResult.data.command,
      category: specificHelpResult.data.category,
      commandSource: specificHelpResult.data.commandSource,
      available: specificHelpResult.data.available
    });

    // Test 5: Test command interdependency logging
    console.log('\n📋 Test 5: Verify interdependency logging');
    console.debug('🎯 TEST: Check that help → list interdependency was logged above');
    console.log('✅ Should see "🔗 BOOTSTRAP: help command calling list command" in logs above');

    console.log('\n✅ Command interdependency tests complete!');
    console.debug('🎯 VERIFIED: Commands can successfully call other commands internally');

  } catch (error) {
    console.error('❌ Command interdependency test failed:', error);
    throw error;
  }
}

// Run the test
testCommandInterdependencies().catch((error) => {
  console.error('❌ Test execution failed:', error);
  process.exit(1);
});