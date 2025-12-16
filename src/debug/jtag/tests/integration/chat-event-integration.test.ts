#!/usr/bin/env tsx
/**
 * Chat Event Integration Test
 * 
 * REFINED TESTING: Clean, focused test for chat message event emission and cross-environment delivery.
 * Eliminates redundancy while maintaining comprehensive coverage.
 */

import { jtag } from '../../server-index';
import { 
  createBrowserEventListenerCode,
  createBrowserEventProofCode,
  waitForEventPropagation,
  validateEventTestResult,
  cleanupBrowserProofElements
} from '../shared/EventTestUtilities';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

async function testChatEventIntegration() {
  console.log('💬 CHAT EVENT INTEGRATION TEST');
  console.log('='.repeat(50));
  
  let client: any;
  
  try {
    // Connect to JTAG system
    console.log('🔌 Connecting to JTAG system...');
    client = await jtag.connect({ targetEnvironment: 'server' });
    console.log('✅ Connected successfully');
    
    const testRoomId = 'chat-integration-room';
    const testSessionId = generateUUID();
    const proofElementId = 'chat-event-proof';
    
    console.log(`🎯 Test Configuration:`);
    console.log(`   Room ID: ${testRoomId}`);
    console.log(`   Session ID: ${testSessionId}`);
    console.log(`   Expected Event: chat-message-sent`);
    
    // Step 1: Setup browser event listener for chat messages
    console.log('\n📝 Step 1: Setting up browser event listener...');
    const setupResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: createBrowserEventListenerCode('chat-message-sent', proofElementId)
      }
    });
    
    if (!setupResult.success) {
      throw new Error(`Browser setup failed: ${setupResult.error}`);
    }
    console.log('✅ Browser event listener ready');
    
    // Step 2: Send chat message that should emit event
    console.log('\n📤 Step 2: Sending chat message...');
    const chatResult = await client.commands['collaboration/chat/send']({
      roomId: testRoomId,
      content: 'Integration test message for event verification',
      sessionId: testSessionId,
      context: { uuid: testSessionId, environment: 'server' }
    });
    
    console.log('📊 Chat Result:', {
      success: chatResult.success,
      messageId: chatResult.messageId,
      error: chatResult.error
    });
    
    if (!chatResult.success) {
      throw new Error(`Chat command failed: ${chatResult.error}`);
    }
    console.log('✅ Chat message sent successfully');
    
    // Step 3: Wait for event propagation
    console.log('\n⏳ Step 3: Waiting for cross-environment event propagation...');
    await waitForEventPropagation(2000);
    
    // Step 4: Verify browser received event
    console.log('\n🔍 Step 4: Verifying browser received event...');
    const verificationResult = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: createBrowserEventProofCode(proofElementId)
      }
    });
    
    if (!verificationResult.success) {
      throw new Error(`Verification failed: ${verificationResult.error}`);
    }
    
    console.log('📊 Verification Result:', verificationResult.result);
    
    // Step 5: Validate results
    console.log('\n✅ Step 5: Validating test results...');
    validateEventTestResult('Chat Event Integration', verificationResult.result);
    
    // Step 6: Cleanup
    console.log('\n🧹 Step 6: Cleaning up test artifacts...');
    await cleanupBrowserProofElements(client, [proofElementId]);
    
    console.log('\n🎉 CHAT EVENT INTEGRATION TEST PASSED!');
    console.log('✅ Chat messages correctly emit cross-environment events');
    console.log('✅ Browser successfully receives server-emitted events');
    console.log('✅ Event bridging system is working correctly');
    
  } catch (error) {
    console.error('\n❌ CHAT EVENT INTEGRATION TEST FAILED:', error);
    
    // Provide helpful debugging information
    if (error instanceof Error) {
      if (error.message.includes('timeout')) {
        console.log('\n💡 System may not be running. Try:');
        console.log('   npm run system:start');
        console.log('   npm run signal:wait');
      } else if (error.message.includes('JTAG system not available')) {
        console.log('\n💡 Browser may not be connected. Check:');
        console.log('   Browser console at http://localhost:9002');
        console.log('   System logs: npm run logs:current');
      }
    }
    
    // Attempt cleanup even on failure
    if (client) {
      try {
        await cleanupBrowserProofElements(client, ['chat-event-proof']);
      } catch (cleanupError) {
        console.warn('⚠️ Cleanup failed:', cleanupError);
      }
    }
    
    throw error;
  }
}

// Run the test
runChatEventIntegration().then(() => {
  console.log('✅ Chat event integration test completed successfully');
  process.exit(0);
}).catch((error) => {
  console.error('💥 Chat event integration test failed');
  process.exit(1);
});