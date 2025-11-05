/**
 * UserSystemTypes - Shared types for Universal User System
 * 
 * SINGLE SOURCE OF TRUTH: These types are used by both client and server
 * to ensure consistency in user management, WebSocket communication, and
 * AI model interactions across the entire system.
 * 
 * Prevents drift between browser and server user representations.
 */

/**
 * Core User Type Definitions
 */
export type UserType = 'human' | 'persona' | 'ai-model' | 'ai-system';

export type UserStatus = 'online' | 'offline' | 'thinking' | 'working' | 'idle';

export type UserCapability = 
  | 'all'
  | 'general-intelligence'
  | 'reasoning' 
  | 'coding'
  | 'analysis'
  | 'multimodal'
  | 'creativity'
  | 'ui-design'
  | 'css'
  | 'user-experience'
  | 'typescript'
  | 'architecture'
  | 'debugging'
  | 'testing'
  | 'quality-assurance'
  | 'user-flows'
  | 'security'
  | 'validation'
  | 'protocol-enforcement'
  | 'writing';

/**
 * Universal User Interface - Shared between client and server
 */
export interface SharedUniversalUser {
  id: string;
  name: string;
  type: UserType;
  avatar: string;
  status: UserStatus;
  capabilities: UserCapability[];
  lastSeen?: string; // ISO date string for serialization
  currentTask?: string;
  mood?: string;
  model?: string; // For AI models: 'gpt-4o', 'claude-3-sonnet', etc.
  isClickable: boolean;
}

/**
 * User Action Types for WebSocket Communication
 */
export type UserActionType = 'chat' | 'train' | 'select' | 'call' | 'collaborate';

export interface UserClickEvent {
  userId: string;
  userType: UserType;
  action: UserActionType;
  context?: Record<string, unknown>;
  timestamp: string;
}

export interface PersonaInteractionRequest {
  personaId: string;
  personaName: string;
  requestedAction: 'chat' | 'train' | 'collaborate';
  context?: Record<string, unknown>;
  initiatedBy: string; // User ID who initiated
  sessionId: string;
}

export interface AIModelConversationRequest {
  modelId: string;
  modelName: string;
  model: string;
  requestedAction: 'chat' | 'query' | 'analyze';
  context?: Record<string, unknown>;
  initiatedBy: string;
  sessionId: string;
}

/**
 * AI Model Communication - Prevents the "Model and message are required" errors
 */
export interface ModelMessageRequest {
  modelId: string;
  message: string;
  requestId: string;
  model: string; // Required: actual model name like 'gpt-4o'
  timeout?: number;
  sessionId: string;
  priority?: 'background' | 'normal' | 'urgent';
}

export interface ModelMessageResponse {
  requestId: string;
  modelId: string;
  success: boolean;
  content?: string;
  error?: {
    type: 'validation' | 'timeout' | 'model_error' | 'permission' | 'unknown';
    message: string;
    code?: string;
  };
  executionTimeMs: number;
  timestamp: string;
}

/**
 * User System Events for Real-time Updates
 */
export interface UserAddedEvent {
  user: SharedUniversalUser;
  addedBy: string;
  timestamp: string;
}

export interface UserUpdatedEvent {
  userId: string;
  updates: Partial<SharedUniversalUser>;
  updatedBy: string;
  timestamp: string;
}

export interface UserRemovedEvent {
  userId: string;
  removedBy: string;
  timestamp: string;
}

export interface UserStatusChangedEvent {
  userId: string;
  previousStatus: UserStatus;
  newStatus: UserStatus;
  reason?: string;
  timestamp: string;
}

/**
 * User System State Management
 */
export interface UserSystemState {
  users: SharedUniversalUser[];
  activeConnections: {
    userId: string;
    connectionId: string;
    connectedAt: string;
  }[];
  systemMetrics: {
    totalUsers: number;
    onlineUsers: number;
    activePersonas: number;
    availableAIModels: number;
  };
  lastUpdated: string;
}

/**
 * WebSocket Message Types for User System
 */
export type UserSystemClientToServerMessage = 
  | { type: 'user_click_event'; data: UserClickEvent }
  | { type: 'persona_interaction_request'; data: PersonaInteractionRequest }
  | { type: 'ai_model_conversation_request'; data: AIModelConversationRequest }
  | { type: 'model_message_request'; data: ModelMessageRequest }
  | { type: 'user_status_update'; data: { userId: string; status: UserStatus; reason?: string } };

export type UserSystemServerToClientMessage =
  | { type: 'user_added'; data: UserAddedEvent }
  | { type: 'user_updated'; data: UserUpdatedEvent }
  | { type: 'user_removed'; data: UserRemovedEvent }
  | { type: 'user_status_changed'; data: UserStatusChangedEvent }
  | { type: 'model_message_response'; data: ModelMessageResponse }
  | { type: 'user_system_state'; data: UserSystemState };

/**
 * Type Guards for Runtime Validation
 */
export function isValidUserType(type: string): type is UserType {
  return ['human', 'persona', 'ai-model', 'ai-system'].includes(type);
}

export function isValidUserStatus(status: string): status is UserStatus {
  return ['online', 'offline', 'thinking', 'working', 'idle'].includes(status);
}

export function isValidUserCapability(capability: string): capability is UserCapability {
  const validCapabilities: UserCapability[] = [
    'all', 'general-intelligence', 'reasoning', 'coding', 'analysis',
    'multimodal', 'creativity', 'ui-design', 'css', 'user-experience',
    'typescript', 'architecture', 'debugging', 'testing', 'quality-assurance',
    'user-flows', 'security', 'validation', 'protocol-enforcement', 'writing'
  ];
  return validCapabilities.includes(capability as UserCapability);
}

export function isModelMessageRequest(msg: any): msg is { type: 'model_message_request'; data: ModelMessageRequest } {
  return msg?.type === 'model_message_request' &&
         typeof msg?.data?.modelId === 'string' &&
         typeof msg?.data?.message === 'string' &&
         typeof msg?.data?.model === 'string' &&
         typeof msg?.data?.requestId === 'string';
}

export function isUserClickEvent(msg: any): msg is { type: 'user_click_event'; data: UserClickEvent } {
  return msg?.type === 'user_click_event' &&
         typeof msg?.data?.userId === 'string' &&
         isValidUserType(msg?.data?.userType);
}

/**
 * Default User Configurations
 */
export const DEFAULT_USERS: Readonly<SharedUniversalUser[]> = [
  {
    id: 'human-user',
    name: 'YOU',
    type: 'human',
    avatar: '👤',
    status: 'online',
    capabilities: ['all'],
    isClickable: false
  },
  {
    id: 'gpt-4o',
    name: 'Marcus (GPT-4o)',
    type: 'ai-model',
    avatar: '🧠',
    status: 'online',
    capabilities: ['general-intelligence', 'reasoning', 'coding', 'analysis'],
    model: 'gpt-4o',
    isClickable: true
  },
  {
    id: 'claude-sonnet',
    name: 'Claude (Sonnet)',
    type: 'ai-model',
    avatar: '🎭',
    status: 'online',
    capabilities: ['reasoning', 'writing', 'coding', 'analysis'],
    model: 'claude-3-sonnet',
    isClickable: true
  },
  {
    id: 'aria-model',
    name: 'Aria',
    type: 'ai-model',
    avatar: '🎵',
    status: 'online',
    capabilities: ['multimodal', 'reasoning', 'creativity'],
    model: 'aria',
    isClickable: true
  }
] as const;

/**
 * Model Name Mappings for API calls
 */
export const MODEL_NAME_MAP: Record<string, string> = {
  'gpt-4o': 'gpt-4o',
  'claude-sonnet': 'claude-3-sonnet',
  'aria-model': 'aria'
} as const;

/**
 * Migration Aliases - For gradual transition from existing types
 */
export type UniversalUser = SharedUniversalUser; // Backward compatibility
export type UserActionEvent = UserClickEvent; // Descriptive naming