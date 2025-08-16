/**
 * Chat Data Service - Server Implementation
 * 
 * Simple server-only chat storage using direct file operations
 * Follows the pattern from FileSaveServerCommand - server does actual work
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { IChatDataService, ChatRoom, ChatMessage } from '../shared/ChatDataTypes';

/**
 * Chat Data Service - Server Implementation
 * 
 * Direct file operations for chat storage (like FileSaveServerCommand pattern)
 */
export class ChatDataServiceServer implements IChatDataService {
  private dataDir: string;

  constructor() {
    this.dataDir = path.join(process.cwd(), 'examples/test-bench/.continuum/jtag/chat-data');
    this.ensureDataDirectory();
  }

  private async ensureDataDirectory(): Promise<void> {
    try {
      await fs.mkdir(this.dataDir, { recursive: true });
      await fs.mkdir(path.join(this.dataDir, 'rooms'), { recursive: true });
      await fs.mkdir(path.join(this.dataDir, 'messages'), { recursive: true });
    } catch (error) {
      console.error('Failed to create chat data directories:', error);
    }
  }

  async initialize(): Promise<void> {
    await this.ensureDataDirectory();
    console.log('💬 ChatDataServiceServer: Initialized with file storage');
  }

  async close(): Promise<void> {
    console.log('💬 ChatDataServiceServer: Closed');
  }

  /**
   * Create a new chat room
   */
  async createRoom(room: ChatRoom): Promise<ChatRoom> {
    const roomPath = path.join(this.dataDir, 'rooms', `${room.roomId}.json`);
    
    // Check if room already exists
    try {
      await fs.access(roomPath);
      throw new Error(`Room ${room.roomId} already exists`);
    } catch (error) {
      // Room doesn't exist, continue with creation
    }

    await fs.writeFile(roomPath, JSON.stringify(room, null, 2));
    console.log(`🏠 ChatDataServiceServer: Created room '${room.name}' (${room.roomId})`);
    
    return room;
  }

  /**
   * Get room by ID
   */
  async getRoom(roomId: UUID): Promise<ChatRoom | null> {
    const roomPath = path.join(this.dataDir, 'rooms', `${roomId}.json`);
    
    try {
      const roomData = await fs.readFile(roomPath, 'utf-8');
      return JSON.parse(roomData) as ChatRoom;
    } catch (error) {
      return null;
    }
  }

  /**
   * List all rooms
   */
  async listRooms(): Promise<ChatRoom[]> {
    const roomsDir = path.join(this.dataDir, 'rooms');
    
    try {
      const files = await fs.readdir(roomsDir);
      const rooms: ChatRoom[] = [];
      
      for (const file of files) {
        if (file.endsWith('.json')) {
          try {
            const roomData = await fs.readFile(path.join(roomsDir, file), 'utf-8');
            rooms.push(JSON.parse(roomData) as ChatRoom);
          } catch (error) {
            console.warn(`Failed to read room file ${file}:`, error);
          }
        }
      }
      
      return rooms;
    } catch (error) {
      return [];
    }
  }

  /**
   * Save a chat message
   */
  async saveMessage(message: ChatMessage): Promise<ChatMessage> {
    const messagePath = path.join(this.dataDir, 'messages', `${message.messageId}.json`);
    
    await fs.writeFile(messagePath, JSON.stringify(message, null, 2));
    console.log(`💬 ChatDataServiceServer: Saved message from ${message.citizenName}`);
    
    return message;
  }

  /**
   * Get messages for a room
   */
  async getMessages(roomId: UUID, limit: number = 50): Promise<ChatMessage[]> {
    const messagesDir = path.join(this.dataDir, 'messages');
    
    try {
      const files = await fs.readdir(messagesDir);
      const messages: ChatMessage[] = [];
      
      for (const file of files) {
        if (file.endsWith('.json')) {
          try {
            const messageData = await fs.readFile(path.join(messagesDir, file), 'utf-8');
            const message = JSON.parse(messageData) as ChatMessage;
            
            if (message.roomId === roomId) {
              messages.push(message);
            }
          } catch (error) {
            console.warn(`Failed to read message file ${file}:`, error);
          }
        }
      }
      
      // Sort by timestamp and apply limit
      messages.sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime());
      
      return messages.slice(-limit);
    } catch (error) {
      return [];
    }
  }
}

/**
 * Factory function - what the tests expect
 */
export function createChatDataService(testId?: string): ChatDataServiceServer {
  return new ChatDataServiceServer();
}