/**
 * Seed Data Constants - Single Source of Truth
 * 
 * All IDs, collections, and seed data values defined once.
 * Typing like Rust - strict, explicit, and predictable.
 */

// Collection names - branded types for safety
export const COLLECTIONS = {
  USERS: 'users' as const,
  ROOMS: 'rooms' as const,  
  MESSAGES: 'messages' as const
} as const;

// User IDs - single source of truth
export const USER_IDS = {
  HUMAN: 'user-human-12345' as const,
  CLAUDE_CODE: 'claude-code-agent' as const,
  GENERAL_AI: 'general-ai-persona' as const,
  CODE_AI: 'code-ai-agent' as const,
  PLANNER_AI: 'planner-ai-agent' as const,
  AUTO_ROUTE: 'auto-route-agent' as const
} as const;

// Room IDs - single source of truth  
export const ROOM_IDS = {
  GENERAL: 'room-general' as const,
  ACADEMY: 'room-academy' as const,
  SUPPORT: 'room-support' as const,
  AI_TRAINING: 'room-ai-training' as const
} as const;

// Message IDs - single source of truth
export const MESSAGE_IDS = {
  WELCOME_GENERAL: 'msg-welcome-general' as const,
  CLAUDE_INTRO: 'msg-claude-intro' as const,
  WELCOME_ACADEMY: 'msg-welcome-academy' as const
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