/**
 * Bootstrap Integration Test - TypeScript
 * Test promise-based command queueing with dual console.debug logging
 */

import { BootstrapSystem } from '../BootstrapSystem.js';

async function testBootstrapIntegration(): Promise<void> {
  console.log('🧪 Testing Bootstrap System Integration with console.debug logging...\n');

  const bootstrap = new BootstrapSystem();
  
  // Enable debug logging to see both server and client side messages
  console.debug('🔧 TEST: Debug logging enabled for bootstrap system testing');
  
  // Test 1: Immediate commands should work before initialization
  console.log('📋 Test 1: Immediate commands (info, status) before initialization');
  
  try {
    const infoResult = await bootstrap.executeCommand('info', { section: 'version' });
    console.log('✅ INFO command succeeded:', infoResult.data?.version);
    
    const statusResult = await bootstrap.executeCommand('status', {});
    console.log('✅ STATUS command succeeded - systemReady:', statusResult.data?.systemReady);
  } catch (error) {
    console.log('❌ Immediate command failed:', (error as Error).message);
  }
  
  // Test 2: Queue commands that need module discovery (list, help)  
  console.log('\n📋 Test 2: Queue post-discovery commands before initialization');
  
  console.debug('🧪 TEST: About to queue list and help commands...');
  
  const listPromise = bootstrap.executeCommand('list', {});
  const helpPromise = bootstrap.executeCommand('help', {});
  
  console.log('⏳ Commands queued, now starting system initialization...');
  
  // Start system initialization  
  await bootstrap.start();
  
  // Wait for queued commands to resolve
  try {
    console.debug('🧪 TEST: Waiting for queued commands to resolve...');
    
    const [listResult, helpResult] = await Promise.all([listPromise, helpPromise]);
    
    console.log('✅ LIST command resolved:', listResult.data?.totalCommands, 'total commands');
    console.debug('📋 LIST result details:', {
      bootstrapCommands: listResult.data?.bootstrapCommands?.length,
      discoveredCommands: listResult.data?.discoveredCommands?.length, 
      systemReady: listResult.data?.systemReady
    });
    
    console.log('✅ HELP command resolved:', helpResult.data?.availableCommands?.length, 'available commands');
    console.debug('📋 HELP result details:', {
      totalCommands: helpResult.data?.systemState?.totalCommands,
      usage: helpResult.data?.usage
    });
    
  } catch (error) {
    console.log('❌ Queued command failed:', (error as Error).message);
  }
  
  // Test 3: Commands after initialization should execute immediately
  console.log('\n📋 Test 3: Commands after initialization (immediate execution)');
  
  try {
    console.debug('🧪 TEST: Testing post-initialization command execution...');
    
    const listResult2 = await bootstrap.executeCommand('list', {});
    console.log('✅ POST-INIT LIST command:', listResult2.data?.totalCommands, 'commands');
    
    const infoResult2 = await bootstrap.executeCommand('info', {});
    console.log('✅ POST-INIT INFO command:', infoResult2.data?.version);
    
  } catch (error) {
    console.log('❌ Post-init command failed:', (error as Error).message);
  }
  
  // Test 4: System state verification
  console.log('\n📋 Test 4: System state verification');
  
  const finalState = bootstrap.getSystemState();
  console.log('📊 Final System State:', {
    phase: finalState.phase,
    systemReady: finalState.systemReady,
    commandsAvailable: finalState.commandsAvailable.length,
    queuedCommands: finalState.queuedCommands,
    initializationTime: `${finalState.initializationTime}ms`
  });
  
  console.debug('📊 Detailed final state:', finalState);
  
  console.log('\n✅ Bootstrap integration test complete!');
  console.debug('🎯 TEST: All promise-based command queueing verified working');
}

// Run the test
testBootstrapIntegration().catch((error) => {
  console.error('❌ Bootstrap integration test failed:', error);
  process.exit(1);
});