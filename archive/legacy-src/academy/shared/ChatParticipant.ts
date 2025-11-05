// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * ChatParticipant - Universal foundation for all chat participants
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: ChatParticipant class methods and capabilities
 * - Integration tests: Participant + CondensedIdentity interaction
 * - Registry tests: ParticipantRegistry operations (register/unregister/find)
 * - Specialized tests: Human/AI/System participant behaviors
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Migrated from interface to class extending CondensedIdentity
 * - Specialized participant types (Human, AI, System) with specific capabilities
 * - ParticipantRegistry for centralized participant management
 * - Factory functions for easy participant creation
 * - Strong typing eliminates 'any' usage throughout chat system
 * 
 * MIGRATED TO CONDENSED IDENTITY SYSTEM
 * 
 * This now uses the CondensedIdentity foundation which provides:
 * - Strong typing (eliminates 'any' usage)
 * - Unified capability system
 * - Consolidated metadata
 * - Universal event system
 * - All features preserved, just better organized
 */

import { UniversalIdentity, BaseMetadata, BaseCapabilities } from '../../core/identity/UniversalIdentity';

// ==================== CHAT PARTICIPANT TYPES ====================

/**
 * Chat-specific metadata extending base metadata
 */
export interface ChatMetadata extends BaseMetadata {
  displayName?: string | undefined;
  avatar?: string | undefined;
  role?: 'user' | 'admin' | 'moderator' | 'guest';
  status?: 'active' | 'away' | 'busy' | 'offline';
  preferences?: {
    communicationStyle?: string;
    topics?: string[];
    expertise?: string[];
    timezone?: string;
    language?: string;
  };
}

/**
 * Chat-specific capabilities extending base capabilities
 */
export interface ChatCapabilities extends BaseCapabilities {
  // Chat capabilities
  sendMessages: boolean;
  receiveMessages: boolean;
  joinRooms: boolean;
  createRooms: boolean;
  moderateRooms: boolean;
  useCommands: boolean;
  mention: boolean;
  react: boolean;
  
  // Academy capabilities (optional for chat)
  learn?: boolean;
  evolve?: boolean;
  teach?: boolean;
  spawn?: boolean;
  mutate?: boolean;
  crossover?: boolean;
  adapt?: boolean;
  useRAG?: boolean;
}

// ==================== CHAT PARTICIPANT (CONDENSED) ====================

/**
 * ChatParticipant is now a UniversalIdentity with chat-focused defaults
 */
class ChatParticipant extends UniversalIdentity<ChatMetadata, ChatCapabilities> {
  constructor(config: {
    id?: string;
    name: string;
    type: ParticipantType;
    displayName?: string;
    avatar?: string;
    metadata?: Partial<ChatMetadata>;
  }) {
    // Create chat-focused capabilities
    const chatCapabilities: ChatCapabilities = {
      // Core
      communicate: true,
      serialize: true,
      
      // Chat capabilities (preserved from original interface)
      sendMessages: true,
      receiveMessages: true,
      joinRooms: true,
      createRooms: config.type === 'human',
      moderateRooms: false,
      useCommands: true,
      mention: true,
      react: true,
      
      // Academy capabilities (disabled by default for chat focus)
      learn: false,
      evolve: false,
      teach: false,
      spawn: false,
      mutate: false,
      crossover: false,
      adapt: false,
      useRAG: false,
      
      // Override with provided capabilities
      ...config.metadata?.capabilities
    };
    
    // Create chat-focused metadata
    const chatMetadata: ChatMetadata = {
      // Universal properties
      description: config.metadata?.description || `${config.name} chat participant`,
      version: config.metadata?.version || '1.0.0',
      source: config.metadata?.source || 'chat',
      lastActivity: config.metadata?.lastActivity || Date.now(),
      isActive: config.metadata?.isActive !== false,
      
      // Chat-specific metadata (preserved from original)
      ...(config.displayName && { displayName: config.displayName }),
      ...(config.avatar && { avatar: config.avatar }),
      
      // Merge any additional metadata
      ...config.metadata
    };
    
    super({
      ...(config.id && { id: config.id }),
      name: config.name,
      type: config.type,
      metadata: chatMetadata,
      capabilities: chatCapabilities
    });
    
    // Set chat context by default
    this.updateState({ context: 'chat' });
  }
  
  // ==================== BACKWARD COMPATIBILITY ====================
  
  /**
   * Preserve original ChatParticipant interface
   */
  get canCommunicate(): boolean {
    return this.hasCapability('communicate');
  }
  
  get displayName(): string | undefined {
    return this.metadata.displayName;
  }
  
  set displayName(value: string | undefined) {
    this.metadata = { ...this.metadata, displayName: value };
  }
  
  get avatar(): string | undefined {
    return this.metadata.avatar;
  }
  
  set avatar(value: string | undefined) {
    this.metadata = { ...this.metadata, avatar: value };
  }
  
  // ==================== REQUIRED ABSTRACT METHODS ====================
  
  /**
   * Handle incoming messages (chat-specific implementation)
   */
  async handleMessage(message: any): Promise<void> {
    this.logMessage(`💬 ${this.name} received message: ${message.content || message.type}`);
    
    // Record the message interaction
    this.recordEvent('message_received', {
      success: true,
      messageId: message.id,
      content: message.content
    });
  }
  
  /**
   * Initialize chat-specific resources
   */
  async initializeSpecific(): Promise<void> {
    this.logMessage(`🚀 Initializing chat participant: ${this.name}`);
    
    // Initialize chat-specific capabilities
    if (this.hasCapability('joinRooms')) {
      this.logMessage(`🏠 Chat participant can join rooms`);
    }
    
    if (this.hasCapability('sendMessages')) {
      this.logMessage(`📤 Chat participant can send messages`);
    }
  }
  
  /**
   * Cleanup chat-specific resources
   */
  async destroySpecific(): Promise<void> {
    this.logMessage(`🔚 Destroying chat participant: ${this.name}`);
    
    // Clean up any chat-specific resources
    // (rooms, message subscriptions, etc.)
  }
}

/**
 * Types of chat participants (preserved from original)
 */
type ParticipantType = 
  | 'human'           // Real human user
  | 'ai_assistant'    // AI like Claude
  | 'persona'         // Custom persona
  | 'bot'             // Automated bot
  | 'agent'           // AI agent
  | 'system'          // System messages
  | 'custom';         // User-defined

// ==================== SPECIALIZED PARTICIPANT TYPES ====================

/**
 * Human participant - now extends ChatParticipant (CondensedIdentity)
 */
class HumanParticipant extends ChatParticipant {
  constructor(config: {
    id?: string;
    name: string;
    displayName?: string;
    avatar?: string;
    preferences?: {
      communicationStyle?: string;
      topics?: string[];
      expertise?: string[];
      timezone?: string;
      language?: string;
    };
    role?: 'user' | 'admin' | 'moderator' | 'guest';
    status?: 'active' | 'away' | 'busy' | 'offline';
  }) {
    super({
      ...(config.id && { id: config.id }),
      name: config.name,
      type: 'human',
      metadata: {
        // Human-specific metadata
        ...(config.displayName && { displayName: config.displayName }),
        ...(config.avatar && { avatar: config.avatar }),
        ...(config.preferences && { preferences: config.preferences }),
        role: config.role || 'user',
        status: config.status || 'active',
        
        // Human gets enhanced chat capabilities
        capabilities: {
          communicate: true,
          serialize: true,
          sendMessages: true,
          receiveMessages: true,
          joinRooms: true,
          createRooms: true,
          moderateRooms: config.role === 'moderator' || config.role === 'admin',
          useCommands: true,
          mention: true,
          react: true,
          
          // Humans can learn from conversations
          learn: true,
          adapt: true,
          
          // Other capabilities disabled by default
          evolve: false,
          teach: false,
          spawn: false,
          mutate: false,
          crossover: false,
          useRAG: false
        }
      }
    });
  }
}

/**
 * AI Assistant participant (like Claude) - now extends ChatParticipant
 */
class AIAssistantParticipant extends ChatParticipant {
  constructor(config: {
    id?: string;
    name: string;
    displayName?: string;
    avatar?: string;
    model?: string;
    capabilities?: string[];
    limitations?: string[];
    systemPrompt?: string;
    version?: string;
    provider?: string;
    contextWindow?: number;
  }) {
    super({
      ...(config.id && { id: config.id }),
      name: config.name,
      type: 'ai_assistant',
      metadata: {
        // AI-specific metadata
        ...(config.displayName && { displayName: config.displayName }),
        ...(config.avatar && { avatar: config.avatar }),
        ...(config.model && { model: config.model }),
        ...(config.limitations && { limitations: config.limitations }),
        ...(config.systemPrompt && { systemPrompt: config.systemPrompt }),
        ...(config.version && { version: config.version }),
        ...(config.provider && { provider: config.provider }),
        ...(config.contextWindow && { contextWindow: config.contextWindow }),
        
        // AI gets enhanced capabilities
        capabilities: {
          communicate: true,
          serialize: true,
          sendMessages: true,
          receiveMessages: true,
          joinRooms: true,
          createRooms: false,
          moderateRooms: false,
          useCommands: true,
          mention: true,
          react: true,
          
          // AI can learn and use RAG
          learn: true,
          adapt: true,
          useRAG: true,
          
          // Evolution depends on specific AI type
          evolve: false,
          teach: config.capabilities?.includes('teaching') || false,
          spawn: false,
          mutate: false,
          crossover: false
        }
      }
    });
  }
}

/**
 * System participant for system messages - now extends ChatParticipant
 */
class SystemParticipant extends ChatParticipant {
  constructor(config: {
    id?: string;
    name: string;
    component?: string;
    level?: 'info' | 'warning' | 'error' | 'success';
    automated?: boolean;
  }) {
    super({
      ...(config.id && { id: config.id }),
      name: config.name,
      type: 'system',
      metadata: {
        // System-specific metadata
        ...(config.component && { component: config.component }),
        level: config.level || 'info',
        automated: config.automated !== false,
        
        // System has limited capabilities
        capabilities: {
          communicate: true,
          serialize: true,
          sendMessages: true,
          receiveMessages: false,
          joinRooms: false,
          createRooms: false,
          moderateRooms: false,
          useCommands: false,
          mention: false,
          react: false,
          
          // System doesn't learn or evolve
          learn: false,
          evolve: false,
          teach: false,
          spawn: false,
          mutate: false,
          crossover: false,
          adapt: false,
          useRAG: false
        }
      }
    });
  }
}

// ==================== PARTICIPANT CREATION ====================

/**
 * Create a basic chat participant (now using CondensedIdentity)
 */
export function createChatParticipant(config: {
  name: string;
  type: ParticipantType;
  displayName?: string;
  avatar?: string;
  metadata?: Partial<ChatMetadata>;
}): ChatParticipant {
  return new ChatParticipant(config);
}

/**
 * Create a human participant (now using CondensedIdentity)
 */
export function createHumanParticipant(
  name: string, 
  config?: {
    displayName?: string;
    avatar?: string;
    preferences?: {
      communicationStyle?: string;
      topics?: string[];
      expertise?: string[];
      timezone?: string;
      language?: string;
    };
    role?: 'user' | 'admin' | 'moderator' | 'guest';
    status?: 'active' | 'away' | 'busy' | 'offline';
  }
): HumanParticipant {
  return new HumanParticipant({
    name,
    ...config
  });
}

/**
 * Create an AI assistant participant (now using CondensedIdentity)
 */
export function createAIAssistantParticipant(
  name: string,
  config?: {
    displayName?: string;
    avatar?: string;
    model?: string;
    capabilities?: string[];
    limitations?: string[];
    systemPrompt?: string;
    version?: string;
    provider?: string;
    contextWindow?: number;
  }
): AIAssistantParticipant {
  return new AIAssistantParticipant({
    name,
    ...config
  });
}

/**
 * Create a Claude participant (specialized AI assistant)
 */
export function createClaudeParticipant(
  name: string = 'Claude',
  config?: {
    displayName?: string;
    avatar?: string;
    systemPrompt?: string;
    version?: string;
  }
): AIAssistantParticipant {
  return new AIAssistantParticipant({
    name,
    displayName: config?.displayName || 'Claude',
    avatar: config?.avatar || '🤖',
    model: 'claude-3',
    provider: 'anthropic',
    version: config?.version || '3.0',
    systemPrompt: config?.systemPrompt || 'You are Claude, an AI assistant created by Anthropic.',
    capabilities: ['reasoning', 'analysis', 'writing', 'coding', 'teaching'],
    limitations: ['no_internet', 'no_real_time', 'training_cutoff'],
    contextWindow: 200000
  });
}

/**
 * Create a system participant (now using CondensedIdentity)
 */
export function createSystemParticipant(
  name: string,
  config?: {
    component?: string;
    level?: 'info' | 'warning' | 'error' | 'success';
    automated?: boolean;
  }
): SystemParticipant {
  return new SystemParticipant({
    name,
    ...config
  });
}

// ==================== PARTICIPANT REGISTRY ====================

/**
 * Registry for managing chat participants (now using CondensedIdentity)
 */
class ParticipantRegistry {
  private participants: Map<string, ChatParticipant> = new Map();

  /**
   * Register a participant
   */
  register(participant: ChatParticipant): void {
    this.participants.set(participant.id, participant);
    console.log(`📋 Registered participant: ${participant.name} (${participant.type})`);
  }

  /**
   * Unregister a participant
   */
  unregister(id: string): void {
    const participant = this.participants.get(id);
    if (participant) {
      this.participants.delete(id);
      console.log(`📋 Unregistered participant: ${participant.name}`);
    }
  }

  /**
   * Get participant by ID
   */
  get(id: string): ChatParticipant | undefined {
    return this.participants.get(id);
  }

  /**
   * Get all participants
   */
  getAll(): ChatParticipant[] {
    return Array.from(this.participants.values());
  }

  /**
   * Find participants by type
   */
  findByType(type: ParticipantType): ChatParticipant[] {
    return this.getAll().filter(p => p.type === type);
  }

  /**
   * Find participants by capability
   */
  findByCapability(capability: string): ChatParticipant[] {
    return this.getAll().filter(p => p.hasCapability(capability));
  }

  /**
   * Get participants by type (alias for findByType)
   */
  getByType(type: ParticipantType): ChatParticipant[] {
    return this.findByType(type);
  }

  /**
   * Get count of participants
   */
  get count(): number {
    return this.participants.size;
  }
}

// ==================== EXPORTS ====================

export {
  ChatParticipant,
  HumanParticipant,
  AIAssistantParticipant,
  SystemParticipant,
  ParticipantRegistry
};

export type { ParticipantType };
