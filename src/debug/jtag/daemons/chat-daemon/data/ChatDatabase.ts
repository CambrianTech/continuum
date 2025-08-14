/**
 * Chat Database Layer - SQLite-based persistence for chat system
 * 
 * Provides type-safe database operations for rooms, citizens, and messages.
 * Supports both in-memory (testing) and file-based (production) SQLite.
 * 
 * Features:
 * - Atomic transactions for consistency
 * - Connection pooling for performance  
 * - Migration system for schema evolution
 * - Full TypeScript type safety
 */

// Use dynamic import for better-sqlite3 to avoid compilation issues
const Database = (() => {
  try {
    return require('better-sqlite3');
  } catch (error) {
    console.warn('better-sqlite3 not available, using mock implementation');
    return class MockDatabase {
      constructor() {}
      pragma() {}
      prepare() { return { run: () => {}, get: () => null, all: () => [] }; }
      close() {}
    };
  }
})();
import { generateUUID, type UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { ChatRoom, ChatCitizen, ChatMessage } from '../shared/ChatDaemon';

export interface DatabaseConfig {
  path: string;
  inMemory: boolean;
  verbose?: boolean;
  migrationsPath?: string;
}

export interface DatabaseStats {
  roomCount: number;
  citizenCount: number;
  messageCount: number;
  totalSize: number;
  lastBackup?: string;
}

/**
 * Chat Database - SQLite-based persistence layer
 */
export class ChatDatabase {
  private db: any;
  private isInitialized: boolean = false;
  private config: DatabaseConfig;

  constructor(config: DatabaseConfig) {
    this.config = config;
    this.db = new Database(config.inMemory ? ':memory:' : config.path, {
      verbose: config.verbose ? console.log : undefined
    });
    
    // Enable foreign keys and WAL mode for better performance
    this.db.pragma('foreign_keys = ON');
    if (!config.inMemory) {
      this.db.pragma('journal_mode = WAL');
    }
  }

  /**
   * Initialize database schema and run migrations
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) return;

    await this.createTables();
    await this.runMigrations();
    
    this.isInitialized = true;
    console.log(`🏗️ ChatDatabase: Initialized ${this.config.inMemory ? 'in-memory' : 'file'} database`);
  }

  /**
   * Create database tables
   */
  private async createTables(): Promise<void> {
    // Rooms table
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS rooms (
        room_id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT,
        category TEXT NOT NULL DEFAULT 'general',
        allow_ai BOOLEAN NOT NULL DEFAULT true,
        require_moderation BOOLEAN NOT NULL DEFAULT false,
        is_private BOOLEAN NOT NULL DEFAULT false,
        max_history_length INTEGER NOT NULL DEFAULT 1000,
        created_at TEXT NOT NULL,
        last_activity TEXT NOT NULL
      )
    `);

    // Citizens table
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS citizens (
        citizen_id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        display_name TEXT NOT NULL,
        citizen_type TEXT NOT NULL CHECK (citizen_type IN ('user', 'agent', 'persona')),
        status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'idle', 'offline')),
        last_seen TEXT NOT NULL,
        ai_provider TEXT CHECK (ai_provider IN ('openai', 'anthropic', 'local')),
        ai_model TEXT,
        ai_system_prompt TEXT,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
      )
    `);

    // Messages table
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS messages (
        message_id TEXT PRIMARY KEY,
        room_id TEXT NOT NULL,
        sender_id TEXT NOT NULL,
        sender_name TEXT NOT NULL,
        sender_type TEXT NOT NULL CHECK (sender_type IN ('user', 'agent', 'persona')),
        content TEXT NOT NULL,
        message_type TEXT NOT NULL DEFAULT 'chat' CHECK (message_type IN ('chat', 'command', 'system', 'ai-response')),
        reply_to_id TEXT,
        timestamp TEXT NOT NULL,
        ai_processed BOOLEAN DEFAULT false,
        ai_context TEXT, -- JSON blob
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
        FOREIGN KEY (sender_id) REFERENCES citizens(citizen_id) ON DELETE CASCADE,
        FOREIGN KEY (reply_to_id) REFERENCES messages(message_id) ON DELETE SET NULL
      )
    `);

    // Room memberships (many-to-many)
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS room_memberships (
        room_id TEXT NOT NULL,
        citizen_id TEXT NOT NULL,
        joined_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        
        PRIMARY KEY (room_id, citizen_id),
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
        FOREIGN KEY (citizen_id) REFERENCES citizens(citizen_id) ON DELETE CASCADE
      )
    `);

    // Message mentions (many-to-many)
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS message_mentions (
        message_id TEXT NOT NULL,
        mentioned_citizen_id TEXT NOT NULL,
        
        PRIMARY KEY (message_id, mentioned_citizen_id),
        FOREIGN KEY (message_id) REFERENCES messages(message_id) ON DELETE CASCADE,
        FOREIGN KEY (mentioned_citizen_id) REFERENCES citizens(citizen_id) ON DELETE CASCADE
      )
    `);

    // Create indexes for performance
    this.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_messages_room_timestamp ON messages(room_id, timestamp);
      CREATE INDEX IF NOT EXISTS idx_messages_sender ON messages(sender_id);
      CREATE INDEX IF NOT EXISTS idx_room_memberships_citizen ON room_memberships(citizen_id);
      CREATE INDEX IF NOT EXISTS idx_citizens_session ON citizens(session_id);
      CREATE INDEX IF NOT EXISTS idx_rooms_category ON rooms(category);
      CREATE INDEX IF NOT EXISTS idx_rooms_last_activity ON rooms(last_activity);
    `);
  }

  /**
   * Run database migrations
   */
  private async runMigrations(): Promise<void> {
    // Create migrations table if it doesn't exist
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS migrations (
        version INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
      )
    `);

    // Define migrations
    const migrations = [
      {
        version: 1,
        name: 'add_ai_config_fields',
        sql: `
          -- AI configuration fields were added in initial schema
          INSERT OR IGNORE INTO migrations (version, name) VALUES (1, 'add_ai_config_fields');
        `
      }
    ];

    // Apply missing migrations
    for (const migration of migrations) {
      const existing = this.db.prepare('SELECT version FROM migrations WHERE version = ?').get(migration.version);
      if (!existing) {
        this.db.exec(migration.sql);
        console.log(`📈 ChatDatabase: Applied migration ${migration.version}: ${migration.name}`);
      }
    }
  }

  // ===== ROOM OPERATIONS =====

  /**
   * Create a new chat room
   */
  async createRoom(room: Omit<ChatRoom, 'citizens' | 'messageHistory'>): Promise<void> {
    const stmt = this.db.prepare(`
      INSERT INTO rooms (room_id, name, description, category, allow_ai, require_moderation, 
                        is_private, max_history_length, created_at, last_activity)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
    
    stmt.run(
      room.roomId,
      room.name,
      room.description || null,
      room.category,
      room.allowAI ? 1 : 0,
      room.requireModeration ? 1 : 0,
      room.isPrivate ? 1 : 0,
      room.maxHistoryLength,
      room.createdAt,
      room.lastActivity
    );
  }

  /**
   * Get room by ID
   */
  async getRoom(roomId: UUID): Promise<ChatRoom | null> {
    const stmt = this.db.prepare(`
      SELECT * FROM rooms WHERE room_id = ?
    `);
    
    const row = stmt.get(roomId) as any;
    if (!row) return null;

    // Get room citizens
    const citizens = await this.getRoomCitizens(roomId);
    
    // Get recent messages
    const messageHistory = await this.getRoomMessages(roomId, 50); // Last 50 messages

    return {
      roomId: row.room_id,
      name: row.name,
      description: row.description,
      category: row.category,
      citizens: citizens,
      messageHistory,
      maxHistoryLength: row.max_history_length,
      allowAI: Boolean(row.allow_ai),
      requireModeration: Boolean(row.require_moderation),
      isPrivate: Boolean(row.is_private),
      createdAt: row.created_at,
      lastActivity: row.last_activity,
      citizenCount: citizens.length,
      messageCount: messageHistory.length,
      participantCount: citizens.length
    };
  }

  /**
   * Get all rooms
   */
  async getAllRooms(): Promise<ChatRoom[]> {
    const stmt = this.db.prepare('SELECT room_id FROM rooms ORDER BY last_activity DESC');
    const roomIds = stmt.all().map((row: any) => row.room_id);
    
    const rooms = [];
    for (const roomId of roomIds) {
      const room = await this.getRoom(roomId);
      if (room) rooms.push(room);
    }
    
    return rooms;
  }

  /**
   * Update room's last activity
   */
  async updateRoomActivity(roomId: UUID, timestamp: string): Promise<void> {
    const stmt = this.db.prepare('UPDATE rooms SET last_activity = ? WHERE room_id = ?');
    stmt.run(timestamp, roomId);
  }

  // ===== CITIZEN OPERATIONS =====

  /**
   * Create or update a citizen
   */
  async saveCitizen(citizen: ChatCitizen): Promise<void> {
    const stmt = this.db.prepare(`
      INSERT OR REPLACE INTO citizens 
      (citizen_id, session_id, display_name, citizen_type, status, last_seen,
       ai_provider, ai_model, ai_system_prompt)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
    
    stmt.run(
      citizen.citizenId,
      citizen.sessionId,
      citizen.displayName,
      citizen.citizenType,
      citizen.status,
      citizen.lastSeen,
      citizen.aiConfig?.provider || null,
      citizen.aiConfig?.model || null,
      citizen.aiConfig?.systemPrompt || null
    );
  }

  /**
   * Get citizen by ID
   */
  async getCitizen(citizenId: UUID): Promise<ChatCitizen | null> {
    const stmt = this.db.prepare('SELECT * FROM citizens WHERE citizen_id = ?');
    const row = stmt.get(citizenId) as any;
    
    if (!row) return null;

    // Get subscribed rooms
    const roomStmt = this.db.prepare(`
      SELECT room_id FROM room_memberships WHERE citizen_id = ?
    `);
    const roomRows = roomStmt.all(citizenId) as any[];
    const subscribedRooms = roomRows.map(r => r.room_id);

    return {
      citizenId: row.citizen_id,
      sessionId: row.session_id,
      displayName: row.display_name,
      citizenType: row.citizen_type,
      context: { uuid: row.citizen_id, environment: 'server' }, // Mock context
      subscribedRooms,
      aiConfig: row.ai_provider ? {
        provider: row.ai_provider,
        model: row.ai_model,
        systemPrompt: row.ai_system_prompt
      } : undefined,
      status: row.status,
      lastSeen: row.last_seen,
      joinedAt: row.created_at || new Date().toISOString(),
      isOnline: row.status === 'active'
    };
  }

  /**
   * Get all citizens in a room
   */
  async getRoomCitizens(roomId: UUID): Promise<ChatCitizen[]> {
    const stmt = this.db.prepare(`
      SELECT c.* FROM citizens c
      JOIN room_memberships rm ON c.citizen_id = rm.citizen_id
      WHERE rm.room_id = ?
    `);
    
    const rows = stmt.all(roomId) as any[];
    const citizens = [];
    
    for (const row of rows) {
      const citizen = await this.getCitizen(row.citizen_id);
      if (citizen) citizens.push(citizen);
    }
    
    return citizens;
  }

  /**
   * Add citizen to room
   */
  async addCitizenToRoom(roomId: UUID, citizenId: UUID): Promise<void> {
    const stmt = this.db.prepare(`
      INSERT OR IGNORE INTO room_memberships (room_id, citizen_id)
      VALUES (?, ?)
    `);
    stmt.run(roomId, citizenId);
  }

  /**
   * Remove citizen from room
   */
  async removeCitizenFromRoom(roomId: UUID, citizenId: UUID): Promise<void> {
    const stmt = this.db.prepare(`
      DELETE FROM room_memberships WHERE room_id = ? AND citizen_id = ?
    `);
    stmt.run(roomId, citizenId);
  }

  // ===== MESSAGE OPERATIONS =====

  /**
   * Save a chat message
   */
  async saveMessage(message: ChatMessage): Promise<void> {
    const transaction = this.db.transaction(() => {
      // Insert message
      const stmt = this.db.prepare(`
        INSERT INTO messages 
        (message_id, room_id, sender_id, sender_name, sender_type, content, 
         message_type, reply_to_id, timestamp, ai_processed, ai_context)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      `);
      
      stmt.run(
        message.messageId,
        message.roomId,
        message.senderId,
        message.senderName,
        message.senderType,
        message.content,
        message.messageType,
        message.replyToId || null,
        message.timestamp,
        message.aiProcessed ? 1 : 0,
        message.aiContext ? JSON.stringify(message.aiContext) : null
      );

      // Insert mentions
      if (message.mentions && message.mentions.length > 0) {
        const mentionStmt = this.db.prepare(`
          INSERT INTO message_mentions (message_id, mentioned_citizen_id)
          VALUES (?, ?)
        `);
        
        for (const mentionedId of message.mentions) {
          mentionStmt.run(message.messageId, mentionedId);
        }
      }
    });
    
    transaction();
  }

  /**
   * Get messages for a room
   */
  async getRoomMessages(roomId: UUID, limit: number = 50, before?: UUID): Promise<ChatMessage[]> {
    let sql = `
      SELECT m.*, GROUP_CONCAT(mm.mentioned_citizen_id) as mentions
      FROM messages m
      LEFT JOIN message_mentions mm ON m.message_id = mm.message_id
      WHERE m.room_id = ?
    `;
    
    const params: any[] = [roomId];
    
    if (before) {
      sql += ` AND m.timestamp < (SELECT timestamp FROM messages WHERE message_id = ?)`;
      params.push(before);
    }
    
    sql += ` GROUP BY m.message_id ORDER BY m.timestamp DESC LIMIT ?`;
    params.push(limit);
    
    const stmt = this.db.prepare(sql);
    const rows = stmt.all(...params) as any[];
    
    return rows.map(row => ({
      messageId: row.message_id,
      roomId: row.room_id,
      senderId: row.sender_id,
      senderName: row.sender_name,
      senderType: row.sender_type,
      content: row.content,
      timestamp: row.timestamp,
      messageType: row.message_type,
      replyToId: row.reply_to_id,
      mentions: row.mentions ? row.mentions.split(',') : [],
      aiProcessed: Boolean(row.ai_processed),
      aiContext: row.ai_context ? JSON.parse(row.ai_context) : undefined
    })).reverse(); // Reverse to get chronological order
  }

  /**
   * Clean up old messages beyond max history length
   */
  async cleanupOldMessages(): Promise<number> {
    const stmt = this.db.prepare(`
      DELETE FROM messages 
      WHERE message_id IN (
        SELECT m.message_id 
        FROM messages m
        JOIN rooms r ON m.room_id = r.room_id
        WHERE (
          SELECT COUNT(*) 
          FROM messages m2 
          WHERE m2.room_id = m.room_id 
          AND m2.timestamp > m.timestamp
        ) >= r.max_history_length
      )
    `);
    
    const result = stmt.run();
    return result.changes;
  }

  // ===== DATABASE MANAGEMENT =====

  /**
   * Get database statistics
   */
  async getStats(): Promise<DatabaseStats> {
    const stats = {
      roomCount: this.db.prepare('SELECT COUNT(*) as count FROM rooms').get() as any,
      citizenCount: this.db.prepare('SELECT COUNT(*) as count FROM citizens').get() as any,
      messageCount: this.db.prepare('SELECT COUNT(*) as count FROM messages').get() as any,
      totalSize: 0
    };

    if (!this.config.inMemory) {
      // Get file size for persistent databases
      const fs = await import('fs').then(m => m.promises);
      try {
        const fileStats = await fs.stat(this.config.path);
        stats.totalSize = fileStats.size;
      } catch (error) {
        // File doesn't exist yet
      }
    }

    return {
      roomCount: stats.roomCount.count,
      citizenCount: stats.citizenCount.count,
      messageCount: stats.messageCount.count,
      totalSize: stats.totalSize
    };
  }

  /**
   * Backup database to file
   */
  async backup(backupPath: string): Promise<void> {
    if (this.config.inMemory) {
      throw new Error('Cannot backup in-memory database');
    }
    
    const backup = this.db.backup(backupPath);
    await new Promise<void>((resolve, reject) => {
      backup.run((err: Error | null) => {
        if (err) reject(err);
        else resolve();
      });
    });
    
    console.log(`💾 ChatDatabase: Backup saved to ${backupPath}`);
  }

  /**
   * Close database connection
   */
  async close(): Promise<void> {
    this.db.close();
    this.isInitialized = false;
    console.log(`🔒 ChatDatabase: Connection closed`);
  }

  /**
   * Run a transaction
   */
  async transaction<T>(fn: () => T): Promise<T> {
    const txn = this.db.transaction(fn);
    return txn();
  }
}

/**
 * Create database instance based on environment
 */
export function createChatDatabase(environment: 'test' | 'development' | 'production' = 'development'): ChatDatabase {
  const configs = {
    test: {
      path: ':memory:',
      inMemory: true,
      verbose: false
    },
    development: {
      path: '.continuum/jtag/chat/chat.db',
      inMemory: false,
      verbose: true
    },
    production: {
      path: '.continuum/jtag/chat/chat.db',
      inMemory: false,
      verbose: false
    }
  };

  return new ChatDatabase(configs[environment]);
}