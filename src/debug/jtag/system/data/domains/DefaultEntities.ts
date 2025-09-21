/**
 * Default Entities - Shared Constants for Deterministic UUIDs
 *
 * Provides consistent entity IDs across all components using deterministic UUID generation.
 * These constants are safe to use across shared/browser/server boundaries.
 */

import { stringToUUID, type UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Default User Entity IDs - Deterministic UUIDs based on user names
 */
export const DEFAULT_USERS = {
  HUMAN: stringToUUID('Joel') as UUID,
  CLAUDE_CODE: stringToUUID('Claude Code') as UUID,
  GENERAL_AI: stringToUUID('GeneralAI') as UUID,
  CODE_AI: stringToUUID('CodeAI') as UUID,
  PLANNER_AI: stringToUUID('PlannerAI') as UUID,
  AUTO_ROUTE: stringToUUID('Auto Route') as UUID
} as const;

/**
 * Default Room Entity IDs - Deterministic UUIDs based on room names
 */
export const DEFAULT_ROOMS = {
  GENERAL: stringToUUID('General') as UUID,
  ACADEMY: stringToUUID('Academy') as UUID,
  SUPPORT: stringToUUID('Support') as UUID,
  AI_TRAINING: stringToUUID('AI Training') as UUID
} as const;

/**
 * Default Message Entity IDs - Deterministic UUIDs based on message content keys
 */
export const DEFAULT_MESSAGES = {
  WELCOME_GENERAL: stringToUUID('Welcome General Message') as UUID,
  CLAUDE_INTRO: stringToUUID('Claude Introduction Message') as UUID,
  WELCOME_ACADEMY: stringToUUID('Welcome Academy Message') as UUID
} as const;

/**
 * User Configuration Data - Display names, types, etc.
 */
export const USER_CONFIG = {
  HUMAN: {
    DISPLAY_NAME: 'Joel',
    TYPE: 'human' as const,
    AVATAR: '👤'
  },
  CLAUDE: {
    NAME: 'Claude Code',
    TYPE: 'ai' as const,
    AVATAR: '🤖'
  },
  GENERAL_AI: {
    NAME: 'GeneralAI',
    TYPE: 'ai' as const,
    AVATAR: '⚡'
  }
} as const;

/**
 * Room Configuration Data - Names, descriptions, etc.
 */
export const ROOM_CONFIG = {
  GENERAL: {
    NAME: 'General',
    DESCRIPTION: 'Main discussion room for all users',
    IS_PUBLIC: true
  },
  ACADEMY: {
    NAME: 'Academy',
    DESCRIPTION: 'Learning and educational discussions',
    IS_PUBLIC: true
  }
} as const;

/**
 * Message Content Templates
 */
export const MESSAGE_CONTENT = {
  WELCOME_GENERAL: 'Welcome to the General room! This is where we discuss development, collaborate, and share ideas.',
  CLAUDE_INTRO: 'Hello! I\'m Claude Code, your AI development assistant. I can help with TypeScript, React, debugging, and system architecture. Feel free to ask me anything!',
  WELCOME_ACADEMY: 'Welcome to the Academy! This room is for learning, tutorials, and educational discussions.'
} as const;