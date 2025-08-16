/**
 * Chat Data Types - Shared Types & Interfaces
 * 
 * Universal types for chat storage operations across all environments
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface ChatRoom {
  roomId: UUID;
  name: string;
  description: string;
  category: string;
  allowAI: boolean;
  requireModeration: boolean;
  isPrivate: boolean;
  maxHistoryLength: number;
  createdAt: string;
  lastActivity: string;
}

export interface ChatMessage {
  messageId: UUID;
  roomId: UUID;
  citizenId: UUID;
  citizenName: string;
  citizenType: 'user' | 'ai';
  content: string;
  timestamp: string;
  messageType: 'text' | 'system' | 'action';
}

export interface ChatCitizen {
  citizenId: UUID;
  name: string;
  type: 'user' | 'ai';
  joinedAt: string;
  lastSeen: string;
  status: 'online' | 'offline' | 'away';
}

/**
 * Chat Data Service Interface - What all environments expect
 */
export interface IChatDataService {
  initialize(): Promise<void>;
  close(): Promise<void>;
  
  // Room operations
  createRoom(room: ChatRoom): Promise<ChatRoom>;
  getRoom(roomId: UUID): Promise<ChatRoom | null>;
  listRooms(): Promise<ChatRoom[]>;
  
  // Message operations
  saveMessage(message: ChatMessage): Promise<ChatMessage>;
  getMessages(roomId: UUID, limit?: number): Promise<ChatMessage[]>;
  
  // Citizen operations (future)
  // saveCitizen(citizen: ChatCitizen): Promise<ChatCitizen>;
  // getCitizen(citizenId: UUID): Promise<ChatCitizen | null>;
}