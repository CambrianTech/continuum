/**
 * Chat Migration Plan - Elegant Architecture Transition
 * 
 * This plan ensures the existing chat widget continues to work while
 * gradually migrating to the new PersonaBase architecture.
 * 
 * Key principles:
 * - Don't break existing chat widget functionality
 * - Maintain backward compatibility
 * - Gradually enhance with PersonaBase features
 * - Use adapter pattern for smooth transition
 * - Enable future PersonaBase integration
 */

import { PersonaBase } from '../../academy/shared/PersonaBase';
import { ChatParticipant } from '../../academy/shared/ChatParticipant';
import { ChatMessage, ChatRoom, ChatSystem } from './ChatArchitecture';
import { generateUUID } from '../../academy/shared/AcademyTypes';

// ==================== COMPATIBILITY LAYER ====================

/**
 * Legacy chat participant - what the current chat widget expects
 */
export interface LegacyChatParticipant {
  id: string;
  name: string;
  type?: string;
  avatar?: string;
  isOnline?: boolean;
  lastSeen?: number;
  
  // Legacy properties the current widget might use
  displayName?: string;
  status?: string;
  metadata?: any;
}

/**
 * Legacy chat message - what the current chat widget expects
 */
export interface LegacyChatMessage {
  id: string;
  senderId: string;
  senderName: string;
  content: string;
  timestamp: number;
  type?: 'text' | 'system' | 'error' | 'notification';
  
  // Legacy properties
  roomId?: string;
  attachments?: any[];
  reactions?: any[];
}

/**
 * Legacy chat room - what the current chat widget expects
 */
export interface LegacyChatRoom {
  id: string;
  name: string;
  description?: string;
  participants: LegacyChatParticipant[];
  messages: LegacyChatMessage[];
  created: number;
  
  // Legacy properties
  settings?: any;
  isActive?: boolean;
}

// ==================== ADAPTER PATTERN ====================

/**
 * Adapter that converts PersonaBase to LegacyChatParticipant
 */
export class PersonaToLegacyAdapter {
  static toLegacyParticipant(persona: PersonaBase): LegacyChatParticipant {
    return {
      id: persona.id,
      name: persona.name,
      type: persona.type,
      avatar: persona.avatar,
      isOnline: persona.canCommunicate,
      lastSeen: Date.now(),
      displayName: persona.displayName || persona.name,
      status: persona.canCommunicate ? 'online' : 'offline',
      metadata: {
        ...persona.metadata,
        isPersona: true,
        hasPrompt: !!persona.prompt,
        hasRAG: !!persona.rag
      }
    };
  }

  static fromLegacyParticipant(legacy: LegacyChatParticipant): PersonaBase {
    return {
      id: legacy.id,
      name: legacy.name,
      type: (legacy.type as any) || 'custom',
      created: Date.now(),
      canCommunicate: legacy.isOnline !== false,
      displayName: legacy.displayName || legacy.name,
      avatar: legacy.avatar,
      metadata: {
        ...legacy.metadata,
        migratedFromLegacy: true,
        originalType: legacy.type
      },
      prompt: legacy.metadata?.prompt || `You are ${legacy.name}, a chat participant.`,
      description: `Migrated from legacy chat participant: ${legacy.name}`
    };
  }
}

/**
 * Adapter that converts between ChatMessage and LegacyChatMessage
 */
export class MessageAdapter {
  static toLegacyMessage(message: ChatMessage): LegacyChatMessage {
    return {
      id: message.id,
      senderId: message.sender.id,
      senderName: message.sender.name,
      content: message.content,
      timestamp: message.timestamp,
      type: message.type as any,
      roomId: message.roomId,
      attachments: message.attachments || [],
      reactions: message.reactions || []
    };
  }

  static fromLegacyMessage(legacy: LegacyChatMessage, sender: PersonaBase): ChatMessage {
    return {
      id: legacy.id,
      sender,
      content: legacy.content,
      timestamp: legacy.timestamp,
      type: (legacy.type as any) || 'text',
      roomId: legacy.roomId,
      attachments: legacy.attachments || [],
      reactions: legacy.reactions || [],
      mentions: []
    };
  }
}

/**
 * Adapter that converts between ChatRoom and LegacyChatRoom
 */
export class RoomAdapter {
  static toLegacyRoom(room: ChatRoom): LegacyChatRoom {
    return {
      id: room.id,
      name: room.name,
      description: room.description,
      participants: room.participants.map(p => PersonaToLegacyAdapter.toLegacyParticipant(p)),
      messages: [], // Messages handled separately
      created: room.created,
      settings: room.settings,
      isActive: room.isActive
    };
  }

  static fromLegacyRoom(legacy: LegacyChatRoom): ChatRoom {
    return {
      id: legacy.id,
      name: legacy.name,
      description: legacy.description,
      created: legacy.created,
      participants: legacy.participants.map(p => PersonaToLegacyAdapter.fromLegacyParticipant(p)),
      moderators: [legacy.participants[0]?.id || ''], // First participant as moderator
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
        historyRetentionDays: 30,
        ...legacy.settings
      },
      isActive: legacy.isActive !== false,
      lastActivity: Date.now(),
      messageCount: legacy.messages?.length || 0
    };
  }
}

// ==================== MIGRATION STRATEGY ====================

/**
 * Migration strategy for transitioning to PersonaBase architecture
 */
export class ChatMigrationStrategy {
  private legacyMode: boolean = true;
  private hybridMode: boolean = false;
  private personaMode: boolean = false;

  /**
   * Phase 1: Legacy Mode (Current State)
   * - Existing chat widget works as-is
   * - No PersonaBase integration yet
   * - Backward compatibility maintained
   */
  enableLegacyMode(): void {
    this.legacyMode = true;
    this.hybridMode = false;
    this.personaMode = false;
    console.log('📞 Chat Migration: Legacy Mode enabled');
  }

  /**
   * Phase 2: Hybrid Mode (Transition)
   * - Existing chat widget works with adapters
   * - PersonaBase objects converted to legacy format
   * - Gradual feature enhancement
   */
  enableHybridMode(): void {
    this.legacyMode = false;
    this.hybridMode = true;
    this.personaMode = false;
    console.log('🔄 Chat Migration: Hybrid Mode enabled');
  }

  /**
   * Phase 3: Persona Mode (Future State)
   * - Full PersonaBase integration
   * - Enhanced chat features
   * - Command system active
   */
  enablePersonaMode(): void {
    this.legacyMode = false;
    this.hybridMode = false;
    this.personaMode = true;
    console.log('🎭 Chat Migration: Persona Mode enabled');
  }

  /**
   * Get current migration phase
   */
  getCurrentPhase(): 'legacy' | 'hybrid' | 'persona' {
    if (this.legacyMode) return 'legacy';
    if (this.hybridMode) return 'hybrid';
    return 'persona';
  }

  /**
   * Check if feature is available in current phase
   */
  isFeatureAvailable(feature: ChatFeature): boolean {
    const phase = this.getCurrentPhase();
    
    switch (feature) {
      case 'basic_messaging':
        return true; // Available in all phases
      
      case 'persona_integration':
        return phase === 'hybrid' || phase === 'persona';
      
      case 'command_system':
        return phase === 'persona';
      
      case 'advanced_features':
        return phase === 'persona';
      
      default:
        return false;
    }
  }
}

export type ChatFeature = 
  | 'basic_messaging'
  | 'persona_integration'
  | 'command_system'
  | 'advanced_features';

// ==================== COMPATIBILITY WRAPPER ====================

/**
 * Compatibility wrapper for existing chat widget
 */
export class ChatCompatibilityWrapper {
  private migrationStrategy: ChatMigrationStrategy;
  private chatSystem?: ChatSystem;

  constructor(migrationStrategy: ChatMigrationStrategy) {
    this.migrationStrategy = migrationStrategy;
  }

  /**
   * Set the new chat system (when available)
   */
  setChatSystem(chatSystem: ChatSystem): void {
    this.chatSystem = chatSystem;
  }

  /**
   * Get participants in legacy format
   */
  async getLegacyParticipants(roomId: string): Promise<LegacyChatParticipant[]> {
    const phase = this.migrationStrategy.getCurrentPhase();
    
    if (phase === 'legacy') {
      // Return mock legacy participants for now
      return this.getMockLegacyParticipants();
    }
    
    if (phase === 'hybrid' || phase === 'persona') {
      if (!this.chatSystem) {
        return this.getMockLegacyParticipants();
      }
      
      // Get PersonaBase participants and convert to legacy
      const personas = await this.chatSystem.getParticipants(roomId);
      return personas.map(p => PersonaToLegacyAdapter.toLegacyParticipant(p));
    }
    
    return [];
  }

  /**
   * Get messages in legacy format
   */
  async getLegacyMessages(roomId: string): Promise<LegacyChatMessage[]> {
    const phase = this.migrationStrategy.getCurrentPhase();
    
    if (phase === 'legacy') {
      // Return mock legacy messages for now
      return this.getMockLegacyMessages();
    }
    
    if (phase === 'hybrid' || phase === 'persona') {
      if (!this.chatSystem) {
        return this.getMockLegacyMessages();
      }
      
      // Get ChatMessage objects and convert to legacy
      const messages = await this.chatSystem.getMessages(roomId);
      return messages.map(m => MessageAdapter.toLegacyMessage(m));
    }
    
    return [];
  }

  /**
   * Send message in legacy format
   */
  async sendLegacyMessage(
    senderId: string,
    content: string,
    roomId?: string
  ): Promise<LegacyChatMessage> {
    const phase = this.migrationStrategy.getCurrentPhase();
    
    if (phase === 'legacy') {
      // Create mock legacy message
      return this.createMockLegacyMessage(senderId, content, roomId);
    }
    
    if (phase === 'hybrid' || phase === 'persona') {
      if (!this.chatSystem) {
        return this.createMockLegacyMessage(senderId, content, roomId);
      }
      
      // Get sender as PersonaBase
      const sender = await this.chatSystem.getParticipant(senderId);
      if (!sender) {
        throw new Error(`Sender ${senderId} not found`);
      }
      
      // Send via new system
      const message = await this.chatSystem.sendMessage({
        sender,
        content,
        type: 'text',
        roomId
      });
      
      // Convert to legacy format
      return MessageAdapter.toLegacyMessage(message);
    }
    
    throw new Error('Invalid migration phase');
  }

  // ==================== MOCK DATA (FOR TESTING) ====================

  private getMockLegacyParticipants(): LegacyChatParticipant[] {
    return [
      {
        id: 'user-1',
        name: 'Joel',
        type: 'human',
        isOnline: true,
        displayName: 'Joel',
        status: 'online'
      },
      {
        id: 'claude-1',
        name: 'Claude',
        type: 'ai_assistant',
        isOnline: true,
        displayName: 'Claude',
        status: 'online'
      }
    ];
  }

  private getMockLegacyMessages(): LegacyChatMessage[] {
    return [
      {
        id: 'msg-1',
        senderId: 'user-1',
        senderName: 'Joel',
        content: 'Hello!',
        timestamp: Date.now() - 60000,
        type: 'text'
      },
      {
        id: 'msg-2',
        senderId: 'claude-1',
        senderName: 'Claude',
        content: 'Hello! How can I help you?',
        timestamp: Date.now() - 30000,
        type: 'text'
      }
    ];
  }

  private createMockLegacyMessage(
    senderId: string,
    content: string,
    roomId?: string
  ): LegacyChatMessage {
    return {
      id: generateUUID(),
      senderId,
      senderName: `User ${senderId}`,
      content,
      timestamp: Date.now(),
      type: 'text',
      roomId
    };
  }
}

// ==================== IMPLEMENTATION ROADMAP ====================

/**
 * Implementation roadmap for chat migration
 */
export const CHAT_MIGRATION_ROADMAP = {
  
  /**
   * Phase 1: Foundation (Current)
   * - Existing chat widget works as-is
   * - Create PersonaBase architecture
   * - Build adapter layer
   * - No breaking changes
   */
  phase1: {
    name: 'Foundation',
    description: 'Establish PersonaBase architecture without breaking existing chat',
    tasks: [
      'Create ChatParticipant and PersonaBase types',
      'Build adapter layer for legacy compatibility',
      'Create migration strategy framework',
      'Test existing chat widget still works'
    ],
    deliverables: [
      'PersonaBase architecture',
      'Adapter layer',
      'Migration strategy',
      'Compatibility wrapper'
    ]
  },

  /**
   * Phase 2: Hybrid Integration (Next)
   * - Gradual PersonaBase integration
   * - Enhanced features available
   * - Backward compatibility maintained
   * - Optional PersonaBase features
   */
  phase2: {
    name: 'Hybrid Integration',
    description: 'Gradually integrate PersonaBase features while maintaining compatibility',
    tasks: [
      'Implement ChatSystem with PersonaBase support',
      'Add PersonaBase participant creation',
      'Enable persona-to-legacy conversion',
      'Add basic command system'
    ],
    deliverables: [
      'Working ChatSystem implementation',
      'PersonaBase participant support',
      'Basic command system',
      'Enhanced chat features'
    ]
  },

  /**
   * Phase 3: Full Migration (Future)
   * - Complete PersonaBase integration
   * - Full command system
   * - Academy integration
   * - Advanced features
   */
  phase3: {
    name: 'Full Migration',
    description: 'Complete migration to PersonaBase architecture with all features',
    tasks: [
      'Complete command system implementation',
      'Add Academy persona integration',
      'Implement advanced chat features',
      'Optimize performance and UX'
    ],
    deliverables: [
      'Full PersonaBase chat system',
      'Complete command system',
      'Academy integration',
      'Advanced features'
    ]
  }
};

// ==================== EXPORTS ====================

export {
  LegacyChatParticipant,
  LegacyChatMessage,
  LegacyChatRoom,
  PersonaToLegacyAdapter,
  MessageAdapter,
  RoomAdapter,
  ChatMigrationStrategy,
  ChatCompatibilityWrapper,
  ChatFeature
};