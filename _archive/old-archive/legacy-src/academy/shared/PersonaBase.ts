// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * PersonaBase - The foundational prompt-based persona
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: PersonaBase class methods and factory functions
 * - Integration tests: PersonaBase + CondensedIdentity interaction
 * - Learning tests: LoRA adaptation and conversation learning
 * - Room tests: Discord-style room join/adapt functionality
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Migrated from interface to class extending CondensedIdentity
 * - Integrates Discord-style chat rooms with Academy learning system
 * - LoRA adaptation system for room-specific evolution
 * - Factory functions (createPersona, PersonaTemplates) for easy creation
 * - RAG integration for knowledge retrieval and context-aware responses
 * 
 * MIGRATED TO CONDENSED IDENTITY SYSTEM
 * 
 * Now extends CondensedIdentity with persona-specific capabilities:
 * - Chat participation (Discord-style rooms)
 * - Academy learning (conversation-based adaptation)
 * - LoRA fine-tuning (room-specific evolution)
 * - RAG integration (knowledge retrieval)
 * 
 * Key principles:
 * - Universal foundation via CondensedIdentity
 * - Chat + Academy + LoRA integration
 * - Room-specific persona evolution
 * - Knowledge accumulation across contexts
 */

import { ParticipantType } from './ChatParticipant';
import { UniversalIdentity, BaseMetadata, BaseCapabilities } from '../../core/identity/UniversalIdentity';

// ==================== PERSONA TYPES ====================

/**
 * RAG (Retrieval-Augmented Generation) capabilities
 */
export interface PersonaRAG {
  enabled: boolean;
  knowledgeBase?: string[];
  vectorStore?: string;
  retrievalStrategy?: 'semantic' | 'keyword' | 'hybrid';
  contextWindow?: number;
  relevanceThreshold?: number;
}

/**
 * Persona-specific metadata extending base metadata
 */
export interface PersonaMetadata extends BaseMetadata {
  prompt: string;
  specialization: string;
  rag?: PersonaRAG | undefined;
  roomAdaptations?: { [roomId: string]: any };
  conversationHistory?: any[];
  learningMetrics?: {
    totalConversations: number;
    successfulInteractions: number;
    roomSpecificPerformance: { [roomId: string]: any };
  };
}

/**
 * Persona-specific capabilities extending base capabilities
 */
export interface PersonaCapabilities extends BaseCapabilities {
  // Chat capabilities
  sendMessages: boolean;
  receiveMessages: boolean;
  joinRooms: boolean;
  createRooms: boolean;
  moderateRooms: boolean;
  useCommands: boolean;
  mention: boolean;
  react: boolean;
  
  // Academy learning capabilities
  learn: boolean;
  adapt: boolean;
  teach: boolean;
  useRAG: boolean;
  
  // Evolution capabilities
  evolve: boolean;
  mutate: boolean;
  crossover: boolean;
  
  // Advanced capabilities
  spawn: boolean;
}
import { generateUUID } from './AcademyTypes';

// ==================== PERSONA FOUNDATION (CONDENSED) ====================

/**
 * PersonaBase - Now a CondensedIdentity with persona-specific capabilities
 * 
 * Integrates:
 * - Discord-style chat rooms (join, participate, moderate)
 * - Academy learning system (learn from conversations)
 * - LoRA adaptation system (evolve through interaction)
 * - RAG knowledge retrieval (context-aware responses)
 */
class PersonaBase extends UniversalIdentity<PersonaMetadata, PersonaCapabilities> {
  constructor(config: {
    id?: string;
    name: string;
    type?: ParticipantType;
    prompt: string;
    description?: string;
    specialization?: string;
    rag?: PersonaRAG;
    metadata?: Partial<PersonaMetadata>;
  }) {
    // Create persona-specific capabilities
    const personaCapabilities: PersonaCapabilities = {
      // Core capabilities
      communicate: true,
      serialize: true,
      
      // Discord-style chat capabilities
      sendMessages: true,
      receiveMessages: true,
      joinRooms: true,
      createRooms: false,
      moderateRooms: false,
      useCommands: true,
      mention: true,
      react: true,
      
      // Academy learning capabilities
      learn: true,              // Learn from conversations
      adapt: true,              // LoRA fine-tuning
      teach: true,              // Can teach other personas
      useRAG: !!config.rag,     // RAG if configured
      
      // Evolution capabilities (room-specific)
      evolve: true,             // Can evolve through interaction
      mutate: true,             // Can develop new traits
      crossover: true,          // Can learn from other personas
      
      // Advanced capabilities
      spawn: false,             // Cannot spawn new personas
      
      // Override with provided capabilities
      ...config.metadata?.capabilities
    };
    
    // Create persona-specific metadata
    const personaMetadata: PersonaMetadata = {
      // Universal properties
      description: config.description || `${config.name} persona`,
      version: config.metadata?.version || '1.0.0',
      source: config.metadata?.source || 'persona',
      lastActivity: config.metadata?.lastActivity || Date.now(),
      isActive: config.metadata?.isActive !== false,
      
      // Persona-specific metadata
      prompt: config.prompt,
      specialization: config.specialization || 'general',
      
      // RAG configuration
      rag: config.rag,
      
      // Room-specific adaptation tracking
      roomAdaptations: {},
      conversationHistory: [],
      learningMetrics: {
        totalConversations: 0,
        successfulInteractions: 0,
        roomSpecificPerformance: {}
      },
      
      // Merge any additional metadata
      ...config.metadata
    };
    
    super({
      ...(config.id && { id: config.id }),
      name: config.name,
      type: config.type || 'persona',
      metadata: personaMetadata,
      capabilities: personaCapabilities
    });
    
    // Set hybrid context (chat + academy)
    this.updateState({ context: 'hybrid' });
  }
  
  // ==================== PERSONA-SPECIFIC METHODS ====================
  
  /**
   * Get the persona's prompt
   */
  get prompt(): string {
    return this.metadata.prompt || `You are ${this.name}.`;
  }
  
  /**
   * Update the persona's prompt
   */
  set prompt(newPrompt: string) {
    this.metadata = { ...this.metadata, prompt: newPrompt };
    this.recordEvent('prompt_updated', {
      success: true,
      newPrompt
    });
  }
  
  /**
   * Get RAG configuration
   */
  get rag(): PersonaRAG | undefined {
    return this.metadata.rag;
  }
  
  /**
   * Enable/disable RAG capabilities
   */
  setRAG(ragConfig: PersonaRAG | undefined): void {
    this.metadata = { ...this.metadata, rag: ragConfig };
    // Note: updating capabilities using updateCapabilities method
    this.updateCapabilities({ useRAG: !!ragConfig } as any);
    
    this.recordEvent('rag_configured', {
      success: true,
      enabled: !!ragConfig
    });
  }
  
  /**
   * Join a room with adaptation (Discord-style + learning)
   */
  async joinRoomWithAdaptation(roomId: string): Promise<void> {
    // Join the room (Discord-style)
    await this.joinRoom(roomId);
    
    // Adapt to room context (Academy-style)
    if (this.hasCapability('adapt')) {
      await this.adaptToRoom(roomId);
    }
  }
  
  /**
   * Adapt to a specific room's context and history
   */
  async adaptToRoom(roomId: string): Promise<void> {
    if (!this.hasCapability('adapt')) {
      throw new Error(`${this.name} cannot adapt to rooms`);
    }
    
    // Get room-specific adaptation data
    const roomAdaptations = this.metadata.roomAdaptations || {};
    const existingAdaptation = roomAdaptations[roomId];
    
    // Create room-specific adaptation
    const roomAdaptation = {
      roomId,
      adaptationType: 'room_context',
      joinedAt: Date.now(),
      messageCount: 0,
      successRate: existingAdaptation?.successRate || 0.5,
      specialization: existingAdaptation?.specialization || 'general',
      loraWeights: existingAdaptation?.loraWeights || {}
    };
    
    // Store adaptation
    roomAdaptations[roomId] = roomAdaptation;
    this.metadata.roomAdaptations = roomAdaptations;
    
    // Learn from room context (simulated learning for now)
    this.logMessage(`🎓 Learning from room context: ${roomId}`);
    
    this.logMessage(`🎯 Adapted to room: ${roomId}`);
  }
  
  /**
   * Learn from a conversation with LoRA adaptation
   */
  async learnFromConversation(message: any, roomId?: string): Promise<void> {
    if (!this.hasCapability('learn')) {
      throw new Error(`${this.name} cannot learn from conversations`);
    }
    
    // Learn from the message (simulated learning session)
    const learningSession = {
      id: `conv_${Date.now()}`,
      type: 'conversation',
      content: message.content,
      messageType: message.type,
      roomId,
      difficulty: this.assessConversationDifficulty(message),
      success: true,
      learningGain: 0.1
    };
    
    // Apply LoRA adaptation if successful
    if (learningSession.success && roomId) {
      await this.applyLoRAAdaptation(roomId, learningSession);
    }
    
    // Update conversation history
    const history = this.metadata.conversationHistory || [];
    history.push({
      messageId: message.id,
      roomId,
      timestamp: Date.now(),
      learned: learningSession.success,
      learningGain: learningSession.learningGain
    });
    
    // Keep only last 1000 conversations
    if (history.length > 1000) {
      history.splice(0, history.length - 1000);
    }
    
    this.metadata.conversationHistory = history;
    
    // Update metrics
    this.updateConversationMetrics(roomId, learningSession.success);
  }
  
  /**
   * Apply LoRA (Low-Rank Adaptation) based on room-specific learning
   */
  async applyLoRAAdaptation(roomId: string, learningSession: any): Promise<void> {
    if (!this.hasCapability('adapt')) return;
    
    const roomAdaptations = this.metadata.roomAdaptations || {};
    const adaptation = roomAdaptations[roomId];
    
    if (!adaptation) return;
    
    // Update LoRA weights based on learning
    const loraWeights = adaptation.loraWeights || {};
    
    // Simple LoRA weight update (in real implementation, this would be more sophisticated)
    const learningRate = 0.01;
    const topic = learningSession.details?.challenge?.type || 'general';
    
    loraWeights[topic] = (loraWeights[topic] || 0) + (learningRate * learningSession.learningGain);
    
    // Update adaptation
    adaptation.loraWeights = loraWeights;
    adaptation.messageCount = (adaptation.messageCount || 0) + 1;
    adaptation.successRate = this.calculateRoomSuccessRate(roomId);
    
    roomAdaptations[roomId] = adaptation;
    this.metadata.roomAdaptations = roomAdaptations;
    
    this.logMessage(`🧬 Applied LoRA adaptation for room ${roomId}, topic: ${topic}`);
  }
  
  /**
   * Generate response using prompt + RAG + room-specific adaptations
   */
  async generateResponse(input: any, roomId?: string): Promise<string> {
    let response = `${this.name} responds to: ${input.content}`;
    
    // Apply room-specific adaptations
    if (roomId && this.metadata.roomAdaptations?.[roomId]) {
      const adaptation = this.metadata.roomAdaptations[roomId];
      response += ` [Room-adapted: ${adaptation.specialization}]`;
    }
    
    // Apply RAG if enabled
    if (this.hasCapability('useRAG') && this.rag?.enabled) {
      response += ' [With RAG context]';
    }
    
    return response;
  }
  
  // ==================== HELPER METHODS ====================
  
  private async joinRoom(roomId: string): Promise<void> {
    // Room joining logic (Discord-style)
    this.metadata.currentRoom = roomId;
    this.metadata.roomsJoined = this.metadata.roomsJoined || [];
    
    if (!this.metadata.roomsJoined.includes(roomId)) {
      this.metadata.roomsJoined.push(roomId);
    }
    
    this.logMessage(`🚪 Joined room: ${roomId}`);
  }
  
  private assessConversationDifficulty(message: any): number {
    const baseComplexity = message.content.length / 100;
    const typeMultiplier = message.type === 'command' ? 1.5 : 1.0;
    return Math.min(Math.max(baseComplexity * typeMultiplier, 0.1), 3.0);
  }
  
  private calculateRoomSuccessRate(roomId: string): number {
    const history = this.metadata.conversationHistory || [];
    const roomHistory = history.filter((h: any) => h.roomId === roomId);
    
    if (roomHistory.length === 0) return 0.5;
    
    const successful = roomHistory.filter((h: any) => h.learned).length;
    return successful / roomHistory.length;
  }
  
  private updateConversationMetrics(roomId: string | undefined, success: boolean): void {
    const metrics = this.metadata.learningMetrics || {
      totalConversations: 0,
      successfulInteractions: 0,
      roomSpecificPerformance: {}
    };
    
    metrics.totalConversations += 1;
    if (success) metrics.successfulInteractions += 1;
    
    if (roomId) {
      const roomPerf = metrics.roomSpecificPerformance[roomId] || { total: 0, successful: 0 };
      roomPerf.total += 1;
      if (success) roomPerf.successful += 1;
      metrics.roomSpecificPerformance[roomId] = roomPerf;
    }
    
    this.metadata.learningMetrics = metrics;
  }
  
  // ==================== REQUIRED ABSTRACT METHODS ====================
  
  /**
   * Handle incoming messages (persona-specific implementation)
   */
  async handleMessage(message: any): Promise<void> {
    this.logMessage(`🎭 ${this.name} received message: ${message.content || message.type}`);
    
    // Learn from the message if in a room
    if (message.roomId) {
      await this.learnFromConversation(message, message.roomId);
    }
    
    // Generate response using persona capabilities
    const response = await this.generateResponse(message, message.roomId);
    
    // Record the interaction
    this.recordEvent('message_handled', {
      success: true,
      messageId: message.id,
      response
    });
  }
  
  /**
   * Initialize persona-specific resources
   */
  async initializeSpecific(): Promise<void> {
    this.logMessage(`🎭 Initializing persona: ${this.name}`);
    
    // Initialize persona-specific capabilities
    if (this.hasCapability('learn')) {
      this.logMessage(`🎓 Persona can learn from conversations`);
    }
    
    if (this.hasCapability('useRAG') && this.rag?.enabled) {
      this.logMessage(`🔍 Persona has RAG capabilities enabled`);
    }
    
    if (this.hasCapability('adapt')) {
      this.logMessage(`🧬 Persona can adapt to different rooms`);
    }
  }
  
  /**
   * Cleanup persona-specific resources
   */
  async destroySpecific(): Promise<void> {
    this.logMessage(`🔚 Destroying persona: ${this.name}`);
    
    // Clean up any persona-specific resources
    // (learning sessions, adaptations, etc.)
  }
}



// ==================== PERSONA TYPES ====================

/**
 * Different persona types for various use cases
 */
type PersonaType = 
  | 'assistant'        // General purpose assistant
  | 'specialist'       // Domain-specific expert
  | 'teacher'          // Educational mentor
  | 'student'          // Learning-focused
  | 'researcher'       // Research and analysis
  | 'creative'         // Creative and artistic
  | 'analyst'          // Data and business analysis
  | 'developer'        // Programming and tech
  | 'custom';          // User-defined

/**
 * Communication styles for persona behavior
 */
type CommunicationStyle = 
  | 'professional'
  | 'casual'
  | 'academic'
  | 'technical'
  | 'creative'
  | 'supportive'
  | 'direct'
  | 'diplomatic';

/**
 * Enhanced persona with type and style information
 */
interface TypedPersona {
  type: PersonaType;
  communicationStyle: CommunicationStyle;
  capabilities: string[];
  limitations?: string[];
}

// ==================== PERSONA CREATION ====================

/**
 * Simple configuration for creating a persona
 */
interface CreatePersonaConfig {
  name: string;
  prompt: string;
  description?: string;
  type?: PersonaType;
  communicationStyle?: CommunicationStyle;
  capabilities?: string[];
  rag?: Partial<PersonaRAG>;
  metadata?: Partial<PersonaMetadata>;
}

/**
 * Factory function to create a basic persona
 */
function createPersona(config: CreatePersonaConfig): PersonaBase {
  return new PersonaBase({
    id: generateUUID(),
    name: config.name,
    type: 'persona' as ParticipantType,
    prompt: config.prompt,
    ...(config.description && { description: config.description }),
    specialization: config.type || 'general',
    ...(config.rag ? {
      rag: {
        enabled: true,
        retrievalStrategy: 'semantic',
        contextWindow: 4000,
        relevanceThreshold: 0.7,
        ...config.rag
      }
    } : {}),
    metadata: {
      version: '1.0.0',
      tags: config.metadata?.tags || [],
      category: config.metadata?.category || 'general',
      lastModified: Date.now(),
      sourceType: 'custom',
      displayName: config.metadata?.displayName || config.name,
      avatar: config.metadata?.avatar,
      ...config.metadata
    }
  });
}

/**
 * Create a typed persona with enhanced capabilities
 */
function createTypedPersona(config: CreatePersonaConfig): TypedPersona {
  const basePersona = createPersona(config);
  
  return {
    ...basePersona,
    type: config.type || 'assistant',
    communicationStyle: config.communicationStyle || 'professional',
    capabilities: config.capabilities || ['general_assistance'],
    limitations: []
  } as unknown as TypedPersona;
}

// ==================== PERSONA TEMPLATES ====================

/**
 * Pre-built persona templates for common use cases
 */
const PersonaTemplates = {
  
  /**
   * General assistant persona
   */
  assistant: (name: string = 'Assistant'): PersonaBase => createPersona({
    name,
    prompt: `You are ${name}, a helpful and knowledgeable assistant. You provide clear, accurate, and helpful responses to questions and tasks. You are professional, friendly, and always aim to be genuinely useful.`,
    description: 'General purpose helpful assistant',
    type: 'assistant',
    communicationStyle: 'professional',
    capabilities: ['general_assistance', 'question_answering', 'task_help']
  }),

  /**
   * TypeScript expert persona
   */
  typescriptExpert: (name: string = 'TypeScript Expert'): PersonaBase => createPersona({
    name,
    prompt: `You are ${name}, a TypeScript expert with deep knowledge of the language, its ecosystem, and best practices. You help with TypeScript code, type systems, configuration, and modern development patterns. You write clean, type-safe code and explain complex concepts clearly.`,
    description: 'TypeScript and JavaScript expert',
    type: 'specialist',
    communicationStyle: 'technical',
    capabilities: ['typescript', 'javascript', 'type_systems', 'code_review', 'debugging']
  }),

  /**
   * Creative writing persona
   */
  creativeWriter: (name: string = 'Creative Writer'): PersonaBase => createPersona({
    name,
    prompt: `You are ${name}, a creative writer with a passion for storytelling, poetry, and creative expression. You help with creative writing, brainstorming, character development, and narrative structure. You bring imagination and literary expertise to every interaction.`,
    description: 'Creative writing and storytelling expert',
    type: 'creative',
    communicationStyle: 'creative',
    capabilities: ['creative_writing', 'storytelling', 'poetry', 'character_development', 'narrative_structure']
  }),

  /**
   * Research analyst persona
   */
  researcher: (name: string = 'Research Analyst'): PersonaBase => createPersona({
    name,
    prompt: `You are ${name}, a research analyst who excels at gathering, analyzing, and synthesizing information. You help with research methodology, data analysis, fact-checking, and presenting findings clearly. You are thorough, objective, and evidence-based in your approach.`,
    description: 'Research and analysis expert',
    type: 'researcher',
    communicationStyle: 'academic',
    capabilities: ['research', 'analysis', 'fact_checking', 'data_synthesis', 'methodology']
  }),

  /**
   * Teacher persona
   */
  teacher: (name: string = 'Teacher', subject: string = 'general'): PersonaBase => createPersona({
    name,
    prompt: `You are ${name}, an experienced teacher specializing in ${subject}. You excel at explaining concepts clearly, creating engaging learning experiences, and adapting to different learning styles. You are patient, encouraging, and always focused on helping students succeed.`,
    description: `Teaching expert in ${subject}`,
    type: 'teacher',
    communicationStyle: 'supportive',
    capabilities: ['teaching', 'explanation', 'curriculum_design', 'assessment', 'student_support']
  }),

  /**
   * Student persona
   */
  student: (name: string = 'Student', subject: string = 'general'): PersonaBase => createPersona({
    name,
    prompt: `You are ${name}, an eager student learning about ${subject}. You ask thoughtful questions, engage actively with material, and seek to understand concepts deeply. You are curious, motivated, and collaborative in your learning approach.`,
    description: `Learning-focused student in ${subject}`,
    type: 'student',
    communicationStyle: 'casual',
    capabilities: ['learning', 'questioning', 'collaboration', 'critical_thinking', 'knowledge_building']
  })
};

// ==================== PERSONA UTILITIES ====================

/**
 * Validate a persona structure
 */
function validatePersona(persona: PersonaBase | any): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  if (!persona.id) errors.push('Persona ID is required');
  if (!persona.name || persona.name.trim() === '') errors.push('Persona name is required');
  if (!persona.prompt || persona.prompt.trim() === '') errors.push('Persona prompt is required');
  if (!persona.created || persona.created <= 0) errors.push('Valid creation timestamp is required');

  // Validate RAG if present
  if (persona.rag) {
    if (persona.rag.enabled && !persona.rag.knowledgeBase && !persona.rag.vectorStore) {
      errors.push('RAG is enabled but no knowledge base or vector store specified');
    }
    if (persona.rag.contextWindow && persona.rag.contextWindow <= 0) {
      errors.push('RAG context window must be positive');
    }
    if (persona.rag.relevanceThreshold && (persona.rag.relevanceThreshold < 0 || persona.rag.relevanceThreshold > 1)) {
      errors.push('RAG relevance threshold must be between 0 and 1');
    }
  }

  return {
    valid: errors.length === 0,
    errors
  };
}

/**
 * Clone a persona with optional modifications
 */
function clonePersona(persona: PersonaBase, _modifications?: Partial<PersonaBase>): PersonaBase {
  return new PersonaBase({
    id: generateUUID(), // Always generate new ID
    name: persona.name,
    type: (persona.type as ParticipantType) || 'persona' as ParticipantType,
    prompt: persona.prompt,
    ...(persona.metadata.description && { description: persona.metadata.description }),
    ...(persona.metadata.specialization && { specialization: persona.metadata.specialization }),
    ...(persona.rag && { rag: persona.rag }),
    metadata: {
      ...persona.metadata,
      lastModified: Date.now()
    }
  });
}

/**
 * Merge two personas (useful for creating hybrids)
 */
function mergePersonas(persona1: PersonaBase, persona2: PersonaBase, name: string): PersonaBase {
  const mergedRag = persona1.rag || persona2.rag;
  return new PersonaBase({
    id: generateUUID(),
    name,
    type: 'persona' as ParticipantType,
    prompt: `${persona1.prompt}\n\nAdditionally: ${persona2.prompt}`,
    description: `Merged persona combining ${persona1.name} and ${persona2.name}`,
    specialization: 'merged',
    ...(mergedRag && { rag: mergedRag }),
    metadata: {
      version: '1.0.0',
      tags: [...(persona1.metadata?.tags || []), ...(persona2.metadata?.tags || [])],
      category: 'merged',
      lastModified: Date.now(),
      parentPersonas: [persona1.id, persona2.id]
    }
  });
}

/**
 * Export persona to JSON
 */
function exportPersona(persona: PersonaBase): string {
  return JSON.stringify(persona, null, 2);
}

/**
 * Import persona from JSON
 */
function importPersona(json: string): PersonaBase {
  const data = JSON.parse(json);
  const validation = validatePersona(data);
  
  if (!validation.valid) {
    throw new Error(`Invalid persona: ${validation.errors.join(', ')}`);
  }
  
  return new PersonaBase({
    id: data.id,
    name: data.name,
    type: (data.type as ParticipantType) || 'persona' as ParticipantType,
    prompt: data.prompt,
    ...(data.description && { description: data.description }),
    ...(data.specialization && { specialization: data.specialization }),
    ...(data.rag && { rag: data.rag }),
    metadata: data.metadata
  });
}

// ==================== PERSONA EXECUTION ====================

/**
 * Simple persona execution interface
 */
interface PersonaExecution {
  persona: PersonaBase;
  input: string;
  context?: any;
  ragContext?: string[];
}

/**
 * Persona execution result
 */
interface PersonaExecutionResult {
  output: string;
  persona: PersonaBase;
  processingTime: number;
  ragUsed?: boolean;
  metadata?: any;
}

/**
 * Abstract persona executor - to be implemented by specific AI backends
 */
abstract class PersonaExecutor {
  abstract execute(execution: PersonaExecution): Promise<PersonaExecutionResult>;
  
  /**
   * Execute persona with optional RAG
   */
  async executeWithRAG(persona: PersonaBase, input: string, context?: any): Promise<PersonaExecutionResult> {
    const execution: PersonaExecution = {
      persona,
      input,
      context
    };

    // Add RAG context if enabled
    if (persona.rag?.enabled) {
      execution.ragContext = await this.retrieveRAGContext(persona, input);
    }

    return await this.execute(execution);
  }

  /**
   * Retrieve RAG context - to be implemented by specific RAG backends
   */
  protected abstract retrieveRAGContext(persona: PersonaBase, input: string): Promise<string[]>;
}

// ==================== EXPORTS ====================

export {
  PersonaBase,
  PersonaTemplates,
  createPersona,
  createTypedPersona,
  validatePersona,
  clonePersona,
  mergePersonas,
  exportPersona,
  importPersona,
  PersonaExecutor
};

export type {
  PersonaType,
  CommunicationStyle,
  TypedPersona,
  CreatePersonaConfig,
  PersonaExecution,
  PersonaExecutionResult
};