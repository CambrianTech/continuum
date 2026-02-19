/**
 * Chat Command Integration Tests - Universal Architecture
 * 
 * Tests the location-transparent command-based chat system
 */

import { describe, test, expect, beforeEach, afterEach } from '@jest/globals';
import type { JTAGContext } from '../system/core/types/JTAGTypes';
import { generateUUID } from '../system/core/types/CrossPlatformUUID';
import { ChatJoinRoomCommand } from '../daemons/chat-daemon/commands/ChatJoinRoomCommand';
import { ChatSendMessageCommand } from '../daemons/chat-daemon/commands/ChatSendMessageCommand';
import { ChatRoomUpdateCommand } from '../daemons/chat-daemon/commands/ChatRoomUpdateCommand';
import { RoomCommandCoordinator } from '../daemons/chat-daemon/shared/RoomCommandSystem';
import { 
  createChatJoinRoomParams,
  createChatSendMessageParams,
  type SessionParticipant,
  type ChatMessage
} from '../daemons/chat-daemon/shared/ChatTypes';

// Mock JTAG context
function createMockContext(): JTAGContext {
  return {
    uuid: generateUUID(),
    environment: 'test',
    nodeId: 'test-node-1',
    sessionInfo: {
      displayName: 'Test Session'
    }
  } as JTAGContext;
}

// Mock JTAG router
function createMockRouter() {
  const messages: any[] = [];
  
  return {
    postMessage: async (message: any) => {
      messages.push(message);
      return { success: true, correlationId: message.correlationId };
    },
    registerCommandHandler: async (endpoint: string, handler: any) => {
      console.log(`Mock registered: ${endpoint}`);
    },
    getMessages: () => messages,
    clearMessages: () => messages.splice(0, messages.length)
  };
}

describe('Chat Command System Integration', () => {
  let context: JTAGContext;
  let router: any;
  let roomCoordinator: RoomCommandCoordinator;
  
  beforeEach(() => {
    context = createMockContext();
    router = createMockRouter();
    roomCoordinator = new RoomCommandCoordinator(context, router, 'test-node-1');
  });

  test('Universal participant joining - Human user', async () => {
    const joinRoomCommand = new ChatJoinRoomCommand(context, router, roomCoordinator);
    
    // Create join parameters for human user
    const joinParams = createChatJoinRoomParams(context, context.uuid, {
      roomId: 'test-room-1',
      participantName: 'Alice',
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: false, // Human user - no auto-response
        providesContext: false
      }
    });
    
    // Execute join command
    const result = await joinRoomCommand.execute(joinParams);
    
    // Verify successful join
    expect(result.success).toBe(true);
    expect(result.participantId).toBeDefined();
    expect(result.room.roomId).toBe('test-room-1');
    
    // Verify participant was added to room coordinator
    const roomParticipants = roomCoordinator.getRoomParticipants('test-room-1');
    expect(roomParticipants.length).toBe(1);
    expect(roomParticipants[0].displayName).toBe('Alice');
    expect(roomParticipants[0].capabilities?.autoResponds).toBe(false);
  });
  
  test('Universal participant joining - AI agent', async () => {
    const joinRoomCommand = new ChatJoinRoomCommand(context, router, roomCoordinator);
    
    // Create join parameters for AI agent
    const joinParams = createChatJoinRoomParams(context, context.uuid, {
      roomId: 'test-room-1',
      participantName: 'Claude AI',
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: true, // AI agent - auto-responds
        providesContext: true
      },
      adapter: {
        type: 'ai-api',
        config: {
          provider: 'anthropic',
          model: 'claude-3-haiku',
          apiKey: 'test-key'
        },
        responseStrategy: {
          triggers: [
            { type: 'mention', probability: 1.0 },
            { type: 'question', probability: 0.8 }
          ],
          style: {
            maxLength: 500,
            tone: 'helpful',
            context: 'room-context'
          }
        }
      }
    });
    
    const result = await joinRoomCommand.execute(joinParams);
    
    // Verify successful join
    expect(result.success).toBe(true);
    expect(result.participantId).toBeDefined();
    
    // Verify AI participant configuration
    const roomParticipants = roomCoordinator.getRoomParticipants('test-room-1');
    const aiParticipant = roomParticipants.find(p => p.displayName === 'Claude AI');
    expect(aiParticipant).toBeDefined();
    expect(aiParticipant?.capabilities?.autoResponds).toBe(true);
    expect(aiParticipant?.adapter?.type).toBe('ai-api');
  });

  test('Universal message sending - Location transparent', async () => {
    const sendMessageCommand = new ChatSendMessageCommand(context, router, roomCoordinator);
    
    // First, add some participants to the room
    roomCoordinator.addParticipantToRoom('test-room-1', {
      participantId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Alice',
      joinedAt: new Date().toISOString(),
      lastSeen: new Date().toISOString(),
      isOnline: true,
      capabilities: { 
        canSendMessages: true, 
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false, 
        canModerate: false,
        autoResponds: false,
        providesContext: false
      }
    }, 'test-node-1');
    
    roomCoordinator.addParticipantToRoom('test-room-1', {
      participantId: generateUUID(),
      sessionId: generateUUID(), 
      displayName: 'Bob',
      joinedAt: new Date().toISOString(),
      lastSeen: new Date().toISOString(),
      isOnline: true,
      capabilities: { 
        canSendMessages: true, 
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: true, // Bob is an AI
        providesContext: false
      }
    }, 'remote-node-2'); // Bob is on a different node
    
    // Create send message parameters
    const sendParams = createChatSendMessageParams(context, context.uuid, {
      roomId: 'test-room-1',
      content: 'Hello everyone! How are you doing?',
      category: 'chat',
      mentions: []
    });
    
    // Execute send message command
    const result = await sendMessageCommand.execute(sendParams);
    
    // Verify message was sent successfully
    expect(result.success).toBe(true);
    expect(result.messageId).toBeDefined();
    expect(result.message.content).toBe('Hello everyone! How are you doing?');
    expect(result.message.roomId).toBe('test-room-1');
    
    // Verify room notifications were sent via router
    const messages = router.getMessages();
    expect(messages.length).toBeGreaterThan(0);
    
    console.log('✅ Universal message sending test completed');
  });

  test('Room update notifications - Universal handlers', async () => {
    const roomUpdateCommand = new ChatRoomUpdateCommand(context, router);
    
    // Set up room update handler with participant context
    const testParticipant: SessionParticipant = {
      participantId: generateUUID(),
      sessionId: context.uuid,
      displayName: 'Test Handler',
      joinedAt: new Date().toISOString(),
      lastSeen: new Date().toISOString(),
      isOnline: true,
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: false, // Not an auto-responder in this test
        providesContext: false
      }
    };
    
    roomUpdateCommand.setParticipant(testParticipant);
    roomUpdateCommand.subscribeToRoom('test-room-1');
    
    // Create room update parameters (message sent)
    const updateParams = {
      context,
      sessionId: context.uuid,
      roomId: 'test-room-1',
      updateType: 'message-sent' as const,
      data: {
        message: {
          messageId: generateUUID(),
          roomId: 'test-room-1',
          senderId: generateUUID(),
          senderName: 'Alice',
          content: 'Hello from Alice!',
          timestamp: new Date().toISOString(),
          mentions: [],
          category: 'chat'
        } as ChatMessage
      },
      originNodeId: 'test-node-1'
    };
    
    // Execute room update
    const result = await roomUpdateCommand.execute(updateParams);
    
    // Verify update was processed
    expect(result.success).toBe(true);
    expect(result.processed).toBe(true);
    expect(result.roomId).toBe('test-room-1');
    expect(result.updateType).toBe('message-sent');
    
    console.log('✅ Universal room update test completed');
  });

  test('Cross-node participant coordination', async () => {
    const roomStats = roomCoordinator.getSystemStats();
    expect(roomStats.currentNode).toBe('test-node-1');
    expect(roomStats.totalRooms).toBe(0);
    
    // Add participants from multiple nodes
    const participant1: SessionParticipant = {
      participantId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Alice (Local)',
      joinedAt: new Date().toISOString(),
      lastSeen: new Date().toISOString(),
      isOnline: true,
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: false,
        providesContext: false
      }
    };
    
    const participant2: SessionParticipant = {
      participantId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Bob (Remote)',
      joinedAt: new Date().toISOString(),
      lastSeen: new Date().toISOString(),
      isOnline: true,
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: true,
        providesContext: true
      }
    };
    
    // Add to same room from different nodes
    roomCoordinator.addParticipantToRoom('distributed-room', participant1, 'test-node-1');
    roomCoordinator.addParticipantToRoom('distributed-room', participant2, 'remote-node-2');
    
    // Verify distributed room state
    const roomParticipants = roomCoordinator.getRoomParticipants('distributed-room');
    expect(roomParticipants.length).toBe(2);
    
    const participantsByNode = roomCoordinator.getParticipantsByNode('distributed-room');
    expect(Object.keys(participantsByNode).length).toBe(2); // Two different nodes
    expect(participantsByNode['test-node-1']).toBeDefined();
    expect(participantsByNode['remote-node-2']).toBeDefined();
    
    const finalStats = roomCoordinator.getSystemStats();
    expect(finalStats.totalRooms).toBe(1);
    expect(finalStats.totalParticipants).toBe(2);
    expect(finalStats.distributedRooms).toBe(1); // One room spans multiple nodes
    
    console.log('✅ Cross-node coordination test completed');
  });

  test('Universal response triggers - AI vs Human', async () => {
    // This test verifies that the same command interfaces work for both humans and AIs
    const roomUpdateCommand = new ChatRoomUpdateCommand(context, router);
    
    // Test human participant (no auto-response)
    const humanParticipant: SessionParticipant = {
      participantId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Human Alice',
      joinedAt: new Date().toISOString(),
      lastSeen: new Date().toISOString(),
      isOnline: true,
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: false, // Human - no auto-response
        providesContext: false
      }
    };
    
    roomUpdateCommand.setParticipant(humanParticipant);
    roomUpdateCommand.subscribeToRoom('test-room-1');
    
    // Send message update - human participant won't auto-respond
    const messageForHuman = {
      context,
      sessionId: context.uuid,
      roomId: 'test-room-1',
      updateType: 'message-sent' as const,
      data: {
        message: {
          messageId: generateUUID(),
          roomId: 'test-room-1',
          senderId: generateUUID(),
          senderName: 'Test Sender',
          content: 'Hello Alice, can you help me?',
          timestamp: new Date().toISOString(),
          mentions: [],
          category: 'chat'
        } as ChatMessage
      }
    };
    
    const humanResult = await roomUpdateCommand.execute(messageForHuman);
    expect(humanResult.success).toBe(true);
    
    // Now test AI participant (with auto-response)
    const aiParticipant: SessionParticipant = {
      participantId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'AI Bob',
      joinedAt: new Date().toISOString(),
      lastSeen: new Date().toISOString(),
      isOnline: true,
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: true, // AI - will auto-respond
        providesContext: true
      },
      adapter: {
        type: 'ai-api',
        config: { provider: 'openai', model: 'gpt-4' }
      }
    };
    
    const aiRoomUpdateCommand = new ChatRoomUpdateCommand(context, router);
    aiRoomUpdateCommand.setParticipant(aiParticipant);
    aiRoomUpdateCommand.subscribeToRoom('test-room-1');
    
    const messageForAI = {
      context,
      sessionId: context.uuid,
      roomId: 'test-room-1', 
      updateType: 'message-sent' as const,
      data: {
        message: {
          messageId: generateUUID(),
          roomId: 'test-room-1',
          senderId: generateUUID(),
          senderName: 'Test Sender',
          content: 'Hello AI Bob, what is 2 + 2?',
          timestamp: new Date().toISOString(),
          mentions: [],
          category: 'chat'
        } as ChatMessage
      }
    };
    
    const aiResult = await aiRoomUpdateCommand.execute(messageForAI);
    expect(aiResult.success).toBe(true);
    
    console.log('✅ Universal response triggers test completed');
  });
});