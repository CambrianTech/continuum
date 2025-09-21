/**
 * Room Data Seeding - Centralized Chat Room Creation
 * 
 * Creates initial chat rooms using existing JSON data.
 * Uses proper JTAG data commands instead of manual filesystem calls.
 */

import * as fs from 'fs';
import * as path from 'path';
import { USER_IDS, ROOM_IDS, MESSAGE_IDS } from './SeedConstants';

// Rust-like branded type for strict typing
export type RoomId = string & { readonly __brand: 'RoomId' };

export function createRoomId(id: string): RoomId {
  if (!id || id.trim().length === 0) {
    throw new Error('RoomId cannot be empty');
  }
  return id as RoomId;
}

export interface RoomSeedData {
  readonly rooms: readonly Room[];
  readonly totalCount: number;
  readonly createdAt: string;
}

export interface Room {
  readonly id: RoomId;
  readonly name: string;
  readonly description: string;
  readonly isPublic: boolean;
  readonly allowAI: boolean;
  readonly requireModeration: boolean;
  readonly maxHistoryLength: number;
  readonly createdBy: string;
  readonly createdAt: string;
  readonly members: readonly string[];
  readonly category?: string;
}

export interface RoomMessage {
  readonly id: string;
  readonly roomId: RoomId;
  readonly userId: string;
  readonly content: string;
  readonly timestamp: string;
  readonly type: 'text' | 'system';
}

export class RoomDataSeed {
  private static readonly COLLECTION = 'rooms' as const;
  private static readonly MESSAGE_COLLECTION = 'messages' as const;
  
  /**
   * Generate all seed rooms using existing JSON data
   */
  public static generateSeedRooms(): RoomSeedData {
    // Load existing room configurations
    const initialRoomsPath = path.join(process.cwd(), 'data', 'initial-chat-rooms.json');
    const chatRoomsPath = path.join(process.cwd(), 'data', 'chat-rooms-initial.json');
    
    let rooms: Room[] = [];
    const now = new Date().toISOString();
    
    try {
      // Try loading initial-chat-rooms.json first
      if (fs.existsSync(initialRoomsPath)) {
        const initialRoomsData = JSON.parse(fs.readFileSync(initialRoomsPath, 'utf-8'));
        
        for (const room of initialRoomsData) {
          rooms.push({
            id: createRoomId(`room-${room.category || room.name.toLowerCase()}`),
            name: room.name,
            description: room.description,
            isPublic: !room.isPrivate,
            allowAI: room.allowAI,
            requireModeration: room.requireModeration,
            maxHistoryLength: room.maxHistoryLength,
            createdBy: USER_IDS.HUMAN,
            createdAt: now,
            members: [USER_IDS.HUMAN], // Joel is in all rooms initially
            category: room.category
          });
        }
      }
      
      // Also try loading chat-rooms-initial.json
      if (fs.existsSync(chatRoomsPath)) {
        const chatRoomsData = JSON.parse(fs.readFileSync(chatRoomsPath, 'utf-8'));
        
        for (const room of chatRoomsData.rooms) {
          // Don't duplicate if we already have this room
          const existingRoom = rooms.find(r => r.name.toLowerCase() === room.name.toLowerCase());
          if (!existingRoom) {
            rooms.push({
              id: createRoomId(`room-${room.name.toLowerCase()}`),
              name: room.name,
              description: room.description,
              isPublic: !room.isPrivate,
              allowAI: room.allowAI,
              requireModeration: room.requireModeration,
              maxHistoryLength: room.maxHistoryLength,
              createdBy: USER_IDS.HUMAN,
              createdAt: now,
              members: [USER_IDS.HUMAN],
              category: room.category
            });
          }
        }
      }
      
    } catch (error: any) {
      console.warn('⚠️ Could not load existing room JSON files, using defaults:', error.message);
    }
    
    // If no rooms loaded from JSON, create defaults
    if (rooms.length === 0) {
      rooms = [
        {
          id: createRoomId(ROOM_IDS.GENERAL),
          name: 'General',
          description: 'Main discussion room for all users',
          isPublic: true,
          allowAI: true,
          requireModeration: false,
          maxHistoryLength: 1000,
          createdBy: USER_IDS.HUMAN,
          createdAt: now,
          members: [USER_IDS.HUMAN],
          category: 'general'
        },
        {
          id: createRoomId(ROOM_IDS.ACADEMY),
          name: 'Academy',
          description: 'Learning and educational discussions',
          isPublic: true,
          allowAI: true,
          requireModeration: false,
          maxHistoryLength: 500,
          createdBy: USER_IDS.HUMAN,
          createdAt: now,
          members: [USER_IDS.HUMAN],
          category: 'education'
        }
      ];
    }
    
    // Add AI agents to appropriate rooms
    const allAIAgents = [
      USER_IDS.CLAUDE_CODE,
      USER_IDS.GENERAL_AI,
      USER_IDS.CODE_AI,
      USER_IDS.PLANNER_AI,
      USER_IDS.AUTO_ROUTE
    ];
    
    for (const room of rooms) {
      if (room.allowAI) {
        // Add all AI agents to AI-enabled rooms
        (room as any).members = [...room.members, ...allAIAgents];
      }
    }
    
    return {
      rooms: rooms as readonly Room[],
      totalCount: rooms.length,
      createdAt: now
    };
  }

  /**
   * Generate initial welcome messages for rooms
   */
  public static generateSeedMessages(): RoomMessage[] {
    const messages: RoomMessage[] = [];
    const now = new Date().toISOString();
    
    // Welcome message for general room
    messages.push({
      id: MESSAGE_IDS.WELCOME_GENERAL,
      roomId: createRoomId(ROOM_IDS.GENERAL),
      userId: USER_IDS.HUMAN,
      content: 'Welcome to the General room! This is where we discuss development, collaborate, and share ideas.',
      timestamp: now,
      type: 'text'
    });

    // Claude introduction
    messages.push({
      id: MESSAGE_IDS.CLAUDE_INTRO,
      roomId: createRoomId(ROOM_IDS.GENERAL),
      userId: USER_IDS.CLAUDE_CODE,
      content: 'Hello! I\'m Claude Code, your AI development assistant. I can help with TypeScript, React, debugging, and system architecture. Feel free to ask me anything!',
      timestamp: new Date(Date.now() + 1000).toISOString(),
      type: 'text'
    });

    // Academy welcome if it exists
    messages.push({
      id: MESSAGE_IDS.WELCOME_ACADEMY,
      roomId: createRoomId(ROOM_IDS.ACADEMY),
      userId: USER_IDS.HUMAN,
      content: 'Welcome to the Academy! This room is for learning, tutorials, and educational discussions.',
      timestamp: new Date(Date.now() + 2000).toISOString(),
      type: 'text'
    });
    
    return messages;
  }

  /**
   * Validate room data - crash and burn on invalid data, no fallbacks
   */
  private static validateRoom(room: Room): void {
    if (!room.id) {
      throw new Error(`Room missing required id: ${JSON.stringify(room)}`);
    }
    if (!room.name || room.name.trim().length === 0) {
      throw new Error(`Room ${room.id} missing required name`);
    }
    if (typeof room.isPublic !== 'boolean') {
      throw new Error(`Room ${room.id} has invalid isPublic value: ${room.isPublic}`);
    }
    if (!Array.isArray(room.members)) {
      throw new Error(`Room ${room.id} has invalid members array: ${room.members}`);
    }
  }

  /**
   * Create JTAG data/create command parameters for room
   */
  public static createRoomDataCommand(room: Room): DataCreateCommand {
    this.validateRoom(room); // Crash and burn on invalid data
    
    return {
      collection: this.COLLECTION,
      data: room,
      id: room.id
    };
  }

  /**
   * Create JTAG data/create command parameters for message
   */
  public static createMessageDataCommand(message: RoomMessage): DataCreateCommand {
    if (!message.id || !message.roomId || !message.userId || !message.content) {
      throw new Error(`Message missing required fields: ${JSON.stringify(message)}`);
    }
    
    return {
      collection: this.MESSAGE_COLLECTION,
      data: message,
      id: message.id
    };
  }
}

// Type-safe data create command (shared with UserDataSeed)
export interface DataCreateCommand {
  readonly collection: string;
  readonly data: unknown;
  readonly id?: string;
}

export default RoomDataSeed;