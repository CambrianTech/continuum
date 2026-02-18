/**
 * Seed Data Export
 *
 * Generated via: dataService.exportAll(['users', 'rooms', 'chat_messages'])
 * This file can be regenerated anytime by exporting current entities
 */

import type { User } from '../../system/data/domains/User';
import type { ChatRoom } from '../../system/data/domains/ChatRoom';
import type { ChatMessage } from '../../system/data/domains/ChatMessage';

export interface SeedData {
  users: Omit<User, keyof import('../../system/data/domains/CoreTypes').BaseEntity>[];
  rooms: Omit<ChatRoom, keyof import('../../system/data/domains/CoreTypes').BaseEntity>[];
  chat_messages: Omit<ChatMessage, keyof import('../../system/data/domains/CoreTypes').BaseEntity>[];
  exportedAt: string;
}

// This data can be exported from live entities using dataService.exportAll()
export const seedData: SeedData = {
  users: [
    {
      displayName: 'Joel',
      type: 'human',
      profile: {
        displayName: 'Joel',
        avatar: '👨‍💻',
        bio: 'System architect and developer',
        location: 'San Francisco',
        joinedAt: '2025-01-15T10:00:00.000Z'
      },
      capabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: true,
        canInviteOthers: true,
        canModerate: true,
        autoResponds: false,
        providesContext: false,
        canTrain: false,
        canAccessPersonas: true
      },
      preferences: {
        theme: 'dark',
        language: 'en',
        timezone: 'America/Los_Angeles',
        notifications: {
          mentions: true,
          directMessages: true,
          roomUpdates: true
        },
        privacy: {
          showOnlineStatus: true,
          allowDirectMessages: true,
          shareActivity: true
        }
      },
      status: 'online',
      lastActiveAt: '2025-01-15T10:00:00.000Z',
      sessionsActive: []
    } as any,
    {
      displayName: 'Claude Code',
      type: 'ai',
      profile: {
        displayName: 'Claude Code',
        avatar: '🤖',
        bio: 'AI coding assistant powered by Claude',
        location: 'The Cloud',
        joinedAt: '2025-01-15T10:01:00.000Z'
      },
      status: 'online',
      lastActiveAt: '2025-01-15T10:01:00.000Z',
      sessionsActive: []
    } as any
  ],
  rooms: [
    {
      name: 'general',
      displayName: 'General Discussion',
      description: 'Main chat room for general conversations',
      topic: 'Welcome to the general discussion room!',
      type: 'public',
      status: 'active',
      privacy: {
        isPublic: true,
        requiresInvite: false,
        allowGuestAccess: true,
        searchable: true
      },
      settings: {
        allowReactions: true,
        allowThreads: true,
        allowFileSharing: true,
        messageRetentionDays: 365
      },
      stats: {
        memberCount: 2,
        messageCount: 4,
        createdAt: '2025-01-15T10:00:00.000Z',
        lastActivityAt: '2025-01-15T10:03:00.000Z'
      },
      members: [],
      tags: ['general', 'discussion']
    } as any
  ],
  chat_messages: [
    {
      content: {
        text: 'Welcome to the general chat room! 👋',
        attachments: [],
        formatting: {
          markdown: false,
          mentions: [],
          hashtags: [],
          links: [],
          codeBlocks: []
        }
      },
      status: 'sent',
      priority: 'normal',
      timestamp: '2025-01-15T10:00:00.000Z',
      reactions: [],
      metadata: {
        source: 'user',
        deviceType: 'web'
      }
    } as any
  ],
  exportedAt: '2025-01-15T10:00:00.000Z'
};

export default seedData;