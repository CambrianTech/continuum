/**
 * Chat Architecture - Universal Chat System Design
 * 
 * This defines the chat system architecture where:
 * - PersonaBase is the universal entity for all chat interactions
 * - Chat widgets work with any PersonaBase entity
 * - Chat rooms are collections of PersonaBase participants
 * - Chat commands are executed by PersonaBase entities
 * - Everything works through the universal interface
 * 
 * Key principle: PersonaBase is the foundation for ALL chat interactions
 */

import { PersonaBase } from '../../academy/shared/PersonaBase';
import { ChatParticipant } from '../../academy/shared/ChatParticipant';
import { generateUUID } from '../../academy/shared/AcademyTypes';

// ==================== CHAT MESSAGE SYSTEM ====================

/**
 * Universal chat message - works with any PersonaBase entity
 */
export interface ChatMessage {
  id: string;
  sender: PersonaBase;
  content: string;
  timestamp: number;
  
  // Message metadata
  type: MessageType;
  roomId?: string;
  threadId?: string;
  
  // Optional enhancements
  attachments?: ChatAttachment[];
  reactions?: ChatReaction[];
  mentions?: string[]; // PersonaBase IDs
  
  // Command context
  isCommand?: boolean;
  commandName?: string;
  commandArgs?: any[];
  
  // Response tracking
  responseToId?: string;
  conversationId?: string;
}

export type MessageType = 
  | 'text'
  | 'command'
  | 'system'
  | 'notification'
  | 'error'
  | 'success'
  | 'file'
  | 'image'
  | 'code';

export interface ChatAttachment {
  id: string;
  name: string;
  type: string;
  size: number;
  url?: string;
  data?: any;
}

export interface ChatReaction {
  emoji: string;
  count: number;
  reactors: string[]; // PersonaBase IDs
}

// ==================== CHAT ROOM SYSTEM ====================

/**
 * Chat room - collection of PersonaBase participants
 */
export interface ChatRoom {
  id: string;
  name: string;
  description?: string;
  created: number;
  
  // Participants are all PersonaBase entities
  participants: PersonaBase[];
  moderators: string[]; // PersonaBase IDs
  
  // Room configuration
  settings: ChatRoomSettings;
  
  // Room state
  isActive: boolean;
  lastActivity: number;
  messageCount: number;
  
  // Optional features
  topic?: string;
  tags?: string[];
  category?: string;
}

export interface ChatRoomSettings {
  // Access control
  isPublic: boolean;
  requireInvite: boolean;
  allowGuests: boolean;
  
  // Moderation
  moderationLevel: 'none' | 'basic' | 'strict';
  allowCommands: boolean;
  commandPrefix: string;
  
  // Features
  enableFileSharing: boolean;
  enableReactions: boolean;
  enableThreads: boolean;
  enableMentions: boolean;
  
  // Limits
  maxParticipants: number;
  maxMessageLength: number;
  rateLimitPerMinute: number;
  
  // Persistence
  persistHistory: boolean;
  historyRetentionDays: number;
}

// ==================== CHAT COMMANDS ====================

/**
 * Chat command - executed by PersonaBase entities
 */
export interface ChatCommand {
  name: string;
  description: string;
  usage: string;
  
  // Command metadata
  aliases?: string[];
  category?: string;
  permissions?: string[];
  
  // Execution context
  requiresRoom?: boolean;
  requiresModerator?: boolean;
  allowedRoles?: string[];
  
  // Handler function
  handler: ChatCommandHandler;
}

export type ChatCommandHandler = (
  context: ChatCommandContext
) => Promise<ChatCommandResult>;

export interface ChatCommandContext {
  // Command info
  command: string;
  args: string[];
  rawInput: string;
  
  // Execution context
  sender: PersonaBase;
  room?: ChatRoom;
  message: ChatMessage;
  
  // System access
  chatSystem: ChatSystem;
  
  // Utilities
  reply: (content: string, type?: MessageType) => Promise<void>;
  sendToRoom: (content: string, roomId: string) => Promise<void>;
  sendPrivate: (content: string, targetId: string) => Promise<void>;
}

export interface ChatCommandResult {
  success: boolean;
  message?: string;
  data?: any;
  error?: string;
  
  // Side effects
  shouldReply?: boolean;
  replyContent?: string;
  replyType?: MessageType;
}

// ==================== CHAT WIDGET INTERFACE ====================

/**
 * Chat widget configuration - works with PersonaBase entities
 */
export interface ChatWidgetConfig {
  // Widget identity
  id: string;
  title: string;
  
  // Current user (PersonaBase entity)
  currentUser: PersonaBase;
  
  // Display settings
  theme: 'light' | 'dark' | 'auto';
  position: 'bottom-right' | 'bottom-left' | 'top-right' | 'top-left' | 'center';
  size: 'small' | 'medium' | 'large' | 'fullscreen';
  
  // Features
  features: ChatWidgetFeatures;
  
  // Integration
  roomId?: string;
  autoConnect?: boolean;
  
  // Callbacks
  onMessage?: (message: ChatMessage) => void;
  onCommand?: (context: ChatCommandContext) => void;
  onError?: (error: Error) => void;
}

export interface ChatWidgetFeatures {
  // Core features
  sendMessages: boolean;
  receiveMessages: boolean;
  showHistory: boolean;
  
  // Advanced features
  enableCommands: boolean;
  enableMentions: boolean;
  enableReactions: boolean;
  enableFileUpload: boolean;
  
  // Persona features
  showPersonaInfo: boolean;
  allowPersonaSwitching: boolean;
  showPersonaCapabilities: boolean;
  
  // UI features
  showTypingIndicator: boolean;
  showOnlineStatus: boolean;
  enableNotifications: boolean;
  enableSounds: boolean;
}

// ==================== CHAT SYSTEM ====================

/**
 * Main chat system - manages all chat interactions
 */
export interface ChatSystem {
  // Room management
  createRoom(config: Partial<ChatRoom>): Promise<ChatRoom>;
  joinRoom(roomId: string, participant: PersonaBase): Promise<boolean>;
  leaveRoom(roomId: string, participantId: string): Promise<boolean>;
  getRooms(): Promise<ChatRoom[]>;
  getRoom(roomId: string): Promise<ChatRoom | null>;
  
  // Message handling
  sendMessage(message: Omit<ChatMessage, 'id' | 'timestamp'>): Promise<ChatMessage>;
  getMessage(messageId: string): Promise<ChatMessage | null>;
  getMessages(roomId: string, options?: MessageQueryOptions): Promise<ChatMessage[]>;
  
  // Command system
  registerCommand(command: ChatCommand): void;
  executeCommand(context: ChatCommandContext): Promise<ChatCommandResult>;
  getCommands(): ChatCommand[];
  
  // Participant management
  getParticipant(id: string): Promise<PersonaBase | null>;
  getParticipants(roomId: string): Promise<PersonaBase[]>;
  
  // Widget management
  createWidget(config: ChatWidgetConfig): Promise<ChatWidget>;
  getWidget(widgetId: string): Promise<ChatWidget | null>;
  
  // Events
  on(event: ChatEvent, handler: ChatEventHandler): void;
  off(event: ChatEvent, handler: ChatEventHandler): void;
  emit(event: ChatEvent, data: any): void;
}

export interface MessageQueryOptions {
  limit?: number;
  offset?: number;
  before?: number;
  after?: number;
  fromUser?: string;
  messageType?: MessageType;
}

export type ChatEvent = 
  | 'message_sent'
  | 'message_received'
  | 'user_joined'
  | 'user_left'
  | 'command_executed'
  | 'room_created'
  | 'room_deleted'
  | 'widget_created'
  | 'error';

export type ChatEventHandler = (data: any) => void;

// ==================== CHAT WIDGET IMPLEMENTATION ====================

/**
 * Chat widget - UI component for chat interactions
 */
export interface ChatWidget {
  id: string;
  config: ChatWidgetConfig;
  
  // State management
  isConnected: boolean;
  currentRoom?: ChatRoom;
  messages: ChatMessage[];
  participants: PersonaBase[];
  
  // Core operations
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  sendMessage(content: string): Promise<void>;
  executeCommand(command: string): Promise<void>;
  
  // Persona operations
  switchPersona(persona: PersonaBase): Promise<void>;
  getCurrentPersona(): PersonaBase;
  
  // UI operations
  show(): void;
  hide(): void;
  minimize(): void;
  maximize(): void;
  
  // Events
  on(event: string, handler: Function): void;
  off(event: string, handler: Function): void;
}

// ==================== BUILT-IN COMMANDS ====================

/**
 * Standard chat commands that work with PersonaBase entities
 */
export const BUILT_IN_COMMANDS: ChatCommand[] = [
  {
    name: 'help',
    description: 'Show available commands',
    usage: '/help [command]',
    handler: async (context) => {
      const { args, chatSystem } = context;
      const commands = chatSystem.getCommands();
      
      if (args.length > 0) {
        const cmd = commands.find(c => c.name === args[0]);
        if (cmd) {
          await context.reply(`**${cmd.name}**: ${cmd.description}\nUsage: ${cmd.usage}`);
        } else {
          await context.reply(`Command '${args[0]}' not found.`);
        }
      } else {
        const commandList = commands.map(c => `**${c.name}**: ${c.description}`).join('\n');
        await context.reply(`Available commands:\n${commandList}`);
      }
      
      return { success: true };
    }
  },
  
  {
    name: 'who',
    description: 'Show information about current persona',
    usage: '/who [persona_id]',
    handler: async (context) => {
      const { args, sender } = context;
      
      if (args.length > 0) {
        const targetId = args[0];
        const participant = await context.chatSystem.getParticipant(targetId);
        if (participant) {
          await context.reply(`**${participant.name}** (${participant.type})\n${participant.description || 'No description'}`);
        } else {
          await context.reply(`Persona '${targetId}' not found.`);
        }
      } else {
        await context.reply(`**${sender.name}** (${sender.type})\n${sender.description || 'No description'}`);
      }
      
      return { success: true };
    }
  },
  
  {
    name: 'participants',
    description: 'List all participants in current room',
    usage: '/participants',
    requiresRoom: true,
    handler: async (context) => {
      const { room } = context;
      
      if (!room) {
        return { success: false, error: 'Not in a room' };
      }
      
      const participantList = room.participants
        .map(p => `• **${p.name}** (${p.type})`)
        .join('\n');
      
      await context.reply(`Participants in ${room.name}:\n${participantList}`);
      
      return { success: true };
    }
  },
  
  {
    name: 'create_persona',
    description: 'Create a new persona from a prompt',
    usage: '/create_persona <name> <prompt>',
    handler: async (context) => {
      const { args } = context;
      
      if (args.length < 2) {
        await context.reply('Usage: /create_persona <name> <prompt>');
        return { success: false, error: 'Invalid arguments' };
      }
      
      const name = args[0];
      const prompt = args.slice(1).join(' ');
      
      // This would integrate with the PersonaImporter
      await context.reply(`Creating persona '${name}' with prompt: "${prompt}"`);
      
      return { success: true };
    }
  }
];

// ==================== PERSONA CHAT EXTENSIONS ====================

/**
 * Extensions for PersonaBase to support chat functionality
 */
export interface ChatPersonaExtensions {
  // Chat-specific properties
  isOnline?: boolean;
  status?: 'available' | 'busy' | 'away' | 'offline';
  currentRoom?: string;
  
  // Chat history
  messageHistory?: ChatMessage[];
  lastSeen?: number;
  
  // Chat preferences
  chatPreferences?: {
    theme?: 'light' | 'dark';
    notifications?: boolean;
    sounds?: boolean;
    autoReply?: boolean;
  };
  
  // Command permissions
  commandPermissions?: string[];
  isModerator?: boolean;
}

/**
 * Enhanced PersonaBase for chat interactions
 */
export interface ChatPersona extends PersonaBase {
  // Chat extensions
  chat?: ChatPersonaExtensions;
  
  // Chat-specific methods
  sendChatMessage?(content: string, roomId?: string): Promise<void>;
  executeCommand?(command: string, args: string[]): Promise<ChatCommandResult>;
  joinRoom?(roomId: string): Promise<boolean>;
  leaveRoom?(roomId: string): Promise<boolean>;
}

// ==================== FACTORY FUNCTIONS ====================

/**
 * Create a chat room with default settings
 */
export function createChatRoom(name: string, creator: PersonaBase): ChatRoom {
  return {
    id: generateUUID(),
    name,
    description: `Chat room created by ${creator.name}`,
    created: Date.now(),
    participants: [creator],
    moderators: [creator.id],
    settings: {
      isPublic: true,
      requireInvite: false,
      allowGuests: true,
      moderationLevel: 'basic',
      allowCommands: true,
      commandPrefix: '/',
      enableFileSharing: true,
      enableReactions: true,
      enableThreads: true,
      enableMentions: true,
      maxParticipants: 100,
      maxMessageLength: 2000,
      rateLimitPerMinute: 60,
      persistHistory: true,
      historyRetentionDays: 30
    },
    isActive: true,
    lastActivity: Date.now(),
    messageCount: 0
  };
}

/**
 * Create a chat message
 */
export function createChatMessage(
  sender: PersonaBase,
  content: string,
  type: MessageType = 'text',
  roomId?: string
): ChatMessage {
  return {
    id: generateUUID(),
    sender,
    content,
    timestamp: Date.now(),
    type,
    roomId,
    attachments: [],
    reactions: [],
    mentions: []
  };
}

/**
 * Create a chat widget configuration
 */
export function createChatWidgetConfig(
  currentUser: PersonaBase,
  overrides?: Partial<ChatWidgetConfig>
): ChatWidgetConfig {
  return {
    id: generateUUID(),
    title: 'Chat',
    currentUser,
    theme: 'auto',
    position: 'bottom-right',
    size: 'medium',
    features: {
      sendMessages: true,
      receiveMessages: true,
      showHistory: true,
      enableCommands: true,
      enableMentions: true,
      enableReactions: true,
      enableFileUpload: false,
      showPersonaInfo: true,
      allowPersonaSwitching: true,
      showPersonaCapabilities: true,
      showTypingIndicator: true,
      showOnlineStatus: true,
      enableNotifications: true,
      enableSounds: false
    },
    autoConnect: true,
    ...overrides
  };
}

// ==================== EXPORTS ====================

export {
  ChatMessage,
  ChatRoom,
  ChatCommand,
  ChatCommandContext,
  ChatCommandResult,
  ChatSystem,
  ChatWidget,
  ChatWidgetConfig,
  ChatPersona,
  MessageType,
  ChatRoomSettings,
  ChatWidgetFeatures
};