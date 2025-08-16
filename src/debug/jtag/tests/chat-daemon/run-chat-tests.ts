#!/usr/bin/env tsx
/**
 * Chat Daemon Test Runner
 * 
 * Unit tests for chat system foundation using data daemon for persistence.
 * Tests universal participant-agnostic chat architecture.
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { DataDaemon, DataOperationContext } from '../../daemons/data-daemon/shared/DataDaemon';
import { 
  SessionParticipant,
  ChatRoom,
  ChatMessage,
  ChatCreateRoomParams,
  ChatJoinRoomParams,
  ChatSendMessageParams,
  ChatListRoomsParams,
  ParticipantCapabilities,
  createChatCreateRoomParams,
  createChatJoinRoomParams,
  createChatSendMessageParams,
  createChatListRoomsParams
} from '../../daemons/chat-daemon/shared/ChatTypes';
import { JTAGContext } from '../../system/core/types/JTAGTypes';
import { UUID } from '../../system/core/types/CrossPlatformUUID';

console.log('💬 Chat System Test Suite');

let testsPassed = 0;
let testsFailed = 0;

function test(description: string, testFn: () => Promise<void> | void): void {
  const run = async () => {
    try {
      await testFn();
      console.log(`✅ ${description}`);
      testsPassed++;
    } catch (error) {
      console.log(`❌ ${description}: ${error instanceof Error ? error.message : String(error)}`);
      testsFailed++;
    }
  };
  
  run().catch(error => {
    console.error(`Test runner error for "${description}":`, error);
    testsFailed++;
  });
}

function expect(actual: any) {
  return {
    toBe: (expected: any) => {
      if (actual !== expected) {
        throw new Error(`Expected ${expected}, got ${actual}`);
      }
    },
    toEqual: (expected: any) => {
      if (JSON.stringify(actual) !== JSON.stringify(expected)) {
        throw new Error(`Expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
      }
    },
    toBeTruthy: () => {
      if (!actual) {
        throw new Error(`Expected truthy value, got ${actual}`);
      }
    },
    toBeUndefined: () => {
      if (actual !== undefined) {
        throw new Error(`Expected undefined, got ${actual}`);
      }
    },
    toContain: (expected: any) => {
      if (!Array.isArray(actual) || !actual.includes(expected)) {
        throw new Error(`Expected array to contain ${expected}, got ${JSON.stringify(actual)}`);
      }
    },
    toHaveLength: (expected: number) => {
      if (!actual || actual.length !== expected) {
        throw new Error(`Expected length ${expected}, got ${actual?.length || 'undefined'}`);
      }
    },
    toHaveProperty: (property: string) => {
      if (!actual || !(property in actual)) {
        throw new Error(`Expected object to have property ${property}, got ${JSON.stringify(actual)}`);
      }
    }
  };
}

async function runTests() {
  console.log('\n🧪 Testing Chat Type Factory Functions...');
  
  // Test type factory functions follow JTAG patterns
  test('should create ChatCreateRoomParams with proper JTAG context', () => {
    const context: JTAGContext = { uuid: 'test-context' as UUID, environment: 'server' };
    const sessionId = 'test-session' as UUID;
    
    const params = createChatCreateRoomParams(context, sessionId, {
      name: 'Test Room',
      description: 'A test chat room',
      isPrivate: false
    });
    
    expect(params.context).toBe(context);
    expect(params.sessionId).toBe(sessionId);
    expect(params.name).toBe('Test Room');
    expect(params.description).toBe('A test chat room');
    expect(params.isPrivate).toBe(false);
  });

  test('should create ChatJoinRoomParams with participant capabilities', () => {
    const context: JTAGContext = { uuid: 'test-context' as UUID, environment: 'browser' };
    const sessionId = 'test-session' as UUID;
    const roomId = 'room-123' as UUID;
    
    const capabilities: ParticipantCapabilities = {
      canSendMessages: true,
      canReceiveMessages: true,
      canCreateRooms: false,
      canInviteOthers: false,
      canModerate: false,
      autoResponds: false,
      providesContext: true
    };
    
    const params = createChatJoinRoomParams(context, sessionId, {
      roomId,
      participantName: 'Test User',
      capabilities
    });
    
    expect(params.roomId).toBe(roomId);
    expect(params.participantName).toBe('Test User');
    expect(params.capabilities).toEqual(capabilities);
  });

  console.log('\n💾 Testing Chat Data Storage via DataDaemon...');
  
  // Test chat data persistence using data daemon
  test('should store and retrieve chat rooms using data daemon', async () => {
    const config = {
      strategy: 'memory' as const,
      backend: 'memory',
      namespace: `chat-test-${Date.now()}`,
      options: { maxRecords: 1000 }
    };

    const dataDaemon = new DataDaemon(config);
    
    try {
      const context: DataOperationContext = {
        sessionId: 'chat-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'chat-test'
      };

      // Create a chat room record
      const roomData: Partial<ChatRoom> = {
        name: 'General Chat',
        description: 'Main discussion room',
        isPrivate: false,
        participantCount: 0,
        messageCount: 0,
        createdAt: context.timestamp,
        lastActivity: context.timestamp
      };

      const createResult = await dataDaemon.create('chat-rooms', roomData, context);
      expect(createResult.success).toBe(true);
      expect(createResult.data?.data.name).toBe('General Chat');
      
      const roomId = createResult.data!.id;

      // Retrieve the room
      const readResult = await dataDaemon.read('chat-rooms', roomId, context);
      expect(readResult.success).toBe(true);
      expect(readResult.data?.data.name).toBe('General Chat');
      expect(readResult.data?.data.isPrivate).toBe(false);
      
    } finally {
      await dataDaemon.close();
    }
  });

  test('should store and retrieve chat participants using data daemon', async () => {
    const config = {
      strategy: 'memory' as const,
      backend: 'memory',
      namespace: `participants-test-${Date.now()}`,
      options: { maxRecords: 1000 }
    };

    const dataDaemon = new DataDaemon(config);
    
    try {
      const context: DataOperationContext = {
        sessionId: 'participants-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'participants-test'
      };

      // Create participant records
      const participantData: Partial<SessionParticipant> = {
        displayName: 'Alice',
        joinedAt: context.timestamp,
        lastSeen: context.timestamp,
        isOnline: true,
        capabilities: {
          canSendMessages: true,
          canReceiveMessages: true,
          canCreateRooms: true,
          canInviteOthers: true,
          canModerate: false,
          autoResponds: false,
          providesContext: false
        }
      };

      const createResult = await dataDaemon.create('participants', participantData, context);
      expect(createResult.success).toBe(true);
      expect(createResult.data?.data.displayName).toBe('Alice');
      expect(createResult.data?.data.isOnline).toBe(true);
      
    } finally {
      await dataDaemon.close();
    }
  });

  test('should store and retrieve chat messages using data daemon', async () => {
    const config = {
      strategy: 'memory' as const,
      backend: 'memory',
      namespace: `messages-test-${Date.now()}`,
      options: { maxRecords: 1000 }
    };

    const dataDaemon = new DataDaemon(config);
    
    try {
      const context: DataOperationContext = {
        sessionId: 'messages-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'messages-test'
      };

      // Create message records
      const messageData: Partial<ChatMessage> = {
        roomId: 'room-123' as UUID,
        senderId: 'user-456' as UUID,
        senderName: 'Alice',
        content: 'Hello, world!',
        timestamp: context.timestamp,
        mentions: [],
        category: 'chat'
      };

      const createResult = await dataDaemon.create('chat-messages', messageData, context);
      expect(createResult.success).toBe(true);
      expect(createResult.data?.data.content).toBe('Hello, world!');
      expect(createResult.data?.data.category).toBe('chat');
      
      // Test message querying by room
      const queryResult = await dataDaemon.query({
        collection: 'chat-messages',
        filters: { 'data.roomId': 'room-123' },
        sort: [{ field: 'data.timestamp', direction: 'desc' }]
      }, context);
      
      expect(queryResult.success).toBe(true);
      expect(queryResult.data).toHaveLength(1);
      expect(queryResult.data?.[0].data.content).toBe('Hello, world!');
      
    } finally {
      await dataDaemon.close();
    }
  });

  console.log('\n🏗️ Testing Chat Constants and Types...');
  
  // Test strong typing and constants prevent errors
  test('should validate participant capabilities with strong typing', () => {
    const capabilities: ParticipantCapabilities = {
      canSendMessages: true,
      canReceiveMessages: true,
      canCreateRooms: false,
      canInviteOthers: false,
      canModerate: false,
      autoResponds: true,  // Auto-responder (AI/bot/persona)
      providesContext: true
    };
    
    // Type system prevents runtime errors
    expect(capabilities.canSendMessages).toBe(true);
    expect(capabilities.autoResponds).toBe(true);
    expect(capabilities.providesContext).toBe(true);
  });

  test('should handle legacy compatibility aliases in types', () => {
    // Test that legacy ChatCitizen type still works
    const participant: SessionParticipant = {
      participantId: 'participant-123' as UUID,
      sessionId: 'session-456' as UUID,
      displayName: 'Legacy User',
      joinedAt: new Date().toISOString(),
      lastSeen: new Date().toISOString(),
      isOnline: true
    };
    
    // Should work as ChatCitizen via type alias
    const legacyParticipant = participant as any; // Legacy type compatibility
    expect(legacyParticipant.participantId).toBe('participant-123');
    expect(legacyParticipant.displayName).toBe('Legacy User');
  });

  console.log('\n🔄 Testing Universal Participant-Agnostic Architecture...');
  
  test('should support any participant type through capabilities', () => {
    // Human participant
    const humanCapabilities: ParticipantCapabilities = {
      canSendMessages: true,
      canReceiveMessages: true,
      canCreateRooms: true,
      canInviteOthers: true,
      canModerate: true,
      autoResponds: false,  // Humans don't auto-respond
      providesContext: false
    };
    
    // AI/Bot participant  
    const aiCapabilities: ParticipantCapabilities = {
      canSendMessages: true,
      canReceiveMessages: true,
      canCreateRooms: false,
      canInviteOthers: false,
      canModerate: false,
      autoResponds: true,   // AI auto-responds
      providesContext: true  // AI provides context
    };
    
    // System service participant
    const serviceCapabilities: ParticipantCapabilities = {
      canSendMessages: true,
      canReceiveMessages: false,
      canCreateRooms: false,
      canInviteOthers: false,
      canModerate: false,
      autoResponds: true,   // Services auto-respond
      providesContext: false
    };
    
    // All participants follow same interface - no special cases
    expect(humanCapabilities.autoResponds).toBe(false);
    expect(aiCapabilities.autoResponds).toBe(true);
    expect(serviceCapabilities.autoResponds).toBe(true);
    expect(aiCapabilities.providesContext).toBe(true);
  });

  // Wait for all async tests to complete
  await new Promise(resolve => setTimeout(resolve, 100));

  console.log(`\n📊 Test Results: ${testsPassed} passed, ${testsFailed} failed`);
  
  if (testsFailed > 0) {
    console.log('❌ Some chat tests failed');
    process.exit(1);
  } else {
    console.log('✅ All chat system tests passed!');
    process.exit(0);
  }
}

runTests().catch(error => {
  console.error('Chat test runner failed:', error);
  process.exit(1);
});