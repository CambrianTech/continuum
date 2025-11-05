#!/usr/bin/env tsx
/**
 * Database Seeding - Modular Data Population
 *
 * Provides consistent test data for both JSON and SQLite backends
 * Supports realistic Discord-like chat data: users, rooms, messages, participations
 */

import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';

// Seeding data structures
export interface SeedUser {
  id: string;
  name: string;
  displayName: string;
  userType: 'human' | 'agent' | 'persona';
  email?: string;
  isOnline: boolean;
  createdAt: string;
  lastActiveAt: string;
  avatar?: string;
  preferences?: {
    theme?: string;
    notifications?: boolean;
    autoComplete?: boolean;
  };
  capabilities?: string[];
  metadata?: Record<string, any>;
}

export interface SeedRoom {
  id: string;
  name: string;
  displayName: string;
  type: 'public' | 'private' | 'direct';
  description?: string;
  createdAt: string;
  lastActivity: string;
  memberCount: number;
  isArchived: boolean;
  metadata?: Record<string, any>;
}

export interface SeedMessage {
  id: string;
  roomId: string;
  senderId: string;
  content: string;
  type: 'text' | 'system' | 'action';
  timestamp: string;
  editedAt?: string;
  reactions?: Record<string, string[]>;
  threadId?: string;
  metadata?: Record<string, any>;
}

export interface SeedParticipation {
  id: string;
  userId: string;
  roomId: string;
  role: 'member' | 'admin' | 'moderator';
  joinedAt: string;
  lastReadAt: string;
  isActive: boolean;
  notifications: boolean;
}

/**
 * Database Seeder - Creates realistic test data
 */
export class DatabaseSeeder {
  private baseTimestamp: string;
  private userIds: string[] = [];
  private roomIds: string[] = [];
  private messageIds: string[] = [];

  constructor() {
    this.baseTimestamp = new Date().toISOString();
  }

  /**
   * Generate realistic user data (humans, agents, personas)
   */
  generateUsers(count: number = 10): SeedUser[] {
    const users: SeedUser[] = [];

    // Always include key users for consistent testing
    const keyUsers = [
      {
        name: 'Joel',
        displayName: 'Joel - Creator',
        userType: 'human' as const,
        email: 'joel@continuum.dev',
        avatar: '👨‍💻',
        preferences: { theme: 'dark', notifications: true, autoComplete: true },
        metadata: { role: 'creator', expertise: ['system-architecture', 'ai-integration'] }
      },
      {
        name: 'Claude Code',
        displayName: 'Claude Code Agent',
        userType: 'agent' as const,
        capabilities: ['code-generation', 'debugging', 'architecture', 'testing'],
        avatar: '🤖',
        metadata: { model: 'claude-sonnet-4', provider: 'anthropic', version: 'latest' }
      },
      {
        name: 'Assistant Alpha',
        displayName: 'Alpha Persona',
        userType: 'persona' as const,
        email: 'alpha@continuum.ai',
        capabilities: ['natural-language', 'task-planning', 'user-assistance'],
        avatar: '🎭',
        metadata: { specialization: 'user-onboarding', training: 'customer-support' }
      }
    ];

    // Add key users
    keyUsers.forEach((userData, index) => {
      const user: SeedUser = {
        id: generateUUID(),
        name: userData.name,
        displayName: userData.displayName,
        userType: userData.userType,
        email: userData.email,
        isOnline: Math.random() > 0.3, // 70% online rate
        createdAt: new Date(Date.now() - (30 - index) * 24 * 60 * 60 * 1000).toISOString(),
        lastActiveAt: new Date(Date.now() - Math.random() * 3600000).toISOString(),
        avatar: userData.avatar,
        preferences: userData.preferences,
        capabilities: userData.capabilities,
        metadata: userData.metadata
      };
      users.push(user);
      this.userIds.push(user.id);
    });

    // Generate additional users
    const userTypes: ('human' | 'agent' | 'persona')[] = ['human', 'human', 'agent', 'persona'];
    const nameTemplates = {
      human: ['Alice', 'Bob', 'Charlie', 'Diana', 'Eve', 'Frank', 'Grace'],
      agent: ['DataBot', 'CodeAssist', 'QueryEngine', 'AnalyzerAI', 'SearchBot'],
      persona: ['Helper', 'Guide', 'Advisor', 'Mentor', 'Coach']
    };

    for (let i = keyUsers.length; i < count; i++) {
      const userType = userTypes[i % userTypes.length];
      const names = nameTemplates[userType];
      const baseName = names[i % names.length];

      const user: SeedUser = {
        id: generateUUID(),
        name: `${baseName} ${i + 1}`,
        displayName: `${baseName} ${userType === 'human' ? 'User' : userType === 'agent' ? 'Agent' : 'Persona'} ${i + 1}`,
        userType,
        email: userType !== 'agent' ? `${baseName.toLowerCase()}${i + 1}@test.dev` : undefined,
        isOnline: Math.random() > 0.3,
        createdAt: new Date(Date.now() - Math.random() * 30 * 24 * 60 * 60 * 1000).toISOString(),
        lastActiveAt: new Date(Date.now() - Math.random() * 3600000).toISOString(),
        preferences: userType === 'human' ? {
          theme: Math.random() > 0.5 ? 'dark' : 'light',
          notifications: Math.random() > 0.2,
          autoComplete: Math.random() > 0.3
        } : undefined,
        capabilities: userType !== 'human' ? [
          'natural-language',
          userType === 'agent' ? 'code-analysis' : 'user-assistance',
          'data-processing'
        ] : undefined,
        metadata: {
          testUser: true,
          generatedAt: this.baseTimestamp
        }
      };

      users.push(user);
      this.userIds.push(user.id);
    }

    return users;
  }

  /**
   * Generate realistic room data (public, private, direct messages)
   */
  generateRooms(count: number = 5): SeedRoom[] {
    const rooms: SeedRoom[] = [];

    // Always include key rooms for consistent testing
    const keyRooms = [
      {
        name: 'general',
        displayName: 'General Discussion',
        type: 'public' as const,
        description: 'Main discussion room for all members',
        memberCount: Math.min(this.userIds.length, 8)
      },
      {
        name: 'academy',
        displayName: 'AI Academy',
        type: 'private' as const,
        description: 'Private training and development discussions',
        memberCount: Math.min(this.userIds.length, 5)
      },
      {
        name: 'development',
        displayName: 'Development Chat',
        type: 'public' as const,
        description: 'Technical development discussions',
        memberCount: Math.min(this.userIds.length, 6)
      }
    ];

    // Add key rooms
    keyRooms.forEach((roomData, index) => {
      const room: SeedRoom = {
        id: generateUUID(),
        name: roomData.name,
        displayName: roomData.displayName,
        type: roomData.type,
        description: roomData.description,
        createdAt: new Date(Date.now() - (20 - index) * 24 * 60 * 60 * 1000).toISOString(),
        lastActivity: new Date(Date.now() - Math.random() * 3600000).toISOString(),
        memberCount: roomData.memberCount,
        isArchived: false,
        metadata: {
          purpose: roomData.name === 'general' ? 'main-chat' :
                  roomData.name === 'academy' ? 'training' : 'development',
          createdBy: this.userIds[0] // Joel as creator
        }
      };
      rooms.push(room);
      this.roomIds.push(room.id);
    });

    // Generate additional rooms
    const roomTypes: ('public' | 'private' | 'direct')[] = ['public', 'private', 'direct'];
    const roomTemplates = ['Random', 'Support', 'Gaming', 'Music', 'Books', 'Tech'];

    for (let i = keyRooms.length; i < count; i++) {
      const type = roomTypes[i % roomTypes.length];
      const template = roomTemplates[i % roomTemplates.length];

      const room: SeedRoom = {
        id: generateUUID(),
        name: `${template.toLowerCase()}-${i + 1}`,
        displayName: `${template} Room ${i + 1}`,
        type,
        description: `${type === 'public' ? 'Open' : type === 'private' ? 'Private' : 'Direct'} ${template.toLowerCase()} discussion`,
        createdAt: new Date(Date.now() - Math.random() * 15 * 24 * 60 * 60 * 1000).toISOString(),
        lastActivity: new Date(Date.now() - Math.random() * 3600000).toISOString(),
        memberCount: type === 'direct' ? 2 : Math.floor(Math.random() * this.userIds.length) + 1,
        isArchived: Math.random() > 0.9, // 10% archived
        metadata: {
          testRoom: true,
          generatedAt: this.baseTimestamp
        }
      };

      rooms.push(room);
      this.roomIds.push(room.id);
    }

    return rooms;
  }

  /**
   * Generate realistic message data with conversation flow
   */
  generateMessages(count: number = 50): SeedMessage[] {
    const messages: SeedMessage[] = [];

    if (this.roomIds.length === 0 || this.userIds.length === 0) {
      throw new Error('Must generate users and rooms before generating messages');
    }

    // Message templates for realistic conversations
    const messageTemplates = {
      greeting: ['Hello everyone!', 'Good morning!', 'Hey there!', 'Hi all!'],
      question: ['How is everyone doing?', 'Anyone working on something interesting?', 'What do you think about this?'],
      answer: ['That sounds great!', 'I agree with that approach', 'Interesting perspective', 'Thanks for sharing!'],
      system: ['User joined the room', 'Room settings updated', 'New announcement posted'],
      technical: ['Just pushed the latest changes', 'Running tests now', 'Database performance looks good', 'Deploy completed successfully']
    };

    const messageTypes = Object.keys(messageTemplates) as (keyof typeof messageTemplates)[];

    for (let i = 0; i < count; i++) {
      const roomId = this.roomIds[i % this.roomIds.length];
      const senderId = this.userIds[i % this.userIds.length];
      const messageType = messageTypes[i % messageTypes.length];
      const isSystemMessage = messageType === 'system';

      const templates = messageTemplates[messageType];
      const content = templates[i % templates.length];

      const message: SeedMessage = {
        id: generateUUID(),
        roomId,
        senderId: isSystemMessage ? 'system' : senderId,
        content: `${content} (msg ${i + 1})`,
        type: isSystemMessage ? 'system' : 'text',
        timestamp: new Date(Date.now() - (count - i) * 60000 + Math.random() * 30000).toISOString(),
        editedAt: Math.random() > 0.9 ? new Date(Date.now() - Math.random() * 3600000).toISOString() : undefined,
        reactions: Math.random() > 0.8 ? {
          '👍': [this.userIds[0]],
          '❤️': Math.random() > 0.5 ? [this.userIds[1]] : []
        } : undefined,
        threadId: Math.random() > 0.95 ? generateUUID() : undefined,
        metadata: {
          testMessage: true,
          generatedAt: this.baseTimestamp,
          messageType
        }
      };

      messages.push(message);
      this.messageIds.push(message.id);
    }

    return messages;
  }

  /**
   * Generate room participation data (user-room relationships)
   */
  generateParticipations(): SeedParticipation[] {
    const participations: SeedParticipation[] = [];

    if (this.roomIds.length === 0 || this.userIds.length === 0) {
      throw new Error('Must generate users and rooms before generating participations');
    }

    // Ensure every user is in at least one room
    this.userIds.forEach((userId, userIndex) => {
      // Join the general room (everyone should be here)
      const generalRoom = this.roomIds[0]; // Assuming first room is general

      const participation: SeedParticipation = {
        id: generateUUID(),
        userId,
        roomId: generalRoom,
        role: userIndex === 0 ? 'admin' : userIndex <= 2 ? 'moderator' : 'member', // Joel admin, first few users moderators
        joinedAt: new Date(Date.now() - Math.random() * 20 * 24 * 60 * 60 * 1000).toISOString(),
        lastReadAt: new Date(Date.now() - Math.random() * 3600000).toISOString(),
        isActive: Math.random() > 0.1, // 90% active
        notifications: Math.random() > 0.2 // 80% have notifications on
      };

      participations.push(participation);

      // Join additional random rooms
      const additionalRooms = Math.floor(Math.random() * (this.roomIds.length - 1)) + 1;
      const shuffledRooms = [...this.roomIds].sort(() => 0.5 - Math.random()).slice(1, additionalRooms + 1);

      shuffledRooms.forEach(roomId => {
        const additionalParticipation: SeedParticipation = {
          id: generateUUID(),
          userId,
          roomId,
          role: Math.random() > 0.9 ? 'moderator' : 'member', // 10% chance of moderator
          joinedAt: new Date(Date.now() - Math.random() * 15 * 24 * 60 * 60 * 1000).toISOString(),
          lastReadAt: new Date(Date.now() - Math.random() * 3600000).toISOString(),
          isActive: Math.random() > 0.2, // 80% active in additional rooms
          notifications: Math.random() > 0.3 // 70% notifications for additional rooms
        };

        participations.push(additionalParticipation);
      });
    });

    return participations;
  }

  /**
   * Generate complete dataset with all related entities
   */
  generateCompleteDataset(options: {
    userCount?: number;
    roomCount?: number;
    messageCount?: number;
  } = {}): {
    users: SeedUser[];
    rooms: SeedRoom[];
    messages: SeedMessage[];
    participations: SeedParticipation[];
    summary: {
      totalRecords: number;
      collections: Record<string, number>;
      relationships: number;
      dataSize: string;
    };
  } {
    const { userCount = 10, roomCount = 5, messageCount = 50 } = options;

    console.log('🌱 Generating complete test dataset...');
    console.log(`   Users: ${userCount}, Rooms: ${roomCount}, Messages: ${messageCount}`);

    const users = this.generateUsers(userCount);
    const rooms = this.generateRooms(roomCount);
    const messages = this.generateMessages(messageCount);
    const participations = this.generateParticipations();

    const totalRecords = users.length + rooms.length + messages.length + participations.length;
    const estimatedSize = JSON.stringify({ users, rooms, messages, participations }).length;

    const summary = {
      totalRecords,
      collections: {
        users: users.length,
        rooms: rooms.length,
        messages: messages.length,
        participations: participations.length
      },
      relationships: participations.length,
      dataSize: `${(estimatedSize / 1024).toFixed(1)}KB`
    };

    console.log('✅ Dataset generated successfully!');
    console.log(`📊 Total records: ${totalRecords} across ${Object.keys(summary.collections).length} collections`);
    console.log(`💾 Estimated size: ${summary.dataSize}`);

    return { users, rooms, messages, participations, summary };
  }

  /**
   * Get generated IDs for relationship testing
   */
  getGeneratedIds(): {
    userIds: string[];
    roomIds: string[];
    messageIds: string[];
  } {
    return {
      userIds: [...this.userIds],
      roomIds: [...this.roomIds],
      messageIds: [...this.messageIds]
    };
  }
}

/**
 * Quick seeding helper functions
 */
export function createQuickTestData(size: 'small' | 'medium' | 'large' = 'medium') {
  const seeder = new DatabaseSeeder();

  const configs = {
    small: { userCount: 5, roomCount: 3, messageCount: 20 },
    medium: { userCount: 10, roomCount: 5, messageCount: 50 },
    large: { userCount: 25, roomCount: 10, messageCount: 200 }
  };

  return seeder.generateCompleteDataset(configs[size]);
}

/**
 * Discord-scale test data for performance testing
 */
export function createDiscordScaleData() {
  const seeder = new DatabaseSeeder();

  return seeder.generateCompleteDataset({
    userCount: 100,    // Small server
    roomCount: 20,     // Multiple channels
    messageCount: 1000 // Active chat history
  });
}

// Export singleton seeder for convenience
export const defaultSeeder = new DatabaseSeeder();