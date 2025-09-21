/**
 * Seed Data Constants - Single Source of Truth
 *
 * All IDs, collections, and seed data values defined once.
 * Typing like Rust - strict, explicit, and predictable.
 */

import { COLLECTIONS } from '../../system/data/core/FieldMapping';
import { stringToUUID, type UUID } from '../../system/core/types/CrossPlatformUUID';

// Re-export the authoritative COLLECTIONS from FieldMapping for consistency
export { COLLECTIONS } from '../../system/data/core/FieldMapping';

// User IDs - deterministic UUIDs based on user names
export const USER_IDS = {
  HUMAN: stringToUUID('Joel') as UUID,
  CLAUDE_CODE: stringToUUID('Claude Code') as UUID,
  GENERAL_AI: stringToUUID('GeneralAI') as UUID,
  CODE_AI: stringToUUID('CodeAI') as UUID,
  PLANNER_AI: stringToUUID('PlannerAI') as UUID,
  AUTO_ROUTE: stringToUUID('Auto Route') as UUID
} as const;

// Room IDs - deterministic UUIDs based on room names
export const ROOM_IDS = {
  GENERAL: stringToUUID('General') as UUID,
  ACADEMY: stringToUUID('Academy') as UUID,
  SUPPORT: stringToUUID('Support') as UUID,
  AI_TRAINING: stringToUUID('AI Training') as UUID
} as const;

// Message IDs - deterministic UUIDs based on message content keys
export const MESSAGE_IDS = {
  WELCOME_GENERAL: stringToUUID('Welcome General Message') as UUID,
  CLAUDE_INTRO: stringToUUID('Claude Introduction Message') as UUID,
  WELCOME_ACADEMY: stringToUUID('Welcome Academy Message') as UUID
} as const;

// User configuration constants
export const USER_CONFIG = {
  HUMAN: {
    NAME: 'human' as const,
    EMAIL: 'human@continuum.dev' as const,
    DISPLAY_NAME: 'Human User' as const,
    AVATAR: '🤖' as const
  },
  CLAUDE: {
    NAME: 'Claude Code' as const,
    MODEL: 'claude-sonnet-4' as const,
    PROVIDER: 'anthropic' as const
  },
  GENERAL_AI: {
    NAME: 'GeneralAI' as const,
    MODEL: 'claude-haiku' as const,
    PROVIDER: 'anthropic' as const
  },
  CODE_AI: {
    NAME: 'CodeAI' as const,
    MODEL: 'deepseek-coder' as const,
    PROVIDER: 'deepseek' as const  
  },
  PLANNER_AI: {
    NAME: 'PlannerAI' as const,
    MODEL: 'gpt-4' as const,
    PROVIDER: 'openai' as const
  },
  AUTO_ROUTE: {
    NAME: 'Auto Route' as const,
    MODEL: 'claude-haiku' as const,
    PROVIDER: 'anthropic' as const
  }
} as const;

// Room configuration constants
export const ROOM_CONFIG = {
  GENERAL: {
    NAME: 'General' as const,
    DESCRIPTION: 'Main discussion room for all users' as const,
    CATEGORY: 'general' as const
  },
  ACADEMY: {
    NAME: 'Academy' as const,
    DESCRIPTION: 'Learning and educational discussions' as const,
    CATEGORY: 'education' as const
  }
} as const;

// Message content constants  
export const MESSAGE_CONTENT = {
  WELCOME_GENERAL: 'Welcome to the General room! This is where we discuss development, collaborate, and share ideas.' as const,
  CLAUDE_INTRO: 'Hello! I\'m Claude Code, your AI development assistant. I can help with TypeScript, React, debugging, and system architecture. Feel free to ask me anything!' as const,
  WELCOME_ACADEMY: 'Welcome to the Academy! This room is for learning, tutorials, and educational discussions.' as const
} as const;

// Type-safe getters to ensure constants are used correctly
export function getUserId(key: keyof typeof USER_IDS): string {
  return USER_IDS[key];
}

export function getRoomId(key: keyof typeof ROOM_IDS): string {
  return ROOM_IDS[key];
}

export function getMessageId(key: keyof typeof MESSAGE_IDS): string {
  return MESSAGE_IDS[key];
}

export function getCollectionName(key: keyof typeof COLLECTIONS): string {
  return COLLECTIONS[key];
}