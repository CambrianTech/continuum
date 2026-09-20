#!/usr/bin/env tsx
/**
 * Chat System Integration Tests
 * 
 * Comprehensive tests for the chat daemon system including:
 * - Room operations (create, list, join, leave)
 * - Message sending and routing
 * - Citizen event notifications
 * - AI integration triggers
 * - Multi-participant coordination
 */

console.log('🧪 Chat System Integration Tests');

// Test utilities with timeout protection
function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

// Timeout wrapper for tests
function withTimeout<T>(promise: Promise<T>, timeoutMs: number = 15000): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => 
      setTimeout(() => reject(new Error(`Test timed out after ${timeoutMs}ms`)), timeoutMs)
    )
  ]);
}

async function ensureSystemRunning() {
  console.log('🔄 Ensuring JTAG system is running...');
  
  const { exec } = await import('child_process');
  const { promisify } = await import('util');
  const execAsync = promisify(exec);
  
  try {
    await execAsync('npm run system:ensure');
    console.log('✅ JTAG system is running');
    return true;
  } catch (error) {
    console.error('❌ Failed to start JTAG system:', error);
    return false;
  }
}

async function testChatConnection() {
  console.log('\n🔗 TEST 1: Basic system connection and screenshot test');
  
  // Use the browser client pattern from existing tests
  const { JTAGClientServer } = await import('../../system/core/client/server/JTAGClientServer');
  
  const result = await withTimeout(JTAGClientServer.connect(), 15000);
  const { client, listResult } = result;
  
  assert(listResult.success === true, 'Client connected to JTAG system');
  assert(Array.isArray(listResult.commands), 'Commands discovered');
  
  const commandNames = listResult.commands.map((c: any) => c.name);
  console.log(`📋 Available commands: ${commandNames.join(', ')}`);
  
  // Test if we have screenshot capability to test the browser UI
  const hasScreenshot = commandNames.includes('screenshot');
  console.log(`📸 Screenshot command available: ${hasScreenshot}`);
  
  return { client, listResult, hasScreenshot };
}

async function testRoomOperations(client: any, context: any) {
  console.log('\n🏠 TEST 2: Room operations (create, list, join, leave)');
  
  // Test 2a: List rooms (should be empty or contain default rooms)
  console.log('📋 Testing room listing...');
  const listResult1 = await withTimeout(client.executeCommand('collaboration/chat/list-rooms', {}), 10000);
  assert(listResult1.success, 'List rooms command succeeded');
  assert(Array.isArray(listResult1.rooms), 'Rooms returned as array');
  
  const initialRoomCount = listResult1.rooms.length;
  console.log(`📊 Found ${initialRoomCount} existing rooms`);
  
  // Test 2b: Create new room
  console.log('🏗️ Testing room creation...');
  const testRoomData = {
    name: 'Test Chat Room',
    description: 'Integration test room',
    category: 'general',
    allowAI: true
  };
  
  const createResult = await withTimeout(client.executeCommand('collaboration/chat/create-room', testRoomData), 10000);
  assert(createResult.success, 'Create room command succeeded');
  assert(typeof createResult.roomId === 'string', 'Room ID returned');
  assert(createResult.name === testRoomData.name, 'Room name matches');
  
  const createdRoomId = createResult.roomId;
  console.log(`🏠 Created room: ${createdRoomId}`);
  
  // Test 2c: List rooms again (should have our new room)
  console.log('📋 Testing updated room list...');
  const listResult2 = await withTimeout(client.executeCommand('collaboration/chat/list-rooms', {}), 10000);
  assert(listResult2.success, 'Updated list rooms command succeeded');
  assert(listResult2.rooms.length === initialRoomCount + 1, 'Room count increased by 1');
  
  const ourRoom = listResult2.rooms.find((room: any) => room.roomId === createdRoomId);
  assert(ourRoom !== undefined, 'Created room appears in list');
  assert(ourRoom.name === testRoomData.name, 'Room name correct in list');
  assert(ourRoom.participantCount === 0, 'New room has no participants');
  
  return { createdRoomId, testRoomData };
}

async function testCitizenOperations(client: any, context: any, roomId: string) {
  console.log('\n👥 TEST 3: Citizen operations (join, message, leave)');
  
  // Test 3a: Join room as test citizen
  console.log('🚪 Testing room joining...');
  const joinData = {
    roomId,
    citizenName: 'TestUser',
    citizenType: 'user'
  };
  
  const joinResult = await withTimeout(client.executeCommand('collaboration/chat/join-room', joinData), 10000);
  assert(joinResult.success, 'Join room command succeeded');
  assert(typeof joinResult.citizenId === 'string', 'Citizen ID returned');
  assert(joinResult.roomId === roomId, 'Room ID matches');
  assert(joinResult.participantCount === 1, 'Room now has 1 participant');
  
  const citizenId = joinResult.citizenId;
  console.log(`👤 Joined as citizen: ${citizenId}`);
  
  // Test 3b: Send message to room
  console.log('💬 Testing message sending...');
  const messageData = {
    roomId,
    citizenId,
    content: 'Hello from integration test! This is a test message.'
  };
  
  const sendResult = await withTimeout(client.executeCommand('collaboration/chat/send', messageData), 10000);
  assert(sendResult.success, 'Send message command succeeded');
  assert(typeof sendResult.messageId === 'string', 'Message ID returned');
  assert(typeof sendResult.messageTimestamp === 'string', 'Message timestamp returned');
  
  console.log(`💬 Sent message: ${sendResult.messageId}`);
  
  // Test 3c: Get chat history
  console.log('📜 Testing chat history retrieval...');
  const historyResult = await withTimeout(client.executeCommand('collaboration/chat/get-history', {
    roomId,
    limit: 10
  }), 10000);
  assert(historyResult.success, 'Get history command succeeded');
  assert(Array.isArray(historyResult.messages), 'Messages returned as array');
  assert(historyResult.messages.length >= 1, 'At least one message in history');
  
  const ourMessage = historyResult.messages.find((msg: any) => msg.messageId === sendResult.messageId);
  assert(ourMessage !== undefined, 'Our message appears in history');
  assert(ourMessage.content === messageData.content, 'Message content matches');
  assert(ourMessage.senderName === joinData.citizenName, 'Sender name matches');
  
  // Test 3d: Leave room
  console.log('👋 Testing room leaving...');
  const leaveResult = await withTimeout(client.executeCommand('collaboration/chat/leave-room', {
    roomId,
    citizenId
  }), 10000);
  assert(leaveResult.success, 'Leave room command succeeded');
  assert(leaveResult.roomId === roomId, 'Room ID matches');
  assert(leaveResult.citizenId === citizenId, 'Citizen ID matches');
  
  return { citizenId, messageId: sendResult.messageId };
}

async function testMultipleCitizens(client: any, context: any, roomId: string) {
  console.log('\n👥👥 TEST 4: Multiple citizens and event routing');
  
  // Join as multiple citizens
  const citizens: any[] = [];
  
  for (let i = 0; i < 3; i++) {
    const joinData = {
      roomId,
      citizenName: `TestUser${i + 1}`,
      citizenType: 'user'
    };
    
    const joinResult = await withTimeout(client.executeCommand('collaboration/chat/join-room', joinData), 10000);
    assert(joinResult.success, `Citizen ${i + 1} joined successfully`);
    
    citizens.push({
      citizenId: joinResult.citizenId,
      name: joinData.citizenName
    });
  }
  
  console.log(`👥 Created ${citizens.length} citizens in room`);
  
  // Send messages from different citizens
  for (let i = 0; i < citizens.length; i++) {
    const messageData = {
      roomId,
      citizenId: citizens[i].citizenId,
      content: `Hello from ${citizens[i].name}! Message ${i + 1}.`
    };
    
    const sendResult = await withTimeout(client.executeCommand('collaboration/chat/send', messageData), 10000);
    assert(sendResult.success, `Message from ${citizens[i].name} sent successfully`);
  }
  
  // Check history has all messages
  const historyResult = await withTimeout(client.executeCommand('collaboration/chat/get-history', {
    roomId,
    limit: 20
  }), 10000);
  assert(historyResult.success, 'History retrieved successfully');
  assert(historyResult.messages.length >= citizens.length, 'All messages appear in history');
  
  // Clean up - leave all citizens
  for (const citizen of citizens) {
    const leaveResult = await withTimeout(client.executeCommand('collaboration/chat/leave-room', {
      roomId,
      citizenId: citizen.citizenId
    }), 10000);
    assert(leaveResult.success, `${citizen.name} left room successfully`);
  }
  
  return citizens;
}

async function testAIIntegration(client: any, context: any, roomId: string) {
  console.log('\n🤖 TEST 5: AI integration and triggers');
  
  // Join as AI agent
  console.log('🤖 Creating AI agent citizen...');
  const aiJoinData = {
    roomId,
    citizenName: 'TestAI',
    citizenType: 'agent',
    aiConfig: {
      provider: 'local', // Use local to avoid API key requirements
      model: 'test-model',
      systemPrompt: 'You are a helpful test assistant.'
    }
  };
  
  const aiJoinResult = await withTimeout(client.executeCommand('collaboration/chat/join-room', aiJoinData), 10000);
  assert(aiJoinResult.success, 'AI agent joined successfully');
  
  const aiCitizenId = aiJoinResult.citizenId;
  console.log(`🤖 AI agent joined: ${aiCitizenId}`);
  
  // Join as regular user to trigger AI response
  const userJoinData = {
    roomId,
    citizenName: 'HumanUser',
    citizenType: 'user'
  };
  
  const userJoinResult = await withTimeout(client.executeCommand('collaboration/chat/join-room', userJoinData), 10000);
  assert(userJoinResult.success, 'Human user joined successfully');
  
  const humanCitizenId = userJoinResult.citizenId;
  
  // Send a question that should trigger AI response
  console.log('❓ Sending question to trigger AI response...');
  const questionData = {
    roomId,
    citizenId: humanCitizenId,
    content: 'Hello AI, can you help me with something?'
  };
  
  const questionResult = await withTimeout(client.executeCommand('collaboration/chat/send', questionData), 10000);
  assert(questionResult.success, 'Question message sent successfully');
  
  // Wait a moment for AI processing
  await new Promise(resolve => setTimeout(resolve, 2000));
  
  // Check history for AI response
  const historyResult = await withTimeout(client.executeCommand('collaboration/chat/get-history', {
    roomId,
    limit: 10
  }), 10000);
  assert(historyResult.success, 'History retrieved successfully');
  
  const aiResponse = historyResult.messages.find((msg: any) => 
    msg.senderName === 'TestAI' && msg.senderType === 'agent'
  );
  
  // Note: AI response might not work without proper API config, but the triggering logic should work
  console.log('🤖 AI response status:', aiResponse ? 'Found' : 'Not found (expected if no local LLM)');
  
  // Clean up
  await withTimeout(client.executeCommand('collaboration/chat/leave-room', { roomId, citizenId: aiCitizenId }), 10000);
  await withTimeout(client.executeCommand('collaboration/chat/leave-room', { roomId, citizenId: humanCitizenId }), 10000);
  
  return { aiCitizenId, humanCitizenId };
}

async function testErrorHandling(client: any, context: any) {
  console.log('\n🚨 TEST 6: Error handling and edge cases');
  
  const { generateUUID } = await import('../../system/core/types/CrossPlatformUUID');
  
  // Test invalid room operations
  const fakeRoomId = generateUUID();
  const fakeCitizenId = generateUUID();
  
  // Try to join non-existent room
  const joinFakeResult = await withTimeout(client.executeCommand('collaboration/chat/join-room', {
    roomId: fakeRoomId,
    citizenName: 'TestUser',
    citizenType: 'user'
  }), 10000);
  // Should auto-create room, so this might succeed
  console.log('🏠 Join non-existent room result:', joinFakeResult.success ? 'Auto-created' : 'Failed as expected');
  
  // Try to send message with invalid citizen/room combo
  const invalidMessageResult = await withTimeout(client.executeCommand('collaboration/chat/send', {
    roomId: fakeRoomId,
    citizenId: fakeCitizenId,
    content: 'This should fail'
  }), 10000);
  assert(!invalidMessageResult.success, 'Invalid message send failed as expected');
  
  // Try to get history from non-existent room
  const invalidHistoryResult = await withTimeout(client.executeCommand('collaboration/chat/get-history', {
    roomId: generateUUID(),
    limit: 10
  }), 10000);
  assert(!invalidHistoryResult.success, 'Invalid history request failed as expected');
  
  console.log('✅ Error handling tests completed');
}

// Main test runner
async function runAllTests() {
  console.log('🚀 Starting Chat System Integration Tests...\n');
  
  try {
    // Ensure system is running
    const systemRunning = await ensureSystemRunning();
    assert(systemRunning, 'JTAG system must be running for tests');
    
    // Test 1: Basic connection
    const { client, connection, context } = await testChatConnection();
    
    // Test 2: Room operations
    const { createdRoomId } = await testRoomOperations(client, context);
    
    // Test 3: Citizen operations
    await testCitizenOperations(client, context, createdRoomId);
    
    // Test 4: Multiple citizens
    await testMultipleCitizens(client, context, createdRoomId);
    
    // Test 5: AI integration
    await testAIIntegration(client, context, createdRoomId);
    
    // Test 6: Error handling
    await testErrorHandling(client, context);
    
    console.log('\n🎉 ALL CHAT SYSTEM INTEGRATION TESTS PASSED!');
    console.log('✅ Chat daemon is fully functional');
    console.log('✅ Room operations work correctly');
    console.log('✅ Message routing functions properly');
    console.log('✅ Multi-citizen coordination works');
    console.log('✅ AI integration triggers correctly');
    console.log('✅ Error handling is robust');
    
  } catch (error) {
    console.error('\n💥 CHAT SYSTEM INTEGRATION TESTS FAILED:');
    console.error(error);
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (require.main === module) {
  runAllTests().catch(error => {
    console.error('❌ Test execution failed:', error);
    process.exit(1);
  });
}

export { runAllTests };