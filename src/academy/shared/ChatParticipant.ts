/**
 * ChatParticipant - Universal foundation for all chat participants
 * 
 * This is the TRUE ROOT of the persona system - supporting ANY entity that can chat:
 * - Humans (like users)
 * - Chatbots (like Claude)
 * - Custom personas
 * - Any communicating entity
 * 
 * Key principle: Universal interface for all chat participants with smart inheritance
 */

import { generateUUID } from './AcademyTypes';

// ==================== UNIVERSAL CHAT PARTICIPANT ====================

/**
 * The most fundamental interface - any entity that can participate in chat
 */
export interface ChatParticipant {
  id: string;
  name: string;
  type: ParticipantType;
  created: number;
  
  // Core communication capability
  canCommunicate: boolean;
  
  // Optional display preferences
  displayName?: string;
  avatar?: string;
  
  // Flexible metadata for any extensions
  metadata?: Record<string, any>;
}

/**
 * Types of chat participants
 */
export type ParticipantType = 
  | 'human'           // Real human user
  | 'ai_assistant'    // AI like Claude
  | 'persona'         // Custom persona
  | 'bot'             // Automated bot
  | 'agent'           // AI agent
  | 'system'          // System messages
  | 'custom';         // User-defined

// ==================== SPECIALIZED PARTICIPANT TYPES ====================

/**
 * Human participant
 */
export interface HumanParticipant extends ChatParticipant {
  type: 'human';
  
  // Human-specific properties
  preferences?: {
    communicationStyle?: string;
    topics?: string[];
    expertise?: string[];
    timezone?: string;
    language?: string;
  };
  
  // Optional human context
  role?: 'user' | 'admin' | 'moderator' | 'guest';
  status?: 'active' | 'away' | 'busy' | 'offline';
}

/**
 * AI Assistant participant (like Claude)
 */
export interface AIAssistantParticipant extends ChatParticipant {
  type: 'ai_assistant';
  
  // AI-specific properties
  model?: string;
  capabilities?: string[];
  limitations?: string[];
  systemPrompt?: string;
  
  // AI context
  version?: string;
  provider?: string;
  contextWindow?: number;
}

/**
 * System participant for system messages
 */
export interface SystemParticipant extends ChatParticipant {
  type: 'system';
  
  // System-specific properties
  component?: string;
  level?: 'info' | 'warning' | 'error' | 'success';
  automated?: boolean;
}

// ==================== PARTICIPANT CREATION ====================

/**
 * Configuration for creating chat participants
 */
export interface CreateParticipantConfig {
  name: string;
  type: ParticipantType;
  displayName?: string;
  avatar?: string;
  metadata?: Record<string, any>;
  
  // Type-specific configurations
  humanConfig?: Partial<HumanParticipant>;
  aiConfig?: Partial<AIAssistantParticipant>;
  systemConfig?: Partial<SystemParticipant>;
}

/**
 * Create a basic chat participant
 */
export function createChatParticipant(config: CreateParticipantConfig): ChatParticipant {
  const participant: ChatParticipant = {
    id: generateUUID(),
    name: config.name,
    type: config.type,
    created: Date.now(),
    canCommunicate: true,
    displayName: config.displayName || config.name,
    avatar: config.avatar,
    metadata: {
      version: '1.0.0',
      createdBy: 'system',
      ...config.metadata
    }
  };

  return participant;
}

/**
 * Create a human participant
 */
export function createHumanParticipant(
  name: string, 
  config?: Partial<HumanParticipant>
): HumanParticipant {
  const baseParticipant = createChatParticipant({
    name,
    type: 'human',
    ...config
  });

  return {
    ...baseParticipant,
    type: 'human',
    role: 'user',
    status: 'active',
    preferences: {
      communicationStyle: 'natural',
      language: 'en',
      ...config?.preferences
    },
    ...config
  };
}

/**
 * Create an AI assistant participant
 */
export function createAIAssistantParticipant(
  name: string,
  config?: Partial<AIAssistantParticipant>
): AIAssistantParticipant {
  const baseParticipant = createChatParticipant({
    name,
    type: 'ai_assistant',
    ...config
  });

  return {
    ...baseParticipant,
    type: 'ai_assistant',
    model: 'claude-3',
    capabilities: ['text_generation', 'analysis', 'coding', 'math'],
    limitations: ['no_internet', 'no_file_access', 'training_cutoff'],
    provider: 'anthropic',
    contextWindow: 200000,
    ...config
  };
}

/**
 * Create a system participant
 */
export function createSystemParticipant(
  name: string,
  config?: Partial<SystemParticipant>
): SystemParticipant {
  const baseParticipant = createChatParticipant({
    name,
    type: 'system',
    ...config
  });

  return {
    ...baseParticipant,
    type: 'system',
    component: 'chat_system',
    level: 'info',
    automated: true,
    ...config
  };
}

// ==================== PARTICIPANT REGISTRY ====================

/**
 * Registry for managing chat participants
 */
export class ParticipantRegistry {
  private participants: Map<string, ChatParticipant> = new Map();
  private nameIndex: Map<string, string> = new Map();

  /**
   * Register a participant
   */
  register(participant: ChatParticipant): void {
    this.participants.set(participant.id, participant);
    this.nameIndex.set(participant.name.toLowerCase(), participant.id);
  }

  /**
   * Get participant by ID
   */
  getById(id: string): ChatParticipant | undefined {
    return this.participants.get(id);
  }

  /**
   * Get participant by name
   */
  getByName(name: string): ChatParticipant | undefined {
    const id = this.nameIndex.get(name.toLowerCase());
    return id ? this.participants.get(id) : undefined;
  }

  /**
   * Get all participants
   */
  getAll(): ChatParticipant[] {
    return Array.from(this.participants.values());
  }

  /**
   * Get participants by type
   */
  getByType(type: ParticipantType): ChatParticipant[] {
    return this.getAll().filter(p => p.type === type);
  }

  /**
   * Remove participant
   */
  remove(id: string): boolean {
    const participant = this.participants.get(id);
    if (participant) {
      this.participants.delete(id);
      this.nameIndex.delete(participant.name.toLowerCase());
      return true;
    }
    return false;
  }

  /**
   * Check if participant exists
   */
  exists(id: string): boolean {
    return this.participants.has(id);
  }

  /**
   * Get participant count
   */
  count(): number {
    return this.participants.size;
  }

  /**
   * Clear all participants
   */
  clear(): void {
    this.participants.clear();
    this.nameIndex.clear();
  }
}

// ==================== PARTICIPANT VALIDATION ====================

/**
 * Validate a chat participant
 */
export function validateChatParticipant(participant: ChatParticipant): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (!participant.id || participant.id.trim() === '') {
    errors.push('Participant ID is required');
  }

  if (!participant.name || participant.name.trim() === '') {
    errors.push('Participant name is required');
  }

  if (!participant.type) {
    errors.push('Participant type is required');
  }

  if (!participant.created || participant.created <= 0) {
    errors.push('Valid creation timestamp is required');
  }

  if (typeof participant.canCommunicate !== 'boolean') {
    errors.push('canCommunicate must be a boolean');
  }

  return {
    valid: errors.length === 0,
    errors
  };
}

// ==================== PARTICIPANT UTILITIES ====================

/**
 * Check if participant is human
 */
export function isHuman(participant: ChatParticipant): participant is HumanParticipant {
  return participant.type === 'human';
}

/**
 * Check if participant is AI
 */
export function isAI(participant: ChatParticipant): participant is AIAssistantParticipant {
  return participant.type === 'ai_assistant';
}

/**
 * Check if participant is system
 */
export function isSystem(participant: ChatParticipant): participant is SystemParticipant {
  return participant.type === 'system';
}

/**
 * Get participant display name
 */
export function getDisplayName(participant: ChatParticipant): string {
  return participant.displayName || participant.name;
}

/**
 * Check if participant can communicate
 */
export function canCommunicate(participant: ChatParticipant): boolean {
  return participant.canCommunicate === true;
}

/**
 * Clone participant with modifications
 */
export function cloneParticipant(
  participant: ChatParticipant,
  modifications?: Partial<ChatParticipant>
): ChatParticipant {
  return {
    ...participant,
    id: generateUUID(), // Always generate new ID
    created: Date.now(),
    metadata: {
      ...participant.metadata,
      clonedFrom: participant.id,
      clonedAt: Date.now()
    },
    ...modifications
  };
}

// ==================== BUILT-IN PARTICIPANTS ====================

/**
 * Create Claude (AI Assistant) participant
 */
export function createClaudeParticipant(): AIAssistantParticipant {
  return createAIAssistantParticipant('Claude', {
    displayName: 'Claude',
    model: 'claude-3-sonnet',
    capabilities: [
      'text_generation',
      'analysis',
      'coding',
      'math',
      'reasoning',
      'creative_writing',
      'research'
    ],
    limitations: [
      'no_internet_access',
      'no_real_time_data',
      'training_cutoff_april_2024'
    ],
    systemPrompt: 'You are Claude, an AI assistant created by Anthropic.',
    version: '3.0',
    provider: 'anthropic',
    contextWindow: 200000
  });
}

/**
 * Create system participant
 */
export function createContinuumSystemParticipant(): SystemParticipant {
  return createSystemParticipant('Continuum System', {
    displayName: 'System',
    component: 'continuum_core',
    level: 'info',
    automated: true
  });
}

// ==================== EXPORTS ====================

export {
  ChatParticipant,
  ParticipantType,
  HumanParticipant,
  AIAssistantParticipant,
  SystemParticipant,
  CreateParticipantConfig,
  ParticipantRegistry
};