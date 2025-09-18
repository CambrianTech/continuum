/**
 * User Seed Data
 *
 * Exportable TypeScript data for seeding users
 */

import type { CreateUserData } from '../../system/data/domains/User';

export const seedUsers: CreateUserData[] = [
  {
    displayName: 'Joel',
    type: 'human',
    profile: {
      avatar: '👨‍💻',
      bio: 'System architect and developer',
      location: 'San Francisco'
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
    }
  },
  {
    displayName: 'Claude Code',
    type: 'ai',
    profile: {
      avatar: '🤖',
      bio: 'AI coding assistant powered by Claude',
      location: 'The Cloud'
    }
  },
  {
    displayName: 'GeneralAI',
    type: 'persona',
    profile: {
      avatar: '🎭',
      bio: 'General-purpose AI assistant',
      location: 'Virtual Space'
    }
  }
];