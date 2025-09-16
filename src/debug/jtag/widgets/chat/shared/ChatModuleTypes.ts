/**
 * Shared Types for Chat Module - Used across all chat widgets
 * 
 * Includes types for:
 * - Chat Widget (messaging)
 * - User List Widget (participants)  
 * - Room List Widget (room navigation)
 * - Tab System Integration
 */

export interface ChatUser {
  id: string;
  name: string;
  avatar: string;
  role: 'USER' | 'AI Assistant' | 'Smart agent selection' | 'Code analysis & debugging' | 'General assistance' | 'Strategy & web commands';
  status: 'online' | 'away' | 'busy' | 'offline';
  lastActive: string; // e.g., "8:19:21 AM"
  isStarred: boolean;
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
  displayName: string; // e.g., "General", "Academy", "Dev"
  description: string;
  participants: ChatUser[];
  messageCount: number;
  unreadCount: number;
  lastMessage?: {
    content: string;
    timestamp: string;
    sender: string;
  };
  isOpen: boolean; // For tab system integration
  createdAt: string;
  metadata?: {
    isPrivate?: boolean;
    maxUsers?: number;
    tags?: string[];
    [key: string]: any;
  };
}

export interface ChatMessage {
  id: string;
  content: string;
  roomId: string;
  senderId: string;
  senderName: string;
  timestamp: string;
  type: 'user' | 'assistant' | 'system';  // What TYPE of entity sent this
  status?: 'sending' | 'sent' | 'delivered' | 'error';
  metadata?: {
    replyTo?: string;
    model?: string;
    persona?: string;
    systemEvent?: string;
    [key: string]: any;
  };
}

/**
 * Convenience methods for ChatMessage (Rust-like approach with explicit typing)
 * These methods provide semantic operations while maintaining type safety
 */
export interface ChatMessageMethods {
  /** Determine if this message was sent by the current user (for positioning) */
  isFromCurrentUser(currentUserId: string): boolean;

  /** Get semantic CSS class for user positioning (current-user vs other-user) */
  getUserPositionClass(currentUserId: string): 'current-user' | 'other-user';

  /** Get alignment direction for message positioning */
  getAlignment(currentUserId: string): 'right' | 'left';

  /** Check if message is from an AI assistant */
  isFromAssistant(): boolean;

  /** Check if message is a system message */
  isSystemMessage(): boolean;

  /** Get display name with fallback logic */
  getDisplayName(): string;

  /** Check if message has error status */
  hasError(): boolean;

  /** Check if message is still being sent */
  isPending(): boolean;
}

// Event system for coordinated widgets
export interface ChatModuleEvents {
  // Room events
  'room:selected': { roomId: string; room: ChatRoom };
  'room:opened': { roomId: string; room: ChatRoom };
  'room:closed': { roomId: string; room: ChatRoom };
  'room:updated': { roomId: string; room: ChatRoom };
  
  // User events  
  'user:selected': { userId: string; user: ChatUser };
  'user:joined': { userId: string; roomId: string; user: ChatUser };
  'user:left': { userId: string; roomId: string; user: ChatUser };
  'user:status-changed': { userId: string; user: ChatUser; oldStatus: string; newStatus: string };
  
  // Message events
  'message:received': { message: ChatMessage; roomId: string };
  'message:sent': { message: ChatMessage; roomId: string };
  'message:updated': { messageId: string; message: ChatMessage };
  'message:deleted': { messageId: string; roomId: string };
  
  // Tab events
  'tab:room-highlighted': { roomId: string; room: ChatRoom };
  'tab:room-closed': { roomId: string; room: ChatRoom };
  
  // System events
  'connection:status-changed': { status: string; error?: string };
  'theme:changed': { themeName: string; customProperties?: Record<string, string> };
}

export type ChatModuleEventType = keyof ChatModuleEvents;
export type ChatModuleEventData<T extends ChatModuleEventType> = ChatModuleEvents[T];

// Widget configuration
export interface ChatModuleConfig {
  // Default rooms to create
  defaultRooms: string[];
  
  // Default users/agents to populate
  defaultUsers: Omit<ChatUser, 'id'>[];
  
  // Theme settings
  theme: string;
  
  // Feature flags
  features: {
    allowPrivateRooms: boolean;
    allowDirectMessages: boolean;
    showUserStatus: boolean;
    showLastActive: boolean;
    allowUserStarring: boolean;
    persistRoomTabs: boolean;
  };
}

// Default configuration
export const DEFAULT_CHAT_CONFIG: ChatModuleConfig = {
  defaultRooms: ['general', 'academy', 'dev', 'research', 'grid'],
  
  defaultUsers: [
    {
      name: 'Claude Code',
      avatar: '🤖',
      role: 'AI Assistant',
      status: 'online',
      lastActive: '8:19:21 AM',
      isStarred: true,
      metadata: { isAI: true, persona: 'Claude Code', capabilities: ['coding', 'architecture', 'debugging'] }
    },
    {
      name: 'Auto Route',
      avatar: '🔄',
      role: 'Smart agent selection', 
      status: 'online',
      lastActive: '8:19:21 AM',
      isStarred: false,
      metadata: { isAI: true, persona: 'Auto Route', capabilities: ['routing', 'agent-selection'] }
    },
    {
      name: 'CodeAI',
      avatar: '💻',
      role: 'Code analysis & debugging',
      status: 'online', 
      lastActive: '8:19:21 AM',
      isStarred: false,
      metadata: { isAI: true, persona: 'CodeAI', capabilities: ['code-analysis', 'debugging', 'refactoring'] }
    },
    {
      name: 'GeneralAI',
      avatar: '🧠',
      role: 'General assistance',
      status: 'online',
      lastActive: '8:19:21 AM', 
      isStarred: false,
      metadata: { isAI: true, persona: 'GeneralAI', capabilities: ['general-assistance', 'research'] }
    },
    {
      name: 'PlannerAI',
      avatar: '📋',
      role: 'Strategy & web commands',
      status: 'online',
      lastActive: '8:19:21 AM',
      isStarred: false,
      metadata: { isAI: true, persona: 'PlannerAI', capabilities: ['strategy', 'planning', 'web-commands'] }
    }
  ],
  
  theme: 'cyberpunk',
  
  features: {
    allowPrivateRooms: true,
    allowDirectMessages: true,
    showUserStatus: true,
    showLastActive: true,
    allowUserStarring: true,
    persistRoomTabs: true
  }
};

// Helper functions
export function createChatUser(data: Partial<ChatUser> & Pick<ChatUser, 'name' | 'role'>): ChatUser {
  return {
    id: crypto.randomUUID(),
    avatar: '👤',
    status: 'online',
    lastActive: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }),
    isStarred: false,
    ...data
  };
}

export function createChatRoom(data: Partial<ChatRoom> & Pick<ChatRoom, 'name'>): ChatRoom {
  return {
    id: data.id || data.name.toLowerCase(),
    displayName: data.name.charAt(0).toUpperCase() + data.name.slice(1),
    description: `${data.name.charAt(0).toUpperCase() + data.name.slice(1)} chat room`,
    participants: [],
    messageCount: 0,
    unreadCount: 0,
    isOpen: false,
    createdAt: new Date().toISOString(),
    ...data
  };
}

export function formatLastActive(timestamp: string): string {
  const date = new Date(timestamp);
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

export function getUserStatusColor(status: ChatUser['status']): string {
  const colors = {
    online: 'var(--success-color, #00ff88)',
    away: 'var(--warning-color, #ffaa00)',
    busy: 'var(--error-color, #ff4444)',
    offline: 'var(--content-secondary, #666666)'
  };
  return colors[status] || colors.offline;
}

/**
 * ChatMessage convenience methods implementation
 * Rust-like approach: explicit, predictable, type-safe
 */
export const ChatMessageHelpers = {
  /** Determine if this message was sent by the current user (for positioning) */
  isFromCurrentUser(message: ChatMessage, currentUserId: string): boolean {
    return message.senderId === currentUserId;
  },

  /** Get semantic CSS class for user positioning (current-user vs other-user) */
  getUserPositionClass(message: ChatMessage, currentUserId: string): 'current-user' | 'other-user' {
    return ChatMessageHelpers.isFromCurrentUser(message, currentUserId) ? 'current-user' : 'other-user';
  },

  /** Get alignment direction for message positioning */
  getAlignment(message: ChatMessage, currentUserId: string): 'right' | 'left' {
    return ChatMessageHelpers.isFromCurrentUser(message, currentUserId) ? 'right' : 'left';
  },

  /** Check if message is from an AI assistant */
  isFromAssistant(message: ChatMessage): boolean {
    return message.type === 'assistant';
  },

  /** Check if message is a system message */
  isSystemMessage(message: ChatMessage): boolean {
    return message.type === 'system';
  },

  /** Get display name with fallback logic */
  getDisplayName(message: ChatMessage): string {
    return message.senderName || message.senderId || 'Unknown User';
  },

  /** Check if message has error status */
  hasError(message: ChatMessage): boolean {
    return message.status === 'error';
  },

  /** Check if message is still being sent */
  isPending(message: ChatMessage): boolean {
    return message.status === 'sending';
  }
} as const;