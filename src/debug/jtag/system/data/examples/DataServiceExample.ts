/**
 * DataService Usage Examples - Replace hardcoded data commands
 * 
 * Demonstrates how to replace the current data/create, data/read, data/list
 * commands with professional DataService.executeOperation calls
 */

import { DataServiceFactory, DataServiceMode } from '../services/DataServiceFactory';
import type { User, CreateUserData } from '../domains/User';
import type { ChatMessage, CreateMessageData } from '../domains/ChatMessage';
import type { BaseEntity } from '../domains/CoreTypes';
import { UserId, SessionId, RoomId, MessageId } from '../domains/CoreTypes';
import { generateUUID } from '../../core/types/CrossPlatformUUID';

/**
 * Example: Replace hardcoded data/create command with DataService.executeOperation
 */
export async function replaceDataCreateCommand() {
  // Before: DataCreateServerCommand with hardcoded file operations
  // After: Clean DataService operation
  
  const dataService = await DataServiceFactory.createHybridMigrating();

  // Create a user (replaces data/create with collection=users)
  const userData: CreateUserData = {
    displayName: 'Claude AI',
    type: 'ai',
    capabilities: {
      canSendMessages: true,
      canReceiveMessages: true,
      autoResponds: true,
      providesContext: true
    }
  };

  // Clean, type-safe operation (replaces hardcoded file operations)
  const userResult = await dataService.executeOperation<User>('users/create', userData);
  
  if (userResult.success) {
    console.log('✅ Created user:', userResult.data.profile.displayName);
    return userResult.data;
  } else {
    console.error('❌ Failed to create user:', userResult.error.message);
    throw new Error(userResult.error.message);
  }
}

/**
 * Example: Replace hardcoded data/list command with DataService.executeOperation
 */
export async function replaceDataListCommand() {
  // Before: DataListServerCommand with manual file system traversal
  // After: Clean DataService query with type safety
  
  const dataService = await DataServiceFactory.createHybridMigrating();

  // List all active users (replaces data/list with collection=users)
  const usersResult = await dataService.executeOperation<User>('users/list', {
    limit: 10,
    orderBy: [{ field: 'updatedAt', direction: 'DESC' }]
  });

  if (usersResult.success) {
    console.log(`✅ Found ${(usersResult.data as User[]).length} users`);
    return usersResult.data as User[];
  } else {
    console.error('❌ Failed to list users:', usersResult.error.message);
    return [];
  }
}

/**
 * Example: Replace hardcoded data/read command with DataService.executeOperation  
 */
export async function replaceDataReadCommand(userId: string) {
  // Before: DataReadServerCommand with manual file path construction
  // After: Clean DataService read with automatic path handling
  
  const dataService = await DataServiceFactory.createHybridMigrating();

  // Read specific user (replaces data/read with collection=users, id=userId)
  const userResult = await dataService.executeOperation<User>('users/read', { id: userId });

  if (userResult.success && userResult.data) {
    console.log('✅ Found user:', (userResult.data as User).profile.displayName);
    return userResult.data as User;
  } else {
    console.log(`❌ User ${userId} not found`);
    return null;
  }
}

/**
 * Example: Professional chat message operations (Discord-like features)
 */
export async function demonstrateChatOperations() {
  const dataService = await DataServiceFactory.createHybridMigrating();

  // Create a rich chat message with Discord-like features
  const messageData: CreateMessageData = {
    roomId: RoomId('general'),
    senderId: UserId('claude-ai'),
    content: {
      text: 'Hello! This message supports **markdown**, @mentions, #hashtags, and ```code blocks```',
      attachments: [],
      formatting: {
        markdown: true,
        mentions: [UserId('user-123')],
        hashtags: ['development', 'ai'],
        links: ['https://example.com'],
        codeBlocks: [{
          language: 'typescript',
          content: 'console.log("Hello World");'
        }]
      }
    },
    priority: 'normal',
    metadata: {
      source: 'ai',
      deviceType: 'web'
    }
  };

  // Create message with full type safety
  const messageResult = await dataService.executeOperation<ChatMessage>('chat_messages/create', messageData);
  
  if (messageResult.success) {
    const message = messageResult.data as ChatMessage;
    console.log('✅ Created message:', message.content.text.substring(0, 50) + '...');

    // Query messages in room with filtering and sorting
    const roomMessagesResult = await dataService.executeOperation<ChatMessage>('chat_messages/query', {
      filters: { roomId: 'general' },
      options: {
        limit: 20,
        orderBy: [{ field: 'timestamp', direction: 'DESC' }]
      }
    });

    if (roomMessagesResult.success) {
      console.log(`✅ Room has ${(roomMessagesResult.data as ChatMessage[]).length} messages`);
    }

    return message;
  } else {
    console.error('❌ Failed to create message:', messageResult.error.message);
    throw new Error(messageResult.error.message);
  }
}

/**
 * Example: ORM-like convenience methods vs executeOperation
 */
export async function demonstrateORMLikeOperations() {
  const dataService = await DataServiceFactory.createHybridMigrating();

  // Method 1: executeOperation (widget.executeCommand style)
  const createResult = await dataService.executeOperation<User>('users/create', {
    displayName: 'Test User',
    type: 'human'
  });

  // Method 2: ORM-like convenience methods (same backend, cleaner syntax)
  const user = await dataService.create<User>('users', {
    displayName: 'Test User 2',  
    type: 'human'
  });

  const users = await dataService.list<User>('users', {
    limit: 10,
    orderBy: [{ field: 'createdAt', direction: 'DESC' }]
  });

  const foundUser = await dataService.read<User>('users', createResult.success ? (createResult.data as User).id : '');

  console.log('✅ Both approaches work - use executeOperation for widget.executeCommand consistency');
  console.log('✅ Use ORM methods for cleaner code when not matching widget patterns');

  return { createResult, user, users, foundUser };
}

/**
 * Example: Migration from existing JSON files
 */
export async function demonstrateJSONMigration() {
  // This DataService automatically reads from existing JSON files
  // while writing new data to SQLite for performance
  const dataService = await DataServiceFactory.createHybridMigrating(
    '.continuum/database',           // Existing JSON files
    '.continuum/database/new.db'     // New SQLite database
  );

  console.log('✅ DataService will:');
  console.log('  - Read from existing JSON files (.continuum/database/*.json)');
  console.log('  - Write new data to SQLite (.continuum/database/new.db)');  
  console.log('  - Automatically migrate JSON data to SQLite on read');
  console.log('  - Provide unified access to both data sources');

  // This query will find data in BOTH JSON files and SQLite
  const allUsers = await dataService.list<User>('users');
  
  if (allUsers.success) {
    console.log(`✅ Found ${(allUsers.data as User[]).length} users across all storage backends`);
  }

  return dataService;
}

/**
 * Example: Production-ready configuration
 */
export async function demonstrateProductionConfig() {
  // For development: Use JSON files for simplicity
  const devDataService = await DataServiceFactory.create({
    mode: DataServiceMode.JSON_ONLY,
    paths: { jsonDatabase: '.continuum/database' },
    context: { source: 'development' }
  });

  // For production: Use SQLite for performance and features  
  const prodDataService = await DataServiceFactory.create({
    mode: DataServiceMode.SQLITE_ONLY,
    paths: { sqliteDatabase: '.continuum/database/production.db' },
    context: { source: 'production' }
  });

  // Same API, different backends - zero code changes needed
  const devUsers = await devDataService.list<User>('users');
  const prodUsers = await prodDataService.list<User>('users'); 

  console.log('✅ Same code works with different backends');
  console.log('✅ Easy to switch from development to production configuration');

  return { devDataService, prodDataService };
}

/**
 * Complete example replacing a hardcoded command
 */
export class ModernDataCreateCommand {
  private dataService: ReturnType<typeof DataServiceFactory.createHybridMigrating>;

  constructor() {
    this.dataService = DataServiceFactory.createHybridMigrating();
  }

  async execute(params: { collection: string; data: any; id?: string }) {
    const dataService = await this.dataService;
    
    // Before: 40+ lines of hardcoded file operations in DataCreateServerCommand
    // After: 1 line with full type safety and multi-backend support
    const result = await dataService.executeOperation<BaseEntity>(
      `${params.collection}/create`, 
      params.data
    );

    if (result.success) {
      console.log(`✅ Created ${params.collection} record:`, (result.data as BaseEntity).id);
      return { success: true, id: (result.data as BaseEntity).id };
    } else {
      console.error(`❌ Failed to create ${params.collection}:`, result.error.message);
      return { success: false, error: result.error.message };
    }
  }
}

// Export all examples for easy testing
export {
  DataServiceFactory,
  DataServiceMode
};