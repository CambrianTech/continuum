/**
 * ChatTypes - Shared type definitions for chat widget
 * 
 * Used by both server and browser implementations
 * Ensures type safety across the entire chat system
 */

export interface ChatMessage {
  id: string;
  content: string;
  roomId: string;
  userId: string;
  type: 'user' | 'assistant' | 'system';
  timestamp: string; // ISO string
  status?: 'sending' | 'sent' | 'delivered' | 'error';
  metadata?: {
    replyTo?: string; // ID of message being replied to
    model?: string; // AI model used for assistant messages
    persona?: string; // AI persona used
    systemEvent?: string; // For system messages
    [key: string]: any;
  };
}

export interface ChatUser {
  id: string;
  name: string;
  avatar: string;
  status: 'online' | 'away' | 'busy' | 'offline';
  joinedAt: string; // ISO string
  lastSeen?: string; // ISO string
  metadata?: {
    isAI?: boolean;
    persona?: string;
    capabilities?: string[];
    [key: string]: any;
  };
}

export interface ChatRoom {
  id: string;
  name: string;
  description: string;
  users: Map<string, ChatUser>;
  createdAt: string; // ISO string
  metadata?: {
    isPrivate?: boolean;
    maxUsers?: number;
    tags?: string[];
    [key: string]: any;
  };
}

export interface ChatWidgetConfig {
  // Connection settings
  autoConnect?: boolean;
  defaultRoom?: string;
  maxRetries?: number;
  reconnectDelay?: number;
  
  // UI settings
  showUserList?: boolean;
  showTypingIndicators?: boolean;
  messageLimit?: number;
  autoScroll?: boolean;
  
  // Features
  allowMarkdown?: boolean;
  allowFileUpload?: boolean;
  allowEmojis?: boolean;
  soundNotifications?: boolean;
  
  // AI settings
  enableAI?: boolean;
  defaultAIPersona?: string;
  aiResponseDelay?: number;
  
  // Appearance
  theme?: 'basic' | 'cyberpunk' | 'anime' | 'custom';
  compactMode?: boolean;
  showTimestamps?: boolean;
  
  // Advanced
  cacheMessages?: boolean;
  persistHistory?: boolean;
  encryptMessages?: boolean;
}

export interface ChatWidgetState {
  // Connection state
  isConnected: boolean;
  connectionStatus: 'connecting' | 'connected' | 'disconnected' | 'error';
  lastError?: string;
  
  // Current context
  currentRoom?: ChatRoom;
  currentUser?: ChatUser;
  
  // UI state
  isTyping: boolean;
  typingUsers: ChatUser[];
  unreadCount: number;
  
  // Message state
  messages: ChatMessage[];
  messageCache: Map<string, ChatMessage[]>; // roomId -> messages
  isLoading: boolean;
  hasMoreHistory: boolean;
}

export interface ChatEventData {
  // Message events
  message_received: { message: ChatMessage };
  message_sent: { message: ChatMessage };
  message_updated: { messageId: string; message: ChatMessage };
  message_deleted: { messageId: string; roomId: string };
  
  // User events  
  user_joined: { user: ChatUser; roomId: string };
  user_left: { user: ChatUser; roomId: string };
  user_status_changed: { user: ChatUser; oldStatus: string; newStatus: string };
  
  // Room events
  room_joined: { room: ChatRoom; user: ChatUser };
  room_left: { room: ChatRoom; user: ChatUser };
  room_updated: { room: ChatRoom };
  
  // Typing events
  typing_indicator: { userId: string; isTyping: boolean; roomId: string };
  
  // System events
  connection_status_changed: { status: string; error?: string };
  error_occurred: { error: string; context?: string };
}

export type ChatEventType = keyof ChatEventData;

export interface ChatCommand {
  type: 'send_message' | 'join_room' | 'leave_room' | 'get_history' | 'get_users' | 'typing_indicator';
  data: any;
  requestId?: string;
}

export interface ChatCommandResponse {
  success: boolean;
  data?: any;
  error?: string;
  requestId?: string;
}

// Validation schemas
export const ChatMessageSchema = {
  required: ['content', 'roomId', 'userId', 'type'],
  properties: {
    content: { type: 'string', minLength: 1, maxLength: 10000 },
    roomId: { type: 'string', minLength: 1 },
    userId: { type: 'string', minLength: 1 },
    type: { enum: ['user', 'assistant', 'system'] }
  }
};

export const ChatUserSchema = {
  required: ['id', 'name'],
  properties: {
    id: { type: 'string', minLength: 1 },
    name: { type: 'string', minLength: 1, maxLength: 100 },
    avatar: { type: 'string' },
    status: { enum: ['online', 'away', 'busy', 'offline'] }
  }
};

// Helper functions
export function createChatMessage(
  content: string,
  roomId: string,
  userId: string,
  type: ChatMessage['type'] = 'user',
  metadata?: ChatMessage['metadata']
): Omit<ChatMessage, 'id' | 'timestamp'> {
  return {
    content,
    roomId,
    userId,
    type,
    metadata: metadata || {}
  };
}

export function validateChatMessage(message: any): message is ChatMessage {
  return (
    typeof message === 'object' &&
    typeof message.content === 'string' &&
    message.content.length > 0 &&
    typeof message.roomId === 'string' &&
    typeof message.userId === 'string' &&
    ['user', 'assistant', 'system'].includes(message.type)
  );
}

export function validateChatUser(user: any): user is ChatUser {
  return (
    typeof user === 'object' &&
    typeof user.id === 'string' &&
    typeof user.name === 'string' &&
    typeof user.avatar === 'string' &&
    ['online', 'away', 'busy', 'offline'].includes(user.status)
  );
}

export function formatMessageTime(timestamp: string): string {
  const date = new Date(timestamp);
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export function formatMessageDate(timestamp: string): string {
  const date = new Date(timestamp);
  const now = new Date();
  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  
  if (date.toDateString() === now.toDateString()) {
    return 'Today';
  } else if (date.toDateString() === yesterday.toDateString()) {
    return 'Yesterday';
  } else {
    return date.toLocaleDateString();
  }
}

export function isSystemMessage(message: ChatMessage): boolean {
  return message.type === 'system';
}

export function isAIMessage(message: ChatMessage): boolean {
  return message.type === 'assistant';
}

export function isUserMessage(message: ChatMessage): boolean {
  return message.type === 'user';
}

export function getMessageAuthor(message: ChatMessage, users: Map<string, ChatUser>): ChatUser | null {
  if (message.type === 'system') {
    return {
      id: 'system',
      name: 'System',
      avatar: 'ℹ️',
      status: 'online',
      joinedAt: '',
      metadata: { isSystem: true }
    };
  }
  
  if (message.type === 'assistant') {
    return {
      id: 'ai_assistant',
      name: message.metadata?.persona || 'AI Assistant',
      avatar: '🤖',
      status: 'online',
      joinedAt: '',
      metadata: { isAI: true, persona: message.metadata?.persona }
    };
  }
  
  return users.get(message.userId) || null;
}

// Event emitter helpers
export type ChatEventListener<T extends ChatEventType> = (data: ChatEventData[T]) => void;

export class ChatEventEmitter {
  private listeners = new Map<ChatEventType, Set<Function>>();
  
  on<T extends ChatEventType>(eventType: T, listener: ChatEventListener<T>): void {
    if (!this.listeners.has(eventType)) {
      this.listeners.set(eventType, new Set());
    }
    this.listeners.get(eventType)!.add(listener);
  }
  
  off<T extends ChatEventType>(eventType: T, listener: ChatEventListener<T>): void {
    const listeners = this.listeners.get(eventType);
    if (listeners) {
      listeners.delete(listener);
      if (listeners.size === 0) {
        this.listeners.delete(eventType);
      }
    }
  }
  
  emit<T extends ChatEventType>(eventType: T, data: ChatEventData[T]): void {
    const listeners = this.listeners.get(eventType);
    if (listeners) {
      listeners.forEach(listener => {
        try {
          listener(data);
        } catch (error) {
          console.error(`Chat event listener error for ${eventType}:`, error);
        }
      });
    }
  }
  
  clear(): void {
    this.listeners.clear();
  }
}