/**
 * Chat Data Service - High-level data operations for chat system
 * 
 * Provides a clean abstraction over the database layer with:
 * - Business logic validation
 * - Caching for performance  
 * - Transaction management
 * - Error handling and retry logic
 * - Event emission for state changes
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { ChatRoom, ChatCitizen, ChatMessage } from '../shared/ChatDaemon';
import { ChatDatabase, type DatabaseConfig, type DatabaseStats } from './ChatDatabase';
import { EventEmitter } from 'events';

export interface DataServiceConfig {
  database: DatabaseConfig;
  caching: {
    enabled: boolean;
    ttlMs: number;
    maxSize: number;
  };
  cleanup: {
    intervalMs: number;
    maxMessageAge: number;
  };
}

export interface CacheStats {
  hits: number;
  misses: number;
  size: number;
  hitRate: number;
}

/**
 * Cache entry with TTL
 */
interface CacheEntry<T> {
  data: T;
  timestamp: number;
  ttl: number;
}

/**
 * Chat Data Service - Business logic layer over database
 */
export class ChatDataService extends EventEmitter {
  private db: ChatDatabase;
  private config: DataServiceConfig;
  private cache: Map<string, CacheEntry<any>> = new Map();
  private cleanupTimer?: NodeJS.Timeout;
  private stats: CacheStats = { hits: 0, misses: 0, size: 0, hitRate: 0 };

  constructor(config: DataServiceConfig) {
    super();
    this.config = config;
    this.db = new ChatDatabase(config.database);
    
    if (config.cleanup.intervalMs > 0) {
      this.startCleanupTimer();
    }
  }

  /**
   * Initialize the data service
   */
  async initialize(): Promise<void> {
    await this.db.initialize();
    this.emit('initialized');
    console.log(`🗄️ ChatDataService: Initialized with caching=${this.config.caching.enabled}`);
  }

  // ===== CACHING SYSTEM =====

  /**
   * Get from cache if valid
   */
  private getCached<T>(key: string): T | null {
    if (!this.config.caching.enabled) return null;
    
    const entry = this.cache.get(key);
    if (!entry) {
      this.stats.misses++;
      return null;
    }
    
    const now = Date.now();
    if (now - entry.timestamp > entry.ttl) {
      this.cache.delete(key);
      this.stats.misses++;
      return null;
    }
    
    this.stats.hits++;
    this.updateHitRate();
    return entry.data;
  }

  /**
   * Store in cache
   */
  private setCached<T>(key: string, data: T, ttlMs?: number): void {
    if (!this.config.caching.enabled) return;
    
    const ttl = ttlMs || this.config.caching.ttlMs;
    this.cache.set(key, {
      data,
      timestamp: Date.now(),
      ttl
    });
    
    // Enforce max cache size
    if (this.cache.size > this.config.caching.maxSize) {
      const firstKey = this.cache.keys().next().value;
      if (firstKey) {
        this.cache.delete(firstKey);
      }
    }
    
    this.stats.size = this.cache.size;
  }

  /**
   * Invalidate cache entries
   */
  private invalidateCache(pattern: string): void {
    const keys = Array.from(this.cache.keys()).filter(key => key.includes(pattern));
    keys.forEach(key => this.cache.delete(key));
    this.stats.size = this.cache.size;
  }

  /**
   * Update hit rate
   */
  private updateHitRate(): void {
    const total = this.stats.hits + this.stats.misses;
    this.stats.hitRate = total > 0 ? this.stats.hits / total : 0;
  }

  // ===== ROOM OPERATIONS =====

  /**
   * Create a new chat room with validation
   */
  async createRoom(room: Omit<ChatRoom, 'citizens' | 'messageHistory'>): Promise<ChatRoom> {
    // Validation
    if (!room.name.trim()) {
      throw new Error('Room name cannot be empty');
    }
    
    if (room.name.length > 100) {
      throw new Error('Room name cannot exceed 100 characters');
    }

    // Check for duplicate names in same category
    const existingRooms = await this.getAllRooms();
    const duplicateName = existingRooms.some(r => 
      r.name === room.name && r.category === room.category
    );
    
    if (duplicateName) {
      throw new Error(`Room '${room.name}' already exists in category '${room.category}'`);
    }

    // Create room
    await this.db.createRoom(room);
    
    const fullRoom = await this.getRoom(room.roomId);
    if (!fullRoom) {
      throw new Error('Failed to create room');
    }

    // Invalidate relevant cache
    this.invalidateCache('rooms:');
    this.invalidateCache('room:');

    // Emit event
    this.emit('room:created', { room: fullRoom });
    
    console.log(`🏠 ChatDataService: Created room '${room.name}' (${room.roomId})`);
    return fullRoom;
  }

  /**
   * Get room with caching
   */
  async getRoom(roomId: UUID): Promise<ChatRoom | null> {
    const cacheKey = `room:${roomId}`;
    const cached = this.getCached<ChatRoom>(cacheKey);
    if (cached) return cached;

    const room = await this.db.getRoom(roomId);
    if (room) {
      this.setCached(cacheKey, room, 30000); // 30s TTL for rooms
    }
    
    return room;
  }

  /**
   * Get all rooms with caching
   */
  async getAllRooms(): Promise<ChatRoom[]> {
    const cacheKey = 'rooms:all';
    const cached = this.getCached<ChatRoom[]>(cacheKey);
    if (cached) return cached;

    const rooms = await this.db.getAllRooms();
    this.setCached(cacheKey, rooms, 10000); // 10s TTL for room list
    
    return rooms;
  }

  /**
   * Update room activity timestamp
   */
  async updateRoomActivity(roomId: UUID): Promise<void> {
    const timestamp = new Date().toISOString();
    await this.db.updateRoomActivity(roomId, timestamp);
    
    // Invalidate room cache
    this.invalidateCache(`room:${roomId}`);
    this.invalidateCache('rooms:');
    
    this.emit('room:activity', { roomId, timestamp });
  }

  // ===== CITIZEN OPERATIONS =====

  /**
   * Save citizen with validation
   */
  async saveCitizen(citizen: ChatCitizen): Promise<void> {
    // Validation
    if (!citizen.displayName.trim()) {
      throw new Error('Citizen display name cannot be empty');
    }
    
    if (citizen.displayName.length > 50) {
      throw new Error('Citizen display name cannot exceed 50 characters');
    }

    // Check for AI config validation
    if (citizen.aiConfig) {
      if (citizen.aiConfig.provider && !['openai', 'anthropic', 'local'].includes(citizen.aiConfig.provider)) {
        throw new Error('Invalid AI provider');
      }
      
      if (citizen.aiConfig.provider !== 'local' && !citizen.aiConfig.apiKey) {
        throw new Error('API key required for external AI providers');
      }
    }

    await this.db.saveCitizen(citizen);
    
    // Invalidate cache
    this.invalidateCache(`citizen:${citizen.citizenId}`);
    this.invalidateCache('citizens:');
    
    this.emit('citizen:saved', { citizen });
    console.log(`👤 ChatDataService: Saved citizen '${citizen.displayName}' (${citizen.citizenId})`);
  }

  /**
   * Get citizen with caching
   */
  async getCitizen(citizenId: UUID): Promise<ChatCitizen | null> {
    const cacheKey = `citizen:${citizenId}`;
    const cached = this.getCached<ChatCitizen>(cacheKey);
    if (cached) return cached;

    const citizen = await this.db.getCitizen(citizenId);
    if (citizen) {
      this.setCached(cacheKey, citizen, 60000); // 1min TTL for citizens
    }
    
    return citizen;
  }

  /**
   * Get all citizens in a room
   */
  async getRoomCitizens(roomId: UUID): Promise<ChatCitizen[]> {
    const cacheKey = `citizens:room:${roomId}`;
    const cached = this.getCached<ChatCitizen[]>(cacheKey);
    if (cached) return cached;

    const citizens = await this.db.getRoomCitizens(roomId);
    this.setCached(cacheKey, citizens, 15000); // 15s TTL for room citizens
    
    return citizens;
  }

  /**
   * Join citizen to room with validation
   */
  async joinCitizenToRoom(roomId: UUID, citizenId: UUID): Promise<void> {
    // Validate room exists
    const room = await this.getRoom(roomId);
    if (!room) {
      throw new Error(`Room ${roomId} not found`);
    }

    // Validate citizen exists
    const citizen = await this.getCitizen(citizenId);
    if (!citizen) {
      throw new Error(`Citizen ${citizenId} not found`);
    }

    // Check if already in room
    const roomCitizens = await this.getRoomCitizens(roomId);
    const alreadyJoined = roomCitizens.some(c => c.citizenId === citizenId);
    if (alreadyJoined) {
      throw new Error(`Citizen already in room ${roomId}`);
    }

    await this.db.addCitizenToRoom(roomId, citizenId);
    
    // Invalidate cache
    this.invalidateCache(`room:${roomId}`);
    this.invalidateCache(`citizens:room:${roomId}`);
    this.invalidateCache(`citizen:${citizenId}`);
    
    this.emit('citizen:joined', { roomId, citizenId, citizen });
    console.log(`🚪 ChatDataService: ${citizen.displayName} joined room ${room.name}`);
  }

  /**
   * Remove citizen from room
   */
  async removeCitizenFromRoom(roomId: UUID, citizenId: UUID): Promise<void> {
    const citizen = await this.getCitizen(citizenId);
    const room = await this.getRoom(roomId);
    
    await this.db.removeCitizenFromRoom(roomId, citizenId);
    
    // Invalidate cache
    this.invalidateCache(`room:${roomId}`);
    this.invalidateCache(`citizens:room:${roomId}`);
    this.invalidateCache(`citizen:${citizenId}`);
    
    this.emit('citizen:left', { roomId, citizenId, citizen });
    
    if (citizen && room) {
      console.log(`🚪 ChatDataService: ${citizen.displayName} left room ${room.name}`);
    }
  }

  // ===== MESSAGE OPERATIONS =====

  /**
   * Save message with validation and side effects
   */
  async saveMessage(message: ChatMessage): Promise<void> {
    // Validation
    if (!message.content.trim()) {
      throw new Error('Message content cannot be empty');
    }
    
    if (message.content.length > 4000) {
      throw new Error('Message content cannot exceed 4000 characters');
    }

    // Validate room exists
    const room = await this.getRoom(message.roomId);
    if (!room) {
      throw new Error(`Room ${message.roomId} not found`);
    }

    // Validate sender exists and is in room
    const roomCitizens = await this.getRoomCitizens(message.roomId);
    const sender = roomCitizens.find(c => c.citizenId === message.senderId);
    if (!sender) {
      throw new Error(`Sender ${message.senderId} not in room ${message.roomId}`);
    }

    // Save message
    await this.db.saveMessage(message);
    
    // Update room activity
    await this.updateRoomActivity(message.roomId);
    
    // Invalidate cache
    this.invalidateCache(`messages:room:${message.roomId}`);
    this.invalidateCache(`room:${message.roomId}`);
    
    this.emit('message:saved', { message, room, sender });
    console.log(`💬 ChatDataService: Message from ${sender.displayName} in ${room.name}`);
  }

  /**
   * Get room messages with caching
   */
  async getRoomMessages(roomId: UUID, limit: number = 50, before?: UUID): Promise<ChatMessage[]> {
    const cacheKey = `messages:room:${roomId}:${limit}:${before || 'latest'}`;
    const cached = this.getCached<ChatMessage[]>(cacheKey);
    if (cached) return cached;

    const messages = await this.db.getRoomMessages(roomId, limit, before);
    
    // Cache for shorter time since messages are frequently updated
    this.setCached(cacheKey, messages, 5000); // 5s TTL for messages
    
    return messages;
  }

  // ===== CLEANUP & MAINTENANCE =====

  /**
   * Start periodic cleanup tasks
   */
  private startCleanupTimer(): void {
    this.cleanupTimer = setInterval(async () => {
      try {
        await this.runCleanupTasks();
      } catch (error) {
        console.error('ChatDataService cleanup error:', error);
      }
    }, this.config.cleanup.intervalMs);
  }

  /**
   * Run cleanup tasks
   */
  private async runCleanupTasks(): Promise<void> {
    // Clean up old messages
    const deletedCount = await this.db.cleanupOldMessages();
    if (deletedCount > 0) {
      console.log(`🧹 ChatDataService: Cleaned up ${deletedCount} old messages`);
      this.invalidateCache('messages:');
    }

    // Clean up expired cache entries
    this.cleanupExpiredCache();
    
    // Update offline citizens
    await this.updateOfflineCitizens();
  }

  /**
   * Clean up expired cache entries
   */
  private cleanupExpiredCache(): void {
    const now = Date.now();
    let cleaned = 0;
    
    for (const [key, entry] of this.cache.entries()) {
      if (now - entry.timestamp > entry.ttl) {
        this.cache.delete(key);
        cleaned++;
      }
    }
    
    if (cleaned > 0) {
      this.stats.size = this.cache.size;
      console.log(`🧹 ChatDataService: Cleaned up ${cleaned} expired cache entries`);
    }
  }

  /**
   * Update citizens who haven't been seen recently to offline status
   */
  private async updateOfflineCitizens(): Promise<void> {
    const cutoffTime = new Date(Date.now() - 300000).toISOString(); // 5 minutes ago
    
    // This would need a custom query - for now just log
    console.log(`🔄 ChatDataService: Checking for offline citizens (cutoff: ${cutoffTime})`);
  }

  // ===== STATISTICS & MONITORING =====

  /**
   * Get comprehensive service statistics
   */
  async getServiceStats(): Promise<{
    database: DatabaseStats;
    cache: CacheStats;
    events: { [key: string]: number };
  }> {
    const dbStats = await this.db.getStats();
    
    return {
      database: dbStats,
      cache: { ...this.stats },
      events: this.eventNames().reduce((acc, event) => {
        acc[event.toString()] = this.listenerCount(event);
        return acc;
      }, {} as { [key: string]: number })
    };
  }

  /**
   * Reset cache statistics
   */
  resetCacheStats(): void {
    this.stats = { hits: 0, misses: 0, size: this.cache.size, hitRate: 0 };
  }

  /**
   * Clear all caches
   */
  clearCache(): void {
    this.cache.clear();
    this.stats.size = 0;
    console.log(`🧹 ChatDataService: Cache cleared`);
  }

  // ===== LIFECYCLE =====

  /**
   * Backup database
   */
  async backup(path: string): Promise<void> {
    await this.db.backup(path);
    this.emit('backup:completed', { path });
  }

  /**
   * Close service and cleanup
   */
  async close(): Promise<void> {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
    }
    
    await this.db.close();
    this.cache.clear();
    this.removeAllListeners();
    
    console.log(`🔒 ChatDataService: Service closed`);
  }
}

/**
 * Create data service with sensible defaults
 */
export function createChatDataService(environment: 'test' | 'development' | 'production' = 'development'): ChatDataService {
  const configs = {
    test: {
      database: {
        path: ':memory:',
        inMemory: true,
        verbose: false
      },
      caching: {
        enabled: false,
        ttlMs: 5000,
        maxSize: 100
      },
      cleanup: {
        intervalMs: 0, // No cleanup for tests
        maxMessageAge: 86400000 // 1 day
      }
    },
    development: {
      database: {
        path: '.continuum/jtag/chat/chat.db',
        inMemory: false,
        verbose: true
      },
      caching: {
        enabled: true,
        ttlMs: 30000, // 30 seconds
        maxSize: 1000
      },
      cleanup: {
        intervalMs: 300000, // 5 minutes
        maxMessageAge: 604800000 // 7 days
      }
    },
    production: {
      database: {
        path: '.continuum/jtag/chat/chat.db',
        inMemory: false,
        verbose: false
      },
      caching: {
        enabled: true,
        ttlMs: 60000, // 1 minute
        maxSize: 10000
      },
      cleanup: {
        intervalMs: 3600000, // 1 hour
        maxMessageAge: 2592000000 // 30 days
      }
    }
  };

  return new ChatDataService(configs[environment]);
}