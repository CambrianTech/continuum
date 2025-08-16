/**
 * Chat Data Service - Browser Implementation
 * 
 * Browser delegates all data operations to server (can't do file I/O directly)
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { IChatDataService, ChatRoom, ChatMessage } from '../shared/ChatDataTypes';

/**
 * Chat Data Service - Browser Implementation (Delegation)
 * 
 * Browser can't do file I/O, so this would delegate to server via commands
 * For now, this is a placeholder that throws meaningful errors
 */
export class ChatDataServiceBrowser implements IChatDataService {
  
  async initialize(): Promise<void> {
    throw new Error('ChatDataServiceBrowser: Browser cannot initialize database directly - use server implementation');
  }

  async close(): Promise<void> {
    console.log('💬 ChatDataServiceBrowser: No-op close');
  }

  async createRoom(_room: ChatRoom): Promise<ChatRoom> {
    throw new Error('ChatDataServiceBrowser: Browser cannot create rooms directly - use chat commands');
  }

  async getRoom(_roomId: UUID): Promise<ChatRoom | null> {
    throw new Error('ChatDataServiceBrowser: Browser cannot read rooms directly - use chat commands');
  }

  async listRooms(): Promise<ChatRoom[]> {
    throw new Error('ChatDataServiceBrowser: Browser cannot list rooms directly - use chat commands');
  }

  async saveMessage(_message: ChatMessage): Promise<ChatMessage> {
    throw new Error('ChatDataServiceBrowser: Browser cannot save messages directly - use chat commands');
  }

  async getMessages(_roomId: UUID, _limit?: number): Promise<ChatMessage[]> {
    throw new Error('ChatDataServiceBrowser: Browser cannot get messages directly - use chat commands');
  }
}