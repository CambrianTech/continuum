/**
 * Test User Manager - Encapsulates common test user patterns
 * 
 * Eliminates repetitive 'any' types and name issues by providing
 * a clean, typed interface for test user management.
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import type { JTAGClient } from '../../system/core/client/shared/JTAGClient';

export interface TestUser {
  client: JTAGClient;
  name: string;
  role: string;
  environment: 'server' | 'browser';
}

export interface TestUserConfig {
  name: string;
  role: string;
  environment: 'server' | 'browser';
  contextData?: Record<string, any>;
}

/**
 * Manages test users with proper types and no repetitive patterns
 */
export class TestUserManager {
  private users: Map<string, TestUser> = new Map();
  private serverUrl = 'ws://localhost:9001';

  /**
   * Connect a user with proper typing
   */
  async connect(config: TestUserConfig): Promise<TestUser> {
    console.log(`👤 Connecting ${config.name} (${config.environment})...`);
    
    const { client } = await JTAGClientServer.connect({
      targetEnvironment: config.environment === 'browser' ? 'server' : 'server', // Control via server for now
      transportType: 'websocket',
      serverUrl: this.serverUrl,
      context: {
        displayName: config.name,
        role: config.role,
        ...config.contextData
      }
    });

    const user: TestUser = {
      client,
      name: config.name,
      role: config.role,
      environment: config.environment
    };

    this.users.set(config.name, user);
    console.log(`✅ ${config.name} connected`);
    
    return user;
  }

  /**
   * Get a connected user
   */
  getUser(name: string): TestUser | undefined {
    return this.users.get(name);
  }

  /**
   * Get all connected users
   */
  getAllUsers(): TestUser[] {
    return Array.from(this.users.values());
  }

  /**
   * Send a message from a user
   */
  async sendMessage(userName: string, roomId: string, content: string): Promise<void> {
    const user = this.users.get(userName);
    if (!user) {
      throw new Error(`User ${userName} not found`);
    }

    await user.client.commands['chat/send-message']({
      roomId,
      content,
      messageContext: { role: user.role }
    });
    
    console.log(`📨 ${userName} sent message`);
  }

  /**
   * Disconnect all users
   */
  async disconnectAll(): Promise<void> {
    console.log('🔌 Disconnecting all users...');
    
    for (const [name, user] of this.users) {
      try {
        if ('disconnect' in user.client && typeof user.client.disconnect === 'function') {
          await user.client.disconnect();
          console.log(`🔌 ${name} disconnected`);
        }
      } catch (error) {
        console.log(`⚠️ Error disconnecting ${name}:`, error);
      }
    }
    
    this.users.clear();
  }

  /**
   * Standard test users - eliminates name/typing issues
   */
  static readonly STANDARD_USERS: TestUserConfig[] = [
    {
      name: 'Human',
      role: 'Human User',
      environment: 'server'
    },
    {
      name: 'AIAssistant', 
      role: 'AI Assistant',
      environment: 'server'
    },
    {
      name: 'DevAssistant',
      role: 'Development AI',
      environment: 'browser'
    }
  ];

  /**
   * Connect standard users for common test scenarios
   */
  async connectStandardUsers(): Promise<TestUser[]> {
    const users: TestUser[] = [];
    
    for (const config of TestUserManager.STANDARD_USERS) {
      const user = await this.connect(config);
      users.push(user);
    }
    
    return users;
  }

  /**
   * Execute a multi-user conversation scenario
   */
  async executeConversation(roomId: string, messages: Array<{ user: string; content: string }>): Promise<void> {
    console.log('💬 Executing conversation scenario...');
    
    for (const message of messages) {
      await this.sendMessage(message.user, roomId, message.content);
      await new Promise(resolve => setTimeout(resolve, 500)); // Small delay
    }
    
    console.log('💬 Conversation completed');
  }
}