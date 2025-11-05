/**
 * Strong TypeScript types for chat event configuration
 * Loaded from chat-events-config.json but with proper type safety
 */

export interface ChatEventHandler {
  handler: string;
  handlerArgs?: any[];
  description: string;
  matchField?: string;
  requiresCurrentRoom?: boolean;
}

export interface GlobalEventHandler {
  handler: string;
  description: string;
  dataPath?: string;
  once?: boolean;
}

export interface MessageTypeConfig {
  icon: string;
  className: string;
  showAvatar: boolean;
  allowEdit: boolean;
}

export interface MessageStatusConfig {
  icon: string;
  className: string;
  description: string;
}

export interface ChatEventsConfig {
  chatEvents: Record<string, ChatEventHandler>;
  globalEvents: Record<string, GlobalEventHandler>;
  messageTypes: Record<string, MessageTypeConfig>;
  messageStatuses: Record<string, MessageStatusConfig>;
}

/**
 * Event data interfaces for incoming chat events
 */
export interface ChatEventData {
  roomId?: string;
  messageId?: string;
  timestamp?: string;
  [key: string]: any;
}

export interface IncomingMessageData extends ChatEventData {
  roomId: string;
  id: string;
  type: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: string;
  metadata?: {
    agent?: string;
    persona?: string;
  };
}

export interface TypingEventData extends ChatEventData {
  roomId: string;
  userId?: string;
  agentId?: string;
}

export interface ParticipantsUpdatedData extends ChatEventData {
  roomId: string;
  participants: Array<{
    id: string;
    name: string;
    type: 'user' | 'ai' | 'system';
    status: 'online' | 'away' | 'offline';
  }>;
}

export interface MessageStatusData extends ChatEventData {
  messageId: string;
  roomId: string;
  status: 'sending' | 'sent' | 'delivered' | 'error';
  error?: string;
}