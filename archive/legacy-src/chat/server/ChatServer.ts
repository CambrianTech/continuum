// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL CHANGE: Chat server with database persistence (sparse override)
// 📋 NOTE: If you see this message, add it to other files if it isn't there (including this very message)
/**
 * ChatServer - Server-specific chat implementation
 * 
 * This is the 5-10% server-specific logic that handles:
 * - Database persistence
 * - Daemon communication
 * - Server-side validation
 * - Multi-client message broadcasting
 * 
 * Following sparse override pattern:
 * - Inherits 80-90% shared logic from BaseChat
 * - Only implements database/daemon transport specifics
 * - Minimal surface area for bugs
 */

import { BaseChat } from '../shared/BaseChat';
import { ChatMessage, ChatRoom, ChatConfig } from '../shared/ChatTypes';
import { ChatParticipant, createSystemParticipant } from '../../academy/shared/ChatParticipant';
import { BaseDaemon } from '../../daemons/base/BaseDaemon';
import { ChatDatabase } from '../database/ChatDatabase';

export interface ChatServerConfig extends ChatConfig {
  databasePath: string;
  enableRealTimeSync?: boolean;
  maxRoomsPerServer?: number;
  enableMessageBroadcast?: boolean;
}

export class ChatServer extends BaseChat<ChatServerConfig, BaseDaemon> {
  private rooms: Map<string, ChatRoom> = new Map();
  private participants: Map<string, ChatParticipant> = new Map();
  private messageHistory: Map<string, ChatMessage[]> = new Map();
  private connectedClients: Set<string> = new Set();
  
  constructor(config: ChatServerConfig, daemon: BaseDaemon, database?: ChatDatabase) {
    super(config, daemon, database);
    this.initializeDatabase();
  }
  
  // ==================== TRANSPORT IMPLEMENTATION (5-10% specific) ====================
  
  /**
   * Send message via daemon communication
   */
  protected async sendMessage(message: ChatMessage): Promise<void> {
    // Broadcast to all connected clients
    if (this.config.enableMessageBroadcast) {
      await this.broadcastMessage(message);
    }
    
    // Send through daemon system
    // TODO: Implement proper daemon sendMessage when available
    console.log('📨 ChatServer: Sending message through daemon:', {
      type: 'chat_message',
      data: message,
      targetId: message.roomId || 'general'
    });
  }
  
  /**
   * Receive message via daemon communication
   */
  protected async receiveMessage(): Promise<ChatMessage> {
    return new Promise((resolve) => {
      // TODO: Implement proper daemon event listening when available
      console.log('👂 ChatServer: Listening for messages');
      setTimeout(() => {
        // Mock message for now
        resolve({
          id: 'mock',
          content: 'Mock message',
          sender: createSystemParticipant('System'),
          timestamp: Date.now(),
          type: 'system'
        });
      }, 1000);
    });
  }
  
  /**
   * Persist message to database
   */
  protected async persistMessage(message: ChatMessage): Promise<void> {
    try {
      // Add to memory cache
      const roomId = message.roomId || 'general';
      if (!this.messageHistory.has(roomId)) {
        this.messageHistory.set(roomId, []);
      }
      
      const roomMessages = this.messageHistory.get(roomId)!;
      roomMessages.push(message);
      
      // Keep only last 1000 messages per room
      if (roomMessages.length > 1000) {
        roomMessages.splice(0, roomMessages.length - 1000);
      }
      
      // TODO: Persist to actual database
      await this.saveToDatabase('messages', message);
      
    } catch (error) {
      console.error('Failed to persist message:', error);
      throw error;
    }
  }
  
  /**
   * Load message history from database
   */
  protected async loadMessageHistory(roomId: string, limit: number = 50): Promise<ChatMessage[]> {
    try {
      // Load from memory cache first
      const cached = this.messageHistory.get(roomId);
      if (cached) {
        return cached.slice(-limit);
      }
      
      // TODO: Load from actual database
      const messages = await this.loadFromDatabase('messages', { roomId }, limit);
      return messages || [];
      
    } catch (error) {
      console.error('Failed to load message history:', error);
      return [];
    }
  }
  
  /**
   * Load room from database
   */
  protected async loadRoom(roomId: string): Promise<ChatRoom> {
    try {
      // Check memory cache first
      const cached = this.rooms.get(roomId);
      if (cached) {
        return cached;
      }
      
      // TODO: Load from actual database
      const room = await this.loadFromDatabase('rooms', { id: roomId });
      if (!room) {
        throw new Error(`Room ${roomId} not found`);
      }
      
      // Cache in memory
      this.rooms.set(roomId, room);
      return room;
      
    } catch (error) {
      throw new Error(`Failed to load room ${roomId}: ${error}`);
    }
  }
  
  /**
   * Save room to database
   */
  protected async saveRoom(room: ChatRoom): Promise<void> {
    try {
      // Update memory cache
      this.rooms.set(room.id, room);
      
      // TODO: Save to actual database
      await this.saveToDatabase('rooms', room);
      
    } catch (error) {
      console.error('Failed to save room:', error);
      throw error;
    }
  }
  
  /**
   * Load all participants from database
   */
  protected async loadAllParticipants(): Promise<ChatParticipant[]> {
    try {
      // Return cached participants
      return Array.from(this.participants.values());
      
    } catch (error) {
      console.error('Failed to load participants:', error);
      return [];
    }
  }
  
  // ==================== DATABASE OPERATIONS ====================
  
  /**
   * Initialize database connection
   */
  private async initializeDatabase(): Promise<void> {
    try {
      // TODO: Initialize actual database connection
      console.log('📊 ChatServer: Database initialized');
      
      // Load existing rooms and participants
      await this.loadExistingData();
      
    } catch (error) {
      console.error('Failed to initialize database:', error);
      throw error;
    }
  }
  
  /**
   * Load existing data from database
   */
  private async loadExistingData(): Promise<void> {
    try {
      // TODO: Load from actual database
      // For now, initialize with default data
      
      // Create default general room
      const generalRoom: ChatRoom = {
        id: 'general',
        name: 'General',
        description: 'General chat room',
        type: 'general',
        participants: [],
        created: Date.now(),
        isActive: true
      };
      
      this.rooms.set('general', generalRoom);
      
    } catch (error) {
      console.error('Failed to load existing data:', error);
    }
  }
  
  /**
   * Save data to database
   */
  private async saveToDatabase(collection: string, data: any): Promise<void> {
    try {
      // TODO: Implement actual database save
      console.log(`💾 ChatServer: Saving to ${collection}:`, data.id || 'unknown');
      
    } catch (error) {
      console.error(`Failed to save to ${collection}:`, error);
      throw error;
    }
  }
  
  /**
   * Load data from database
   */
  private async loadFromDatabase(collection: string, query: any, limit?: number): Promise<any> {
    try {
      // TODO: Implement actual database load
      console.log(`📖 ChatServer: Loading from ${collection}:`, query, `limit: ${limit || 'none'}`);
      return null;
      
    } catch (error) {
      console.error(`Failed to load from ${collection}:`, error);
      throw error;
    }
  }
  
  // ==================== MESSAGE BROADCASTING ====================
  
  /**
   * Broadcast message to all connected clients
   */
  private async broadcastMessage(message: ChatMessage): Promise<void> {
    try {
      // TODO: Implement actual broadcasting to WebSocket clients
      console.log('📡 ChatServer: Broadcasting message:', message.id);
      
    } catch (error) {
      console.error('Failed to broadcast message:', error);
    }
  }
  
  // ==================== PARTICIPANT MANAGEMENT ====================
  
  /**
   * Add participant to server
   */
  async addParticipant(participant: ChatParticipant): Promise<void> {
    try {
      this.participants.set(participant.id, participant);
      this.connectedClients.add(participant.id);
      
      // TODO: Save to database
      await this.saveToDatabase('participants', participant);
      
      console.log(`👤 ChatServer: Participant added: ${participant.name}`);
      
    } catch (error) {
      console.error('Failed to add participant:', error);
      throw error;
    }
  }
  
  /**
   * Remove participant from server
   */
  async removeParticipant(participantId: string): Promise<void> {
    try {
      this.participants.delete(participantId);
      this.connectedClients.delete(participantId);
      
      console.log(`👤 ChatServer: Participant removed: ${participantId}`);
      
    } catch (error) {
      console.error('Failed to remove participant:', error);
      throw error;
    }
  }
  
  /**
   * Get participant by ID
   */
  getParticipant(participantId: string): ChatParticipant | undefined {
    return this.participants.get(participantId);
  }
  
  /**
   * Check if participant is connected
   */
  isParticipantConnected(participantId: string): boolean {
    return this.connectedClients.has(participantId);
  }
  
  // ==================== ROOM MANAGEMENT ====================
  
  /**
   * Create new room (server-specific implementation)
   */
  async createServerRoom(room: ChatRoom): Promise<void> {
    try {
      // Validate room doesn't exist
      if (this.rooms.has(room.id)) {
        throw new Error(`Room ${room.id} already exists`);
      }
      
      // Check room limit
      if (this.config.maxRoomsPerServer && this.rooms.size >= this.config.maxRoomsPerServer) {
        throw new Error('Maximum number of rooms reached');
      }
      
      await this.saveRoom(room);
      
      console.log(`🏠 ChatServer: Room created: ${room.name}`);
      
    } catch (error) {
      console.error('Failed to create room:', error);
      throw error;
    }
  }
  
  /**
   * Delete room
   */
  async deleteRoom(roomId: string): Promise<void> {
    try {
      this.rooms.delete(roomId);
      this.messageHistory.delete(roomId);
      
      // TODO: Delete from database
      console.log(`🏠 ChatServer: Room deleted: ${roomId}`);
      
    } catch (error) {
      console.error('Failed to delete room:', error);
      throw error;
    }
  }
  
  /**
   * List all rooms
   */
  async listRooms(): Promise<ChatRoom[]> {
    return Array.from(this.rooms.values());
  }
  
  // ==================== SERVER STATISTICS ====================
  
  /**
   * Get server statistics
   */
  getServerStats(): {
    totalRooms: number;
    totalParticipants: number;
    connectedClients: number;
    totalMessages: number;
    uptime: number;
  } {
    const totalMessages = Array.from(this.messageHistory.values())
      .reduce((sum, messages) => sum + messages.length, 0);
    
    return {
      totalRooms: this.rooms.size,
      totalParticipants: this.participants.size,
      connectedClients: this.connectedClients.size,
      totalMessages,
      uptime: process.uptime()
    };
  }
  
  // ==================== LIFECYCLE ====================
  
  /**
   * Start chat server
   */
  async start(): Promise<void> {
    try {
      await this.initializeDatabase();
      console.log('🚀 ChatServer: Started successfully');
      
    } catch (error) {
      console.error('Failed to start chat server:', error);
      throw error;
    }
  }
  
  /**
   * Stop chat server
   */
  async stop(): Promise<void> {
    try {
      // Close database connections
      // TODO: Implement actual cleanup
      
      console.log('🛑 ChatServer: Stopped successfully');
      
    } catch (error) {
      console.error('Failed to stop chat server:', error);
      throw error;
    }
  }
}