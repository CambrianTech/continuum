/**
 * ChatService Unit Tests - Pure Logic Testing
 * 
 * Tests ChatService business logic in isolation using mocked transport.
 * Follows middle-out principle: unit tests first, then integration tests.
 * 
 * Covers:
 * - Message validation and sending
 * - Room creation and management  
 * - User operations and permissions
 * - Error handling and edge cases
 * - Caching behavior
 */

import { ChatService, type IChatService } from '../../chat/ChatService';
import type { IServiceTransport } from '../../shared/ServiceBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { SendMessageParams, CreateRoomParams, JoinRoomParams } from '../../../api/commands/chat/ChatCommands';
import type { BaseUser, HumanUser } from '../../../api/types/User';

console.log('🧪 ChatService Unit Tests');

// Test assertion helper
function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

// Mock transport for isolated testing
class MockServiceTransport implements IServiceTransport {
  private mockResponses = new Map<string, any>();
  private callLog: Array<{ command: string; params: any }> = [];
  
  setMockResponse(command: string, response: any): void {
    this.mockResponses.set(command, response);
  }
  
  async sendCommand<TParams, TResult>(
    command: string, 
    params: TParams, 
    context?: JTAGContext
  ): Promise<TResult> {
    this.callLog.push({ command, params });
    
    const response = this.mockResponses.get(command);
    if (!response) {
      return { success: false, error: `No mock response for ${command}` } as TResult;
    }
    
    return response as TResult;
  }
  
  subscribeToEvents(eventType: string, handler: (data: any) => void): void {
    // Mock implementation
  }
  
  unsubscribeFromEvents(eventType: string, handler: (data: any) => void): void {
    // Mock implementation  
  }
  
  getCallLog(): Array<{ command: string; params: any }> {
    return [...this.callLog];
  }
  
  clearCallLog(): void {
    this.callLog = [];
  }
}

// Mock context
const mockContext: JTAGContext = {
  sessionId: 'test-session-123',
  userId: 'test-user-456',
  environment: 'test'
} as JTAGContext;

// Test user data
const mockHumanUser: HumanUser = {
  id: 'user-123',
  name: 'Test User',
  userType: 'human',
  email: 'test@example.com',
  isAuthenticated: true,
  permissions: [
    { action: 'chat', resource: '*', granted: true },
    { action: 'send_messages', resource: '*', granted: true }
  ],
  capabilities: [
    { name: 'human_interaction', enabled: true }
  ],
  profile: {
    displayName: 'Test User',
    preferences: {}
  },
  createdAt: '2025-01-01T00:00:00Z',
  lastActiveAt: '2025-01-01T00:00:00Z'
};

/**
 * UNIT TEST 1: Message Validation
 * Tests input validation without external dependencies
 */
async function testMessageValidation(): Promise<void> {
  console.log('\n📝 Testing Message Validation...');
  
  const mockTransport = new MockServiceTransport();
  const chatService = new ChatService(mockTransport, mockContext);
  
  // Test empty message content
  try {
    await chatService.sendMessage({
      roomId: 'room-123',
      content: { text: '' },
      sender: mockHumanUser
    });
    assert(false, 'Should have thrown error for empty message');
  } catch (error) {
    assert(error.message.includes('Message content is required'), 'Correct validation error for empty message');
  }
  
  // Test missing room ID
  try {
    await chatService.sendMessage({
      roomId: '',
      content: { text: 'Hello world' },
      sender: mockHumanUser
    });
    assert(false, 'Should have thrown error for missing room ID');
  } catch (error) {
    assert(error.message.includes('Room ID is required'), 'Correct validation error for missing room ID');
  }
  
  // Test missing sender
  try {
    await chatService.sendMessage({
      roomId: 'room-123',
      content: { text: 'Hello world' },
      sender: null as any
    });
    assert(false, 'Should have thrown error for missing sender');
  } catch (error) {
    assert(error.message.includes('Sender information is required'), 'Correct validation error for missing sender');
  }
}

/**
 * UNIT TEST 2: Successful Message Sending  
 * Tests happy path with mocked transport response
 */
async function testSuccessfulMessageSending(): Promise<void> {
  console.log('\n💬 Testing Successful Message Sending...');
  
  const mockTransport = new MockServiceTransport();
  const chatService = new ChatService(mockTransport, mockContext);
  
  // Mock successful response
  mockTransport.setMockResponse('chat/send-message', {
    success: true,
    message: {
      id: 'msg-123',
      roomId: 'room-123',
      senderId: 'user-123',
      senderName: 'Test User',
      content: { text: 'Hello world' },
      timestamp: '2025-01-01T00:00:00Z',
      mentions: [],
      reactions: [],
      status: 'sent',
      metadata: {}
    }
  });
  
  const params: SendMessageParams = {
    roomId: 'room-123',
    content: { text: 'Hello world' },
    sender: mockHumanUser
  };
  
  const result = await chatService.sendMessage(params);
  
  assert(result.success, 'Message sending should succeed');
  assert(result.message?.content.text === 'Hello world', 'Message content should match');
  
  // Verify transport was called with correct parameters
  const callLog = mockTransport.getCallLog();
  assert(callLog.length === 1, 'Should have made one transport call');
  assert(callLog[0].command === 'chat/send-message', 'Should have called correct command');
  assert(callLog[0].params.roomId === 'room-123', 'Should have passed correct room ID');
}

/**
 * UNIT TEST 3: Room Creation Validation
 * Tests room creation business logic
 */
async function testRoomCreation(): Promise<void> {
  console.log('\n🏠 Testing Room Creation...');
  
  const mockTransport = new MockServiceTransport();
  const chatService = new ChatService(mockTransport, mockContext);
  
  // Test empty room name
  try {
    await chatService.createRoom({
      name: '',
      creator: mockHumanUser
    });
    assert(false, 'Should have thrown error for empty room name');
  } catch (error) {
    assert(error.message.includes('Room name is required'), 'Correct validation for empty room name');
  }
  
  // Test missing creator
  try {
    await chatService.createRoom({
      name: 'Test Room',
      creator: null as any
    });
    assert(false, 'Should have thrown error for missing creator');
  } catch (error) {
    assert(error.message.includes('Room creator is required'), 'Correct validation for missing creator');
  }
  
  // Test successful room creation
  mockTransport.setMockResponse('chat/create-room', {
    success: true,
    room: {
      id: 'room-456',
      name: 'Test Room',
      displayName: 'Test Room',
      description: 'A test room',
      isPrivate: false,
      creatorId: 'user-123',
      participants: [mockHumanUser],
      messageCount: 0,
      unreadCount: 0,
      createdAt: '2025-01-01T00:00:00Z',
      updatedAt: '2025-01-01T00:00:00Z',
      metadata: {}
    },
    roomId: 'room-456'
  });
  
  const result = await chatService.createRoom({
    name: 'Test Room',
    description: 'A test room',
    creator: mockHumanUser
  });
  
  assert(result.success, 'Room creation should succeed');
  assert(result.roomId === 'room-456', 'Should return correct room ID');
}

/**
 * UNIT TEST 4: Error Handling
 * Tests how service handles transport errors
 */
async function testErrorHandling(): Promise<void> {
  console.log('\n💥 Testing Error Handling...');
  
  const mockTransport = new MockServiceTransport();
  const chatService = new ChatService(mockTransport, mockContext);
  
  // Mock transport failure
  mockTransport.setMockResponse('chat/send-message', {
    success: false,
    error: 'Network timeout'
  });
  
  try {
    await chatService.sendMessage({
      roomId: 'room-123',
      content: { text: 'Hello world' },
      sender: mockHumanUser
    });
    assert(false, 'Should have thrown error for transport failure');
  } catch (error) {
    assert(error.message.includes('Network timeout'), 'Should propagate transport error');
  }
}

/**
 * UNIT TEST 5: Room Operations
 * Tests join/leave room functionality
 */
async function testRoomOperations(): Promise<void> {
  console.log('\n🚪 Testing Room Operations...');
  
  const mockTransport = new MockServiceTransport();
  const chatService = new ChatService(mockTransport, mockContext);
  
  // Test joining room
  mockTransport.setMockResponse('chat/join-room', {
    success: true,
    room: {
      id: 'room-123',
      name: 'Test Room',
      participants: [mockHumanUser]
    },
    joined: true
  });
  
  const joinResult = await chatService.joinRoom({
    roomId: 'room-123',
    user: mockHumanUser
  });
  
  assert(joinResult.success, 'Room join should succeed');
  assert(joinResult.joined, 'Should indicate user joined');
  
  // Test leaving room  
  mockTransport.setMockResponse('chat/leave-room', {
    success: true,
    left: true
  });
  
  const leaveResult = await chatService.leaveRoom('room-123', mockHumanUser);
  
  assert(leaveResult.success, 'Room leave should succeed');
  
  // Verify correct parameters were passed
  const callLog = mockTransport.getCallLog();
  const leaveCall = callLog.find(call => call.command === 'chat/leave-room');
  assert(leaveCall !== undefined, 'Should have called leave-room command');
  assert(leaveCall.params.roomId === 'room-123', 'Should pass correct room ID');
  assert(leaveCall.params.user.id === 'user-123', 'Should pass correct user');
}

/**
 * UNIT TEST 6: Room Listing
 * Tests room discovery functionality
 */
async function testRoomListing(): Promise<void> {
  console.log('\n📋 Testing Room Listing...');
  
  const mockTransport = new MockServiceTransport();
  const chatService = new ChatService(mockTransport, mockContext);
  
  mockTransport.setMockResponse('chat/list-rooms', {
    success: true,
    rooms: [
      {
        id: 'room-1',
        name: 'General',
        displayName: 'General Chat',
        description: 'Main chat room',
        participants: [mockHumanUser],
        messageCount: 42
      },
      {
        id: 'room-2', 
        name: 'Random',
        displayName: 'Random Discussion',
        description: 'Off-topic chat',
        participants: [mockHumanUser],
        messageCount: 15
      }
    ],
    totalCount: 2,
    hasMore: false
  });
  
  const result = await chatService.listRooms();
  
  assert(result.success, 'Room listing should succeed');
  assert(result.rooms.length === 2, 'Should return correct number of rooms');
  assert(result.rooms[0].name === 'General', 'Should return correct room data');
  assert(result.totalCount === 2, 'Should return correct total count');
}

/**
 * Run all unit tests
 */
async function runAllTests(): Promise<void> {
  console.log('🚀 Starting ChatService Unit Tests\n');
  
  try {
    await testMessageValidation();
    await testSuccessfulMessageSending();
    await testRoomCreation();
    await testErrorHandling();
    await testRoomOperations();
    await testRoomListing();
    
    console.log('\n🎉 All ChatService unit tests passed!');
    
  } catch (error) {
    console.error('\n💥 Unit test failed:', error.message);
    throw error;
  }
}

// Auto-run if this is the main module
if (require.main === module) {
  runAllTests().catch(error => {
    console.error('Test execution failed:', error);
    process.exit(1);
  });
}

export { runAllTests };