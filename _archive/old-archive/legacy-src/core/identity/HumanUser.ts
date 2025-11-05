// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Human User - Specialized identity for human participants
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Human-specific message handling and capabilities
 * - Integration tests: Human + chat room interaction
 * - Authentication tests: Human identity verification
 * 
 * ARCHITECTURAL INSIGHTS:
 * - 90% shared logic from UniversalIdentity foundation
 * - 10% human-specific implementation (this class)
 * - Separation of burden: Foundation handles infrastructure, this handles human specifics
 * - Modular design: Human behavior isolated from other identity types
 * 
 * HUMAN-SPECIFIC FEATURES:
 * - Authentication integration
 * - Manual message sending (not automated)
 * - Human-friendly error messages
 * - Session management
 * - Preference management
 */

import { UniversalIdentity, BaseMetadata, BaseCapabilities } from './UniversalIdentity';

/**
 * Human-specific metadata - extends base with human-relevant properties
 */
export interface HumanMetadata extends BaseMetadata {
  email?: string;
  theme?: 'light' | 'dark' | 'auto';
  notifications?: boolean;
  sounds?: boolean;
  isOnline?: boolean;
  lastSeen?: number;
  currentRoom?: string;
  roomsJoined?: string[];
  messageCount?: number;
}

/**
 * Human-specific capabilities - extends base with human-relevant capabilities
 */
export interface HumanCapabilities extends BaseCapabilities {
  // Chat capabilities (humans can do most chat actions)
  sendMessages: boolean;
  receiveMessages: boolean;
  joinRooms: boolean;
  createRooms: boolean;
  moderateRooms: boolean;
  useCommands: boolean;
  mention: boolean;
  react: boolean;
}

/**
 * Human User - Specialized for human participants
 * Generic types follow inheritance naturally - no type overrides needed
 */
export class HumanUser extends UniversalIdentity<HumanMetadata, HumanCapabilities> {
  private sessionId?: string | undefined;
  private preferences: Map<string, any> = new Map();
  
  constructor(config: {
    id?: string;
    name: string;
    email?: string;
    metadata?: Partial<HumanMetadata>;
  }) {
    // Human-specific capabilities - no complex spread operations
    const humanCapabilities: HumanCapabilities = {
      communicate: true,
      serialize: true,
      sendMessages: true,
      receiveMessages: true,
      joinRooms: true,
      createRooms: true,
      moderateRooms: true,
      useCommands: true,
      mention: true,
      react: true
    };
    
    // Human-specific metadata - simple assignment
    const humanMetadata: HumanMetadata = {
      description: config.metadata?.description || `Human user: ${config.name}`,
      ...(config.email && { email: config.email }),
      isOnline: true,
      theme: 'auto',
      notifications: true,
      sounds: true,
      messageCount: 0,
      roomsJoined: [],
      ...config.metadata
    };
    
    super({
      ...(config.id && { id: config.id }),
      name: config.name,
      type: 'human',
      capabilities: humanCapabilities,
      metadata: humanMetadata
    });
    this.logMessage('👤 Human user created');
  }
  
  // ==================== HUMAN-SPECIFIC IMPLEMENTATIONS ====================
  
  /**
   * Handle message - Human-specific message processing
   */
  async handleMessage(message: any): Promise<any> {
    if (!this.hasCapability('receiveMessages')) {
      this.logMessage('⚠️ Cannot receive messages, ignoring');
      return;
    }
    
    this.logMessage(`📨 Human received message: ${message.content || message.type}`);
    
    // Record human message interaction
    this.recordEvent('message_received', {
      success: true,
      messageType: message.type,
      fromId: message.senderId,
      roomId: message.roomId
    });
    
    // Update activity metadata
    this.metadata.lastActivity = Date.now();
    this.metadata.messageCount = (this.metadata.messageCount || 0) + 1;
    
    // Humans don't auto-respond - they manually send messages
    return {
      type: 'message_acknowledged',
      content: 'Message received by human',
      humanId: this.id,
      requiresManualResponse: true
    };
  }
  
  /**
   * Initialize human-specific functionality
   */
  async initializeSpecific(): Promise<void> {
    this.logMessage('🚀 Initializing human user');
    
    // Initialize session
    this.sessionId = `human_session_${Date.now()}`;
    
    // Load user preferences
    await this.loadPreferences();
    
    // Set online status
    this.metadata.isOnline = true;
    this.metadata.lastSeen = Date.now();
    
    this.recordEvent('human_initialized', { success: true, sessionId: this.sessionId });
    this.logMessage('✅ Human user initialized');
  }
  
  /**
   * Cleanup human-specific resources
   */
  async destroySpecific(): Promise<void> {
    this.logMessage('🛑 Destroying human user');
    
    // Save preferences
    await this.savePreferences();
    
    // Set offline status
    this.metadata.isOnline = false;
    this.metadata.lastSeen = Date.now();
    
    // Clear session
    this.sessionId = undefined;
    
    this.recordEvent('human_destroyed', { success: true });
    this.logMessage('✅ Human user destroyed');
  }
  
  // ==================== HUMAN-SPECIFIC METHODS ====================
  
  /**
   * Send message manually (human-initiated)
   */
  async sendMessage(content: string, roomId?: string): Promise<void> {
    if (!this.hasCapability('sendMessages')) {
      throw new Error('Cannot send messages');
    }
    
    this.logMessage(`📤 Human sending message: ${content}`);
    
    // Record human message sending
    this.recordEvent('message_sent', {
      success: true,
      content,
      roomId,
      manual: true
    });
    
    // Update activity
    this.metadata.lastActivity = Date.now();
    this.metadata.messageCount = (this.metadata.messageCount || 0) + 1;
  }
  
  /**
   * Join chat room
   */
  async joinRoom(roomId: string): Promise<void> {
    if (!this.hasCapability('joinRooms')) {
      throw new Error('Cannot join rooms');
    }
    
    this.logMessage(`🚪 Human joining room: ${roomId}`);
    
    // Update metadata
    this.metadata.currentRoom = roomId;
    this.metadata.roomsJoined = this.metadata.roomsJoined || [];
    if (!this.metadata.roomsJoined.includes(roomId)) {
      this.metadata.roomsJoined.push(roomId);
    }
    
    this.recordEvent('room_joined', {
      success: true,
      roomId,
      manual: true
    });
  }
  
  /**
   * Leave chat room
   */
  async leaveRoom(roomId: string): Promise<void> {
    this.logMessage(`🚪 Human leaving room: ${roomId}`);
    
    // Update metadata
    if (this.metadata.currentRoom === roomId) {
      delete this.metadata.currentRoom;
    }
    
    this.recordEvent('room_left', {
      success: true,
      roomId,
      manual: true
    });
  }
  
  /**
   * Update user preferences
   */
  setPreference(key: string, value: any): void {
    this.preferences.set(key, value);
    this.recordEvent('preference_updated', {
      success: true,
      key,
      value
    });
  }
  
  /**
   * Get user preference
   */
  getPreference(key: string): any {
    return this.preferences.get(key);
  }
  
  /**
   * Get session ID
   */
  getSessionId(): string | undefined {
    return this.sessionId;
  }
  
  /**
   * Update online status
   */
  setOnlineStatus(isOnline: boolean): void {
    this.metadata.isOnline = isOnline;
    this.metadata.lastSeen = Date.now();
    
    this.recordEvent('status_updated', {
      success: true,
      isOnline,
      timestamp: this.metadata.lastSeen
    });
  }
  
  // ==================== PRIVATE METHODS ====================
  
  /**
   * Load user preferences
   */
  private async loadPreferences(): Promise<void> {
    // In a real implementation, this would load from database/storage
    this.preferences.set('theme', this.metadata.theme || 'auto');
    this.preferences.set('notifications', this.metadata.notifications !== false);
    this.preferences.set('sounds', this.metadata.sounds !== false);
  }
  
  /**
   * Save user preferences
   */
  private async savePreferences(): Promise<void> {
    // In a real implementation, this would save to database/storage
    this.metadata.theme = this.preferences.get('theme') || 'auto';
    this.metadata.notifications = this.preferences.get('notifications') !== false;
    this.metadata.sounds = this.preferences.get('sounds') !== false;
  }
}

/**
 * Factory function for creating human users
 */
export function createHumanUser(config: {
  id?: string;
  name: string;
  email?: string;
  metadata?: Partial<HumanMetadata>;
}): HumanUser {
  return new HumanUser(config);
}