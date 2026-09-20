/**
 * UserService Unit Tests - User Management Logic Testing
 * 
 * Tests UserService business logic in isolation using mocked transport.
 * Covers user authentication, caching, permissions, and type safety.
 * 
 * Key areas:
 * - User authentication and session management
 * - User creation with proper type hierarchy (BaseUser, HumanUser, etc.)  
 * - Caching behavior and performance optimization
 * - Permission checking and capability management
 * - Error handling and validation
 */

import { UserService, type IUserService } from '../../user/UserService';
import type { IServiceTransport } from '../../shared/ServiceBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { BaseUser, HumanUser, UserType } from '../../../api/types/User';
import { createHumanUser, isHumanUser } from '../../../api/types/User';

console.log('🧪 UserService Unit Tests');

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
  
  subscribeToEvents(): void {}
  unsubscribeFromEvents(): void {}
  
  getCallLog(): Array<{ command: string; params: any }> {
    return [...this.callLog];
  }
  
  clearCallLog(): void {
    this.callLog = [];
  }
}

// Mock context
const mockContext: JTAGContext = {
  sessionId: 'test-session-789',
  userId: 'test-user-456',
  environment: 'test'
} as JTAGContext;

// Test user data
const mockHumanUser: HumanUser = createHumanUser({
  name: 'Alice Developer',
  email: 'alice@continuum.dev',
  avatar: 'https://avatars.example.com/alice',
  displayName: 'Alice D.',
  preferences: { theme: 'dark', notifications: true }
});

/**
 * UNIT TEST 1: User Authentication
 * Tests authentication flow with proper type checking
 */
async function testUserAuthentication(): Promise<void> {
  console.log('\n🔐 Testing User Authentication...');
  
  const mockTransport = new MockServiceTransport();
  const userService = new UserService(mockTransport, mockContext);
  
  // Mock successful authentication
  mockTransport.setMockResponse('user/authenticate', {
    success: true,
    user: mockHumanUser
  });
  
  const result = await userService.authenticateUser({
    email: 'alice@continuum.dev',
    password: 'secure-password-123'
  });
  
  assert(result !== null, 'Authentication should return user');
  assert(result!.userType === 'human', 'Should return HumanUser type');
  assert(result!.email === 'alice@continuum.dev', 'Should return correct email');
  assert(result!.name === 'Alice Developer', 'Should return correct name');
  
  // Verify transport call
  const callLog = mockTransport.getCallLog();
  assert(callLog.length === 1, 'Should make one transport call');
  assert(callLog[0].command === 'user/authenticate', 'Should call authenticate command');
  assert(callLog[0].params.email === 'alice@continuum.dev', 'Should pass email');
  assert(callLog[0].params.password === 'secure-password-123', 'Should pass password');
}

/**
 * UNIT TEST 2: Current User Caching
 * Tests caching behavior to avoid redundant transport calls
 */
async function testCurrentUserCaching(): Promise<void> {
  console.log('\n💾 Testing Current User Caching...');
  
  const mockTransport = new MockServiceTransport();
  const userService = new UserService(mockTransport, mockContext);
  
  // Mock current user response
  mockTransport.setMockResponse('user/get-current', {
    success: true,
    user: mockHumanUser
  });
  
  // First call should hit transport
  const user1 = await userService.getCurrentUser();
  assert(user1 !== null, 'First call should return user');
  assert(user1!.id === mockHumanUser.id, 'Should return correct user ID');
  
  mockTransport.clearCallLog();
  
  // Second call should use cache, not transport
  const user2 = await userService.getCurrentUser();
  assert(user2 !== null, 'Second call should return cached user');
  assert(user2!.id === mockHumanUser.id, 'Should return same user from cache');
  
  const callLog = mockTransport.getCallLog();
  assert(callLog.length === 0, 'Second call should not hit transport (cached)');
  
  // Both references should be the same object (cached)
  assert(user1 === user2, 'Should return same cached object instance');
}

/**
 * UNIT TEST 3: User Creation with Type Safety
 * Tests user factory functions and type hierarchy
 */
async function testUserCreation(): Promise<void> {
  console.log('\n👤 Testing User Creation...');
  
  const mockTransport = new MockServiceTransport();
  const userService = new UserService(mockTransport, mockContext);
  
  // Mock successful user creation
  mockTransport.setMockResponse('user/create', {
    success: true,
    user: mockHumanUser
  });
  
  const result = await userService.createHumanUser({
    name: 'Bob Smith',
    email: 'bob@example.com',
    avatar: 'https://avatars.example.com/bob'
  });
  
  assert(result.userType === 'human', 'Should create HumanUser type');
  assert(result.name === 'Bob Smith', 'Should have correct name');
  assert(result.email === 'bob@example.com', 'Should have correct email');
  assert(result.permissions.length > 0, 'Should have default permissions');
  assert(result.capabilities.length > 0, 'Should have default capabilities');
  
  // Verify transport was called with user object
  const callLog = mockTransport.getCallLog();
  assert(callLog.length === 1, 'Should make one transport call');
  assert(callLog[0].command === 'user/create', 'Should call create command');
  assert(callLog[0].params.user.userType === 'human', 'Should pass HumanUser type');
}

/**
 * UNIT TEST 4: Permission Checking Logic
 * Tests permission and capability checking without external dependencies
 */
async function testPermissionChecking(): Promise<void> {
  console.log('\n🛡️ Testing Permission Checking...');
  
  const mockTransport = new MockServiceTransport();
  const userService = new UserService(mockTransport, mockContext);
  
  // Create user with specific permissions
  const testUser: BaseUser = {
    ...mockHumanUser,
    permissions: [
      { action: 'chat', resource: '*', granted: true },
      { action: 'read_messages', resource: 'room-123', granted: true },
      { action: 'delete_messages', resource: '*', granted: false }
    ],
    capabilities: [
      { name: 'human_interaction', enabled: true },
      { name: 'admin_access', enabled: false },
      { name: 'debugging', enabled: true }
    ]
  };
  
  // Test granted permissions
  assert(
    userService.checkUserPermission(testUser, 'chat', 'any-resource'),
    'Should allow chat with wildcard permission'
  );
  
  assert(
    userService.checkUserPermission(testUser, 'read_messages', 'room-123'),
    'Should allow read_messages for specific room'
  );
  
  // Test denied permissions
  assert(
    !userService.checkUserPermission(testUser, 'delete_messages', 'room-123'),
    'Should deny delete_messages (explicitly denied)'
  );
  
  assert(
    !userService.checkUserPermission(testUser, 'admin_actions', 'anything'),
    'Should deny unknown permission'
  );
  
  // Test capabilities
  const capabilities = userService.getUserCapabilities(testUser);
  assert(capabilities.includes('human_interaction'), 'Should include enabled capabilities');
  assert(capabilities.includes('debugging'), 'Should include debugging capability');
  assert(!capabilities.includes('admin_access'), 'Should exclude disabled capabilities');
}

/**
 * UNIT TEST 5: User Search and Listing
 * Tests user discovery functionality
 */
async function testUserSearchAndListing(): Promise<void> {
  console.log('\n🔍 Testing User Search and Listing...');
  
  const mockTransport = new MockServiceTransport();
  const userService = new UserService(mockTransport, mockContext);
  
  const mockUsers: BaseUser[] = [
    createHumanUser({ name: 'Alice Developer', email: 'alice@continuum.dev' }),
    createHumanUser({ name: 'Bob Designer', email: 'bob@continuum.dev' }),
    createHumanUser({ name: 'Charlie Manager', email: 'charlie@continuum.dev' })
  ];
  
  // Mock list users response
  mockTransport.setMockResponse('user/list', {
    success: true,
    users: mockUsers
  });
  
  const allUsers = await userService.listUsers();
  assert(allUsers.length === 3, 'Should return all users');
  assert(allUsers[0].name === 'Alice Developer', 'Should return correct user data');
  
  // Mock list users by type
  mockTransport.setMockResponse('user/list', {
    success: true,
    users: mockUsers.filter(u => u.userType === 'human')
  });
  
  const humanUsers = await userService.listUsers('human');
  assert(humanUsers.length === 3, 'Should return human users only');
  assert(humanUsers.every(u => u.userType === 'human'), 'All returned users should be human type');
  
  // Mock search response
  mockTransport.setMockResponse('user/search', {
    success: true,
    users: mockUsers.filter(u => u.name.includes('Alice'))
  });
  
  const searchResults = await userService.searchUsers('Alice');
  assert(searchResults.length === 1, 'Should return matching search results');
  assert(searchResults[0].name === 'Alice Developer', 'Should return correct search result');
}

/**
 * UNIT TEST 6: User Cache Management
 * Tests user caching across different operations
 */
async function testUserCacheManagement(): Promise<void> {
  console.log('\n🗄️ Testing User Cache Management...');
  
  const mockTransport = new MockServiceTransport();
  const userService = new UserService(mockTransport, mockContext);
  
  const testUserId = 'user-cache-test-123';
  const testUser: BaseUser = {
    ...mockHumanUser,
    id: testUserId,
    name: 'Cache Test User'
  };
  
  // Mock user fetch response
  mockTransport.setMockResponse('user/get', {
    success: true,
    user: testUser
  });
  
  // First fetch should hit transport
  const user1 = await userService.getUserById(testUserId);
  assert(user1 !== null, 'Should fetch user from transport');
  assert(user1!.name === 'Cache Test User', 'Should return correct user');
  
  const firstCallCount = mockTransport.getCallLog().length;
  assert(firstCallCount === 1, 'Should make one transport call');
  
  // Second fetch should use cache
  const user2 = await userService.getUserById(testUserId);
  assert(user2 !== null, 'Should return cached user');
  assert(user2 === user1, 'Should return same cached object');
  
  const secondCallCount = mockTransport.getCallLog().length;
  assert(secondCallCount === 1, 'Should not make additional transport calls (cached)');
}

/**
 * UNIT TEST 7: Error Handling
 * Tests service error handling and fallback behavior
 */
async function testErrorHandling(): Promise<void> {
  console.log('\n💥 Testing Error Handling...');
  
  const mockTransport = new MockServiceTransport();
  const userService = new UserService(mockTransport, mockContext);
  
  // Test authentication failure
  mockTransport.setMockResponse('user/authenticate', {
    success: false,
    error: 'Invalid credentials'
  });
  
  const authResult = await userService.authenticateUser({
    email: 'wrong@email.com',
    password: 'wrong-password'
  });
  
  assert(authResult === null, 'Should return null for authentication failure');
  
  // Test user creation failure
  mockTransport.setMockResponse('user/create', {
    success: false,
    error: 'Email already exists'
  });
  
  try {
    await userService.createHumanUser({
      name: 'Duplicate User',
      email: 'existing@email.com'
    });
    assert(false, 'Should throw error for creation failure');
  } catch (error) {
    assert(error.message.includes('Email already exists'), 'Should propagate creation error');
  }
  
  // Test getCurrentUser graceful failure
  mockTransport.setMockResponse('user/get-current', {
    success: false,
    error: 'Session expired'
  });
  
  const currentUser = await userService.getCurrentUser();
  assert(currentUser === null, 'Should return null for session failure (graceful)');
}

/**
 * Run all UserService unit tests
 */
async function runAllTests(): Promise<void> {
  console.log('🚀 Starting UserService Unit Tests\n');
  
  try {
    await testUserAuthentication();
    await testCurrentUserCaching();
    await testUserCreation();
    await testPermissionChecking();
    await testUserSearchAndListing();
    await testUserCacheManagement();
    await testErrorHandling();
    
    console.log('\n🎉 All UserService unit tests passed!');
    
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