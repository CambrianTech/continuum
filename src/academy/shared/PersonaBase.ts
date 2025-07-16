/**
 * PersonaBase - The foundational prompt-based persona
 * 
 * Extends ChatParticipant to create personas that can participate in chat
 * while adding prompt-based AI capabilities and RAG support.
 * 
 * Key principles:
 * - Extends universal ChatParticipant foundation
 * - Simple prompt + optional RAG = complete functional persona
 * - Can work independently outside Academy
 * - Easy integration - any chat participant can become a persona
 * - Foundation for all persona types
 */

import { ChatParticipant, ParticipantType } from './ChatParticipant';
import { generateUUID } from './AcademyTypes';

// ==================== CORE PERSONA FOUNDATION ====================

/**
 * Core prompt-based persona - extends ChatParticipant with AI capabilities
 */
export interface PersonaBase extends ChatParticipant {
  // Inherited from ChatParticipant:
  // - id: string
  // - name: string
  // - type: ParticipantType
  // - created: number
  // - canCommunicate: boolean
  // - displayName?: string
  // - avatar?: string
  // - metadata?: Record<string, any>

  // Persona-specific enhancements
  prompt: string;
  description?: string;
  
  // Optional RAG capabilities
  rag?: PersonaRAG;
}

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
 * Persona-specific metadata extensions
 * (Base metadata is inherited from ChatParticipant)
 */
export interface PersonaMetadata {
  // Persona-specific properties
  version?: string;
  tags?: string[];
  category?: string;
  author?: string;
  lastModified?: number;
  
  // Persona provenance
  sourceType?: 'template' | 'imported' | 'custom' | 'evolved';
  originalPrompt?: string;
  
  // Performance tracking
  usageCount?: number;
  successRate?: number;
  
  // Extensible for any additional properties
  [key: string]: any;
}

// ==================== PERSONA TYPES ====================

/**
 * Different persona types for various use cases
 */
export type PersonaType = 
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
export type CommunicationStyle = 
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
export interface TypedPersona extends PersonaBase {
  type: PersonaType;
  communicationStyle: CommunicationStyle;
  capabilities: string[];
  limitations?: string[];
}

// ==================== PERSONA CREATION ====================

/**
 * Simple configuration for creating a persona
 */
export interface CreatePersonaConfig {
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
export function createPersona(config: CreatePersonaConfig): PersonaBase {
  const persona: PersonaBase = {
    // ChatParticipant properties
    id: generateUUID(),
    name: config.name,
    type: 'persona',
    created: Date.now(),
    canCommunicate: true,
    displayName: config.name,
    avatar: config.metadata?.avatar,
    metadata: {
      version: '1.0.0',
      tags: [],
      category: 'general',
      lastModified: Date.now(),
      sourceType: 'custom',
      ...config.metadata
    },
    
    // PersonaBase properties
    prompt: config.prompt,
    description: config.description
  };

  // Add RAG if provided
  if (config.rag) {
    persona.rag = {
      enabled: true,
      retrievalStrategy: 'semantic',
      contextWindow: 4000,
      relevanceThreshold: 0.7,
      ...config.rag
    };
  }

  return persona;
}

/**
 * Create a typed persona with enhanced capabilities
 */
export function createTypedPersona(config: CreatePersonaConfig): TypedPersona {
  const basePersona = createPersona(config);
  
  return {
    ...basePersona,
    type: config.type || 'assistant',
    communicationStyle: config.communicationStyle || 'professional',
    capabilities: config.capabilities || ['general_assistance'],
    limitations: []
  };
}

// ==================== PERSONA TEMPLATES ====================

/**
 * Pre-built persona templates for common use cases
 */
export const PersonaTemplates = {
  
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
export function validatePersona(persona: PersonaBase): { valid: boolean; errors: string[] } {
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
export function clonePersona(persona: PersonaBase, modifications?: Partial<PersonaBase>): PersonaBase {
  return {
    ...persona,
    id: generateUUID(), // Always generate new ID
    created: Date.now(),
    metadata: {
      ...persona.metadata,
      lastModified: Date.now()
    },
    ...modifications
  };
}

/**
 * Merge two personas (useful for creating hybrids)
 */
export function mergePersonas(persona1: PersonaBase, persona2: PersonaBase, name: string): PersonaBase {
  return {
    id: generateUUID(),
    name,
    prompt: `${persona1.prompt}\n\nAdditionally: ${persona2.prompt}`,
    description: `Merged persona combining ${persona1.name} and ${persona2.name}`,
    created: Date.now(),
    rag: persona1.rag || persona2.rag,
    metadata: {
      version: '1.0.0',
      tags: [...(persona1.metadata?.tags || []), ...(persona2.metadata?.tags || [])],
      category: 'merged',
      lastModified: Date.now(),
      parentPersonas: [persona1.id, persona2.id]
    }
  };
}

/**
 * Export persona to JSON
 */
export function exportPersona(persona: PersonaBase): string {
  return JSON.stringify(persona, null, 2);
}

/**
 * Import persona from JSON
 */
export function importPersona(json: string): PersonaBase {
  const persona = JSON.parse(json);
  const validation = validatePersona(persona);
  
  if (!validation.valid) {
    throw new Error(`Invalid persona: ${validation.errors.join(', ')}`);
  }
  
  return persona;
}

// ==================== PERSONA EXECUTION ====================

/**
 * Simple persona execution interface
 */
export interface PersonaExecution {
  persona: PersonaBase;
  input: string;
  context?: any;
  ragContext?: string[];
}

/**
 * Persona execution result
 */
export interface PersonaExecutionResult {
  output: string;
  persona: PersonaBase;
  processingTime: number;
  ragUsed?: boolean;
  metadata?: any;
}

/**
 * Abstract persona executor - to be implemented by specific AI backends
 */
export abstract class PersonaExecutor {
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
  PersonaRAG,
  PersonaMetadata,
  PersonaType,
  CommunicationStyle,
  TypedPersona,
  CreatePersonaConfig,
  PersonaExecution,
  PersonaExecutionResult,
  PersonaExecutor
};