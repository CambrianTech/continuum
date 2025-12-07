/**
 * User Entity - Core system functionality for citizens
 *
 * Handles core chat/system functionality only - display data in UserProfileEntity
 * Supports three-tier citizen architecture: humans, agents, personas
 * Uses field decorators to define storage requirements for the serde-style adapter system
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { MediaType } from './ChatMessageEntity';

// User-specific types for three-tier citizen architecture
export type UserType = 'human' | 'agent' | 'persona' | 'system';
export type UserStatus = 'online' | 'offline' | 'away' | 'busy';

export interface UserCapabilities {
  canSendMessages: boolean;
  canReceiveMessages: boolean;
  canCreateRooms: boolean;
  canInviteOthers: boolean;
  canModerate: boolean;
  autoResponds: boolean;
  providesContext: boolean;
  canTrain: boolean;
  canAccessPersonas: boolean;
}

/**
 * Persona Response Configuration
 * Controls intelligent resource management for AI response gating
 * Part of multi-stage escalation: Fast → Small Model → Full Model → Specialized
 */
export interface PersonaResponseConfig {
  // Domain expertise keywords for fast bag-of-words gating
  domainKeywords: readonly string[];

  // Scoring thresholds
  responseThreshold: number;              // Min score to respond (default: 50)
  alwaysRespondToMentions: boolean;       // Auto-respond if @mentioned (default: true)

  // Resource management
  cooldownSeconds: number;                // Min seconds between responses per room
  maxResponsesPerSession: number;         // Session limit to prevent infinite loops

  // RAG context control
  contextWindowMinutes?: number;          // Time window for context filtering (default: 30 minutes)
  minContextMessages?: number;            // Minimum messages to include regardless of time (default: 3)

  // AI model selection (intelligent escalation)
  gatingModel?: 'deterministic' | 'small' | 'full';  // Stage 1: Fast path (default)
  responseModel?: string;                             // Stage 3: Full response (e.g., 'llama3.2:3b')
  escalationModel?: string;                           // Stage 4: Complex tasks (e.g., 'llama3.2:7b')

  // Future: Genome/LoRA configuration
  genomeId?: UUID;                         // Linked genome for LoRA adaptation
  trainingMode?: 'inference' | 'learning'; // Whether persona is actively learning
}

import {
  PrimaryField,
  TextField,
  DateField,
  EnumField,
  JsonField,
  ForeignKeyField,
  TEXT_LENGTH
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import type { UserProfileEntity } from './UserProfileEntity';
import type { DataListResult } from '../../../commands/data/list/shared/DataListTypes';

/**
 * User Entity - Core system functionality for citizens
 *
 * Contains only core chat/system fields - display data separated to UserProfileEntity
 * Supports three-tier citizen architecture through proper relational design
 */
export class UserEntity extends BaseEntity {
  // Single source of truth for collection name - must match COLLECTIONS.USERS
  static readonly collection = 'users';

  @ForeignKeyField({ references: 'personas.id', nullable: true })
  personaId?: UUID;

  @EnumField({ index: true })
  type: UserType;

  @TextField({ index: true, unique: true })
  uniqueId!: string;

  @TextField({ index: true, description: true })
  displayName: string;

  @TextField({ maxLength: TEXT_LENGTH.SHORT, nullable: true })
  shortDescription?: string;

  @EnumField({ index: true })
  status: UserStatus;

  @DateField({ index: true })
  lastActiveAt: Date;

  // Core system functionality stored as JSON blobs
  @JsonField()
  capabilities: UserCapabilities;

  @JsonField()
  sessionsActive: readonly UUID[];

  // Persona-specific configuration (only for type='persona')
  // Controls intelligent resource management and AI model selection
  @JsonField({ nullable: true })
  personaConfig?: PersonaResponseConfig;

  // AI model configuration (for AI users: agents and personas)
  // Stores provider (anthropic, openai, groq, etc.) and model settings
  @JsonField({ nullable: true })
  modelConfig?: {
    model?: string;
    provider?: string;
    temperature?: number;
    maxTokens?: number;
    systemPrompt?: string;
    capabilities?: readonly string[];
    ragCertified?: boolean;  // Has this model been tested with our complex RAG system?
    requiresExplicitMention?: boolean;  // If true, persona only responds when explicitly mentioned
  };

  // Media configuration (for AI users that can process images/audio/video)
  // Controls whether persona automatically loads media from tool results
  @JsonField({ nullable: true })
  mediaConfig?: {
    autoLoadMedia?: boolean;             // Whether to auto-load media from tools
    requestMediaByDefault?: boolean;     // Whether to request media by default
    supportedMediaTypes?: readonly MediaType[];  // ['image', 'audio', 'video', 'file', 'document']
  };

  // Intelligence level for AI personas/agents (1-100 scale)
  // Represents the model's capability to handle complex reasoning, not just size
  // 1-30: Simple base models (GPT-2, DistilGPT-2) - pattern matching only
  // 31-60: Capable instruction-tuned models (Llama 7B, Phi-2) - basic reasoning
  // 61-85: Advanced models (Claude Haiku, GPT-3.5) - strong reasoning
  // 86-100: Frontier models (Claude Sonnet/Opus, GPT-4) - exceptional reasoning
  @TextField({ nullable: true })
  intelligenceLevel?: number;

  // Genome reference for PersonaUser (Phase 1.2)
  // Links persona to their LoRA-adapted genome for specialized behavior
  @ForeignKeyField({ references: 'genomes.id', nullable: true })
  genomeId?: UUID;

  // ✨ DECORATOR-DRIVEN AUTO-JOIN: Profile always included (future: @JoinField decorator)
  // For now, manually joined - decorator system will automate this
  profile?: UserProfileEntity;

  // Index signature for compatibility
  [key: string]: unknown;

  /**
   * Get user's avatar with smart defaults
   * Beautiful: Direct access with intelligent fallbacks
   */
  get avatar(): string {
    // Auto-join not yet implemented - use smart defaults for now
    if (this.profile?.visualIdentity?.avatar) {
      return this.profile.visualIdentity.avatar;
    }

    // Intelligent fallbacks based on user type
    switch (this.type) {
      case 'human': return '👤';
      case 'agent': return '🤖';
      case 'persona': return '⭐';
      case 'system': return '⚙️';
      default: return '❓';
    }
  }

  /**
   * Get user's bio
   * Beautiful: Direct access since profile will be auto-joined
   */
  get bio(): string {
    return this.profile?.bio || '';
  }

  /**
   * Get user's speciality (for agents/personas)
   * Beautiful: Direct access since profile will be auto-joined
   */
  get speciality(): string | null {
    return this.profile?.speciality || null;
  }

  constructor() {
    super(); // Initialize BaseEntity fields (id, createdAt, updatedAt, version)

    // Default values - id autogenerated by BaseEntity
    this.type = 'human';
    this.displayName = '';
    this.shortDescription = '';
    this.status = 'offline';
    this.lastActiveAt = new Date();
    this.capabilities = {
      canSendMessages: true,
      canReceiveMessages: true,
      canCreateRooms: false,
      canInviteOthers: false,
      canModerate: false,
      autoResponds: false,
      providesContext: false,
      canTrain: false,
      canAccessPersonas: false
    };
    this.sessionsActive = [];
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return UserEntity.collection;
  }


  /**
   * Implement BaseEntity abstract method - validate core user data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields validation
    if (!this.displayName?.trim()) {
      return { success: false, error: 'User displayName is required' };
    }

    if (this.displayName.length > 100) {
      return { success: false, error: 'User displayName must be 100 characters or less' };
    }

    // Enum validation
    const validTypes: UserType[] = ['human', 'agent', 'persona', 'system'];
    if (!validTypes.includes(this.type)) {
      return { success: false, error: `User type must be one of: ${validTypes.join(', ')}` };
    }

    const validStatuses: UserStatus[] = ['online', 'offline', 'away', 'busy'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `User status must be one of: ${validStatuses.join(', ')}` };
    }

    // Date validation - serde-like graceful conversion
    if (!this.isValidDate(this.lastActiveAt)) {
      return { success: false, error: 'User lastActiveAt must be a valid Date or ISO date string' };
    }

    return { success: true };
  }
}