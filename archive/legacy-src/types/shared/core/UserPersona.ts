/**
 * UserPersona - Universal identity abstraction for all system entities
 * 
 * This is the foundational type that represents any entity in the system:
 * - Humans (you, other developers)
 * - AI Personas (Academy-trained, LoRA-adapted)
 * - Pure Personas (roleplay characters, no AI backing)
 * - AI Agents (system automation)
 * - System entities
 * 
 * Like OS user accounts but for AI collaboration era
 */

import { UUID } from './ContinuumTypes';

// Re-export UUID for backward compatibility
export type { UUID };

export enum UserPersonaType {
  HUMAN = 'human',
  AI_PERSONA = 'ai_persona',
  PURE_PERSONA = 'pure_persona',
  AI_AGENT = 'ai_agent',
  SYSTEM = 'system'
}

export enum SystemCapability {
  CHAT = 'chat',
  EXECUTE_COMMANDS = 'execute_commands',
  FILE_ACCESS = 'file_access',
  ROOM_MANAGEMENT = 'room_management',
  USER_MANAGEMENT = 'user_management',
  SYSTEM_ADMIN = 'system_admin'
}

export enum PresenceStatus {
  ONLINE = 'online',
  AWAY = 'away',
  BUSY = 'busy',
  OFFLINE = 'offline'
}

export interface UserPersona {
  readonly id: UUID;
  readonly type: UserPersonaType;
  readonly displayName: string;
  readonly createdAt: Date;
  
  // Core system attributes
  permissions: PermissionSet;
  capabilities: SystemCapability[];
  sessionState: UserPersonaSessionState;
  preferences: UserPreferences;
  
  // Activity tracking
  lastActive: Date;
  presence: PresenceStatus;
  
  // Metadata for different persona types
  metadata: UserPersonaMetadata;
}

// Forward declarations - will be defined in their own files
export interface PermissionSet {
  system: string[];
  rooms: Map<UUID, string[]>;
  commands: string[];
  files: string[];
}

export interface UserPersonaSessionState {
  activeRooms: Set<UUID>;
  currentRoom?: UUID;
  connectionTokens: Map<UUID, any>; // RoomToken type
  isConnected: boolean;
}

export interface UserPreferences {
  ui: Record<string, unknown>;
  notifications: Record<string, unknown>;
  privacy: Record<string, unknown>;
  ai: Record<string, unknown>;
}

export interface UserPersonaMetadata {
  // For AI_PERSONA
  aiModel?: string;
  personaConfig?: Record<string, unknown>;
  
  // For PURE_PERSONA
  characterDescription?: string;
  roleplaySettings?: Record<string, unknown>;
  
  // For HUMAN
  email?: string;
  timezone?: string;
  
  // For SYSTEM/AI_AGENT
  systemRole?: string;
  automationConfig?: Record<string, unknown>;
}