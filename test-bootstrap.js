/**
 * Test Bootstrap System - Promise-based command queueing
 */

import { BootstrapSystem } from './src/core/bootstrap/BootstrapSystem.js';

async function testBootstrapSystem() {
  console.log('🧪 Testing Bootstrap System with promise-based command queueing...\n');

  const bootstrap = new BootstrapSystem();
  
  // Test 1: Immediate commands should work before initialization
  console.log('📋 Test 1: Immediate commands before initialization');
  try {
    const infoResult = await bootstrap.executeCommand('info', {});
    console.log('✅ INFO command succeeded:', infoResult.data.version);
    
    const statusResult = await bootstrap.executeCommand('status', {});
    console.log('✅ STATUS command succeeded:', statusResult.data.systemReady);
  } catch (error) {
    console.log('❌ Immediate command failed:', error.message);
  }
  
  // Test 2: Queue commands that need module discovery
  console.log('\n📋 Test 2: Queue commands before module discovery');
  
  const listPromise = bootstrap.executeCommand('list', {});
  const helpPromise = bootstrap.executeCommand('help', {});
  
  console.log('⏳ Commands queued, now starting system initialization...');
  
  // Start system initialization
  bootstrap.start();
  
  // Wait for queued commands to resolve
  try {
    const [listResult, helpResult] = await Promise.all([listPromise, helpPromise]);
    
    console.log('✅ LIST command resolved:', listResult.data.totalCommands, 'commands');
    console.log('✅ HELP command resolved:', helpResult.data.availableCommands.length, 'available');
  } catch (error) {
    console.log('❌ Queued command failed:', error.message);
  }
  
  // Test 3: Commands after initialization should execute immediately
  console.log('\n📋 Test 3: Commands after initialization');
  
  try {
    const listResult2 = await bootstrap.executeCommand('list', {});
    console.log('✅ POST-INIT LIST command:', listResult2.data.totalCommands, 'commands');
  } catch (error) {
    console.log('❌ Post-init command failed:', error.message);
  }
  
  console.log('\n✅ Bootstrap system test complete!');
  console.log('📊 System State:', bootstrap.getSystemState());
}

testBootstrapSystem().catch(console.error);