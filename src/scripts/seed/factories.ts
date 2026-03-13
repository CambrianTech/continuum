/**
 * Factory Functions for Seed Data Generation
 *
 * Reusable functions to create clean, type-safe data structures
 * for seeding the database.
 *
 * IMPORTANT: Use constants from UserCapabilitiesDefaults.ts for single source of truth
 */

import type { UserCapabilities } from '../../system/data/entities/UserEntity';
import {
  getDefaultCapabilitiesForType,
  getDefaultPreferencesForType
} from '../../system/user/config/UserCapabilitiesDefaults';

/** Seed data shape for a chat room (pre-persistence) */
interface SeedRoom {
  id: string;
  uniqueId: string;
  name: string;
  displayName: string;
  description: string;
  topic: string;
  type: string;
  status: string;
  ownerId: string;
  lastMessageAt: string;
  recipeId: string;
  privacy: { isPublic: boolean; requiresInvite: boolean; allowGuestAccess: boolean; searchable: boolean };
  settings: { allowReactions: boolean; allowThreads: boolean; allowFileSharing: boolean; messageRetentionDays: number };
  stats: { memberCount: number; messageCount: number; createdAt: string; lastActivityAt: string };
  members: string[];
  tags: string[];
}

/** Seed data shape for a chat message (pre-persistence) */
interface SeedChatMessage {
  id: string;
  roomId: string;
  senderId: string;
  senderType: 'user' | 'bot' | 'system';
  content: {
    text: string;
    attachments: unknown[];
    formatting: { markdown: boolean; mentions: unknown[]; hashtags: unknown[]; links: unknown[]; codeBlocks: unknown[] };
  };
  status: string;
  priority: string;
  timestamp: string;
  reactions: unknown[];
}

/** Seed data shape for a content type definition */
interface SeedContentType {
  id: string;
  type: string;
  displayName: string;
  description: string;
  category: string;
  config: {
    widgetSelector: string;
    allowMultiple: boolean;
    autoSave: boolean;
    preloadData: boolean;
    requiredPermissions: string[];
    minUserType: string;
  };
  isActive: boolean;
  isBuiltIn: boolean;
  sortOrder: number;
}

/** Seed data shape for a user state */
interface SeedUserState {
  id: string;
  userId: string;
  deviceId: string;
  contentState: { openItems: unknown[]; lastUpdatedAt: string };
  preferences: Record<string, unknown>;
}

/** Seed data shape for a training session template */
interface SeedTrainingSession {
  id: string;
  sessionName: string;
  description: string;
  status: string;
  hyperparameters: {
    learningRate: number;
    batchSize: number;
    epochs: number;
    warmupSteps: number;
    maxGradNorm: number;
    weightDecay: number;
  };
  isTemplate: boolean;
}

/**
 * Create user capabilities based on user type
 * Uses single source of truth from UserCapabilitiesDefaults
 */
export function createUserCapabilities(type: 'human' | 'agent' | 'persona'): UserCapabilities {
  return getDefaultCapabilitiesForType(type);
}

/**
 * Create a room entity (not persisted yet)
 */
export function createRoom(
  id: string,
  name: string,
  displayName: string,
  description: string,
  topic: string,
  memberCount: number,
  tags: string[],
  ownerId: string,
  uniqueId: string,
  recipeId: string = 'general-chat'
): SeedRoom {
  return {
    id,
    uniqueId,
    name: name.toLowerCase(),
    displayName,
    description,
    topic,
    type: "public",
    status: "active",
    ownerId,
    lastMessageAt: new Date().toISOString(),
    recipeId,
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
      memberCount,
      messageCount: 0,
      createdAt: new Date().toISOString(),
      lastActivityAt: new Date().toISOString()
    },
    members: [], // Empty - let RoomMembershipDaemon handle auto-join
    tags
  };
}

/**
 * Create a chat message entity
 */
export function createChatMessage(
  id: string,
  roomId: string,
  senderId: string,
  text: string,
  senderType: "user" | "bot" | "system" = "user"
): SeedChatMessage {
  return {
    id,
    roomId,
    senderId,
    senderType,
    content: {
      text,
      attachments: [],
      formatting: {
        markdown: false,
        mentions: [],
        hashtags: [],
        links: [],
        codeBlocks: []
      }
    },
    status: "sent",
    priority: "normal",
    timestamp: new Date().toISOString(),
    reactions: []
  };
}

/**
 * Create default content type registry
 */
export function createDefaultContentTypes(): SeedContentType[] {
  return [
    {
      id: 'ct-chat',
      type: 'chat',
      displayName: 'Chat Room',
      description: 'Real-time chat communication',
      category: 'communication',
      config: {
        widgetSelector: 'chat-widget',
        allowMultiple: true,
        autoSave: true,
        preloadData: true,
        requiredPermissions: ['chat:read', 'chat:write'],
        minUserType: 'human'
      },
      isActive: true,
      isBuiltIn: true,
      sortOrder: 10
    },
    {
      id: 'ct-academy',
      type: 'academy-session',
      displayName: 'Academy Training',
      description: 'AI training sessions with hyperparameters',
      category: 'development',
      config: {
        widgetSelector: 'chat-widget',
        allowMultiple: true,
        autoSave: true,
        preloadData: true,
        requiredPermissions: ['academy:read', 'academy:participate'],
        minUserType: 'human'
      },
      isActive: true,
      isBuiltIn: true,
      sortOrder: 20
    },
    {
      id: 'ct-user-list',
      type: 'user-list',
      displayName: 'User Directory',
      description: 'User management and directory',
      category: 'management',
      config: {
        widgetSelector: 'user-list-widget',
        allowMultiple: false,
        autoSave: false,
        preloadData: true,
        requiredPermissions: ['users:read'],
        minUserType: 'human'
      },
      isActive: true,
      isBuiltIn: true,
      sortOrder: 30
    }
  ];
}

/**
 * Create default user states - Fixed to match UserStateEntity schema
 * Uses single source of truth from UserCapabilitiesDefaults
 */
export function createDefaultUserStates(humanUserId: string, claudeUserId: string): SeedUserState[] {
  const humanPrefs = getDefaultPreferencesForType('human');
  const agentPrefs = getDefaultPreferencesForType('agent');

  return [
    {
      id: 'us-joel-chat',
      userId: humanUserId,
      deviceId: 'browser-main',
      contentState: {
        openItems: [],
        lastUpdatedAt: new Date().toISOString()
      },
      preferences: {
        ...humanPrefs,
        theme: 'dark'  // Custom preference for theme persistence
      }
    },
    {
      id: 'us-claude-chat',
      userId: claudeUserId,
      deviceId: 'server-instance',
      contentState: {
        openItems: [],
        lastUpdatedAt: new Date().toISOString()
      },
      preferences: {
        ...agentPrefs,
        theme: 'dark'
      }
    }
  ];
}

/**
 * Create default training session templates
 */
export function createDefaultTrainingSessions(): SeedTrainingSession[] {
  return [
    {
      id: 'ts-template-chat',
      sessionName: 'General Chat Training',
      description: 'Default hyperparameters for conversational chat training',
      status: 'template',
      hyperparameters: {
        learningRate: 0.0001,
        batchSize: 16,
        epochs: 10,
        warmupSteps: 100,
        maxGradNorm: 1.0,
        weightDecay: 0.01
      },
      isTemplate: true
    },
    {
      id: 'ts-template-code',
      sessionName: 'Code Generation Training',
      description: 'Optimized for code generation and debugging tasks',
      status: 'template',
      hyperparameters: {
        learningRate: 0.00005,
        batchSize: 8,
        epochs: 15,
        warmupSteps: 200,
        maxGradNorm: 0.5,
        weightDecay: 0.02
      },
      isTemplate: true
    }
  ];
}
