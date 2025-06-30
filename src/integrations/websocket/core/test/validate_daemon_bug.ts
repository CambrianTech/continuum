#!/usr/bin/env tsx
/**
 * Simple Bug Validation Script
 * Proves the DaemonConnector bug without complex test infrastructure
 */

import { DaemonConnector } from '../DaemonConnector';

async function validateDaemonConnectorBug() {
  console.log('🔍 Validating DaemonConnector Bug');
  console.log('==================================');
  
  const connector = new DaemonConnector();
  
  try {
    // Connect to "command system" 
    console.log('1. Connecting to command system...');
    const connected = await connector.connect();
    console.log(`   Connected: ${connected}`);
    
    if (!connected) {
      console.log('❌ Failed to connect');
      return;
    }
    
    // Get the command processor
    const connection = (connector as any).connection;
    const processor = connection.commandProcessor;
    
    console.log('2. Checking available commands...');
    const commands = processor.getCommands();
    console.log(`   Available commands: ${JSON.stringify(commands)}`);
    console.log(`   Command count: ${commands.length}`);
    
    // Try executing different commands
    console.log('3. Testing command execution...');
    
    // This should work (hardcoded)
    console.log('   Testing selftest command...');
    try {
      const selftestResult = await processor.executeCommand('selftest', {}, {});
      console.log(`   Selftest result: ${selftestResult.success ? '✅ SUCCESS' : '❌ FAILED'}`);
    } catch (e) {
      console.log(`   Selftest error: ${e}`);
    }
    
    // This should fail (not hardcoded)
    console.log('   Testing health command...');
    try {
      const healthResult = await processor.executeCommand('health', {}, {});
      console.log(`   Health result: ${healthResult.success ? '✅ SUCCESS' : '❌ FAILED'}`);
      if (!healthResult.success) {
        console.log(`   Health error: ${healthResult.error}`);
      }
    } catch (e) {
      console.log(`   Health error: ${e}`);
    }
    
    // This should fail (not hardcoded)  
    console.log('   Testing browser command...');
    try {
      const browserResult = await processor.executeCommand('browser', { devtools: true }, {});
      console.log(`   Browser result: ${browserResult.success ? '✅ SUCCESS' : '❌ FAILED'}`);
      if (!browserResult.success) {
        console.log(`   Browser error: ${browserResult.error}`);
      }
    } catch (e) {
      console.log(`   Browser error: ${e}`);
    }
    
    await connector.disconnect();
    
    console.log('\n📊 BUG VALIDATION RESULTS:');
    console.log('==========================');
    console.log('✅ Expected: Only selftest works, health/browser fail');
    console.log('✅ Expected: Only 1 command in available list');
    console.log('✅ Expected: health/browser return "not found" errors');
    console.log('\n🎯 ROOT CAUSE: DaemonConnector hardcoded switch statement');
    console.log('   instead of real IPC to Command Processor daemon');
    
  } catch (error) {
    console.error('❌ Validation failed:', error);
  }
}

// Run if called directly (module detection for ES modules)
if (require.main === module) {
  validateDaemonConnectorBug();
}