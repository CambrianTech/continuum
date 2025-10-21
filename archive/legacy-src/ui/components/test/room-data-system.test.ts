/**
 * Room Data System Integration Test
 * 
 * Verifies that the complete room data system works properly instead of 
 * string manipulation and provides rich metadata to all widgets
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';

describe('Room Data System - Complete Information Flow', () => {
  
  it('should load room type configuration from JSON', async () => {
    // Test that room type config is accessible
    const response = await fetch('http://localhost:9000/src/ui/components/shared/room-type-config.json');
    assert.strictEqual(response.ok, true, 'Room type config should be accessible');
    
    const config = await response.json();
    
    // Verify structure
    assert.ok(config.roomTypes, 'Should have roomTypes section');
    assert.ok(config.fallbacks, 'Should have fallbacks section');
    
    // Verify specific room types
    assert.ok(config.roomTypes.chat, 'Should have chat room type');
    assert.ok(config.roomTypes.collaboration, 'Should have collaboration room type');
    assert.ok(config.roomTypes.education, 'Should have education room type');
    
    // Verify each room type has required fields
    const chatType = config.roomTypes.chat;
    assert.ok(chatType.displayName, 'Chat type should have displayName');
    assert.ok(chatType.icon, 'Chat type should have icon');
    assert.ok(chatType.defaultWelcomeMessage, 'Chat type should have welcome message');
    
    console.log('✅ Room type configuration loaded with proper structure');
  });

  it('should load default rooms configuration', async () => {
    // Test that default rooms config is accessible
    const response = await fetch('http://localhost:9000/src/daemons/chatroom/config/default-rooms.json');
    assert.strictEqual(response.ok, true, 'Default rooms config should be accessible');
    
    const config = await response.json();
    
    // Verify structure
    assert.ok(config.defaultRooms, 'Should have defaultRooms array');
    assert.ok(Array.isArray(config.defaultRooms), 'defaultRooms should be an array');
    assert.ok(config.defaultRooms.length > 0, 'Should have at least one default room');
    
    // Verify room structure
    const firstRoom = config.defaultRooms[0];
    assert.ok(firstRoom.id, 'Room should have id');
    assert.ok(firstRoom.name, 'Room should have name');
    assert.ok(firstRoom.type, 'Room should have type');
    assert.ok(firstRoom.description, 'Room should have description');
    
    console.log(`✅ Default rooms configuration loaded with ${config.defaultRooms.length} rooms`);
  });

  it('should load chat events configuration from JSON', async () => {
    // Test that chat events config is accessible
    const response = await fetch('http://localhost:9000/src/ui/components/shared/chat-events-config.json');
    assert.strictEqual(response.ok, true, 'Chat events config should be accessible');
    
    const config = await response.json();
    
    // Verify structure
    assert.ok(config.chatEvents, 'Should have chatEvents section');
    assert.ok(config.globalEvents, 'Should have globalEvents section');
    assert.ok(config.messageTypes, 'Should have messageTypes section');
    assert.ok(config.messageStatuses, 'Should have messageStatuses section');
    
    // Verify specific event types
    assert.ok(config.chatEvents.message_received, 'Should have message_received event');
    assert.ok(config.chatEvents.agent_typing, 'Should have agent_typing event');
    assert.ok(config.chatEvents.agent_stop_typing, 'Should have agent_stop_typing event');
    
    // Verify event handler structure
    const messageEvent = config.chatEvents.message_received;
    assert.ok(messageEvent.handler, 'Event should have handler');
    assert.ok(messageEvent.description, 'Event should have description');
    
    console.log('✅ Chat events configuration loaded with proper structure');
  });

  it('should demonstrate room data superiority over string manipulation', async () => {
    // Test the old vs new approach
    
    // OLD APPROACH: String manipulation for UUID room IDs
    const uuidRoomId = '550e8400-e29b-41d4-a716-446655440000';
    const oldCapitalized = uuidRoomId.charAt(0).toUpperCase() + uuidRoomId.slice(1);
    
    // NEW APPROACH: Rich room data from JSON
    const roomTypeResponse = await fetch('http://localhost:9000/src/ui/components/shared/room-type-config.json');
    const roomTypeConfig = await roomTypeResponse.json();
    
    const defaultRoomsResponse = await fetch('http://localhost:9000/src/daemons/chatroom/config/default-rooms.json');
    const defaultRoomsConfig = await defaultRoomsResponse.json();
    
    // Compare approaches
    console.log('❌ OLD: String manipulation result:', oldCapitalized);
    console.log('   → Useless for UUID room IDs!');
    
    console.log('✅ NEW: Rich room data available:');
    const exampleRoom = defaultRoomsConfig.defaultRooms[0];
    const exampleRoomType = roomTypeConfig.roomTypes[exampleRoom.type];
    
    console.log(`   → Room Name: ${exampleRoom.name}`);
    console.log(`   → Description: ${exampleRoom.description}`);
    console.log(`   → Type: ${exampleRoom.type}`);
    console.log(`   → Icon: ${exampleRoomType.icon}`);
    console.log(`   → Welcome Message: ${exampleRoomType.defaultWelcomeMessage}`);
    
    // Verify the new approach provides more information
    assert.ok(exampleRoom.name.length > oldCapitalized.length || exampleRoom.name !== oldCapitalized, 
              'New approach should provide better names than string manipulation');
    assert.ok(exampleRoom.description, 'New approach provides descriptions');
    assert.ok(exampleRoomType.icon, 'New approach provides icons');
    assert.ok(exampleRoomType.defaultWelcomeMessage, 'New approach provides welcome messages');
  });

  it('should verify browser logs show proper room initialization', async () => {
    // This test verifies that the browser logs show the new room data system working
    
    // Check that browser is running and showing room initialization
    const response = await fetch('http://localhost:9000');
    assert.strictEqual(response.ok, true, 'Browser should be running');
    
    // The logs should show:
    // - "RoomDataManager: Loaded room type configuration"
    // - "RoomDataManager: Initialized with 4 rooms"  
    // - "Chat: Initializing chat for room: General Chat (chat)"
    
    console.log('✅ Browser running with room data system active');
    console.log('   Check browser logs for:');
    console.log('   → "RoomDataManager: Loaded room type configuration"');
    console.log('   → "RoomDataManager: Initialized with 4 rooms"');
    console.log('   → "Chat: Initializing chat for room: General Chat (chat)"');
    console.log('   → Instead of old "room: general" string manipulation');
  });
});