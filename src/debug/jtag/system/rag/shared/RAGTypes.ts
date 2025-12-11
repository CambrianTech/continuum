/**
 * RAG (Retrieval-Augmented Generation) Types
 *
 * Domain-agnostic types for building LLM context from various sources:
 * - Chat conversations (chat rooms)
 * - Training sessions (academy with benchmarks)
 * - Game sessions (screenshots + state)
 * - Code review (files + diffs)
 *
 * Architecture: Adapter pattern - each domain implements RAGBuilder interface
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Domain types that can provide RAG context
 */
export type RAGDomain = 'chat' | 'academy' | 'game' | 'code' | 'analysis';

/**
 * Model capabilities that affect RAG context building
 * Determines how artifacts (images, etc.) are processed
 */
export type ModelCapability =
  | 'text'                  // Basic text generation
  | 'vision'                // Can process images directly
  | 'function-calling'      // Supports tool/function calls
  | 'streaming'             // Supports streaming responses
  | 'embeddings'            // Can generate embeddings
  | 'multimodal';           // Supports multiple input types

/**
 * Model capability profile
 * Each model adapter reports its capabilities
 */
export interface ModelCapabilities {
  readonly modelId: string;
  readonly providerId: string;
  readonly capabilities: ModelCapability[];
  readonly maxContextTokens: number;
  readonly supportsImages: boolean;
  readonly supportsFunctionCalling: boolean;
  readonly supportsStreaming: boolean;
}

/**
 * LLM message format (OpenAI/Anthropic compatible)
 * Matches ChatMessage from AIProviderTypes
 */
export interface LLMMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
  name?: string;  // Optional speaker name
  timestamp?: number;  // Unix timestamp in milliseconds
}

/**
 * Artifact types that can be attached to context
 * Vision models can process images, code analyzers can read files, etc.
 */
export interface RAGArtifact {
  type: 'screenshot' | 'file' | 'image' | 'data' | 'benchmark' | 'video' | 'audio';
  url?: string;
  base64?: string;
  content?: string;
  metadata: Record<string, unknown>;

  // NEW: Preprocessing results for text-only models
  preprocessed?: {
    type: 'yolo_detection' | 'image_description' | 'ocr' | 'video_summary' | 'audio_transcript';
    result: string | Record<string, unknown>;
    confidence?: number;
    processingTime?: number;
    model?: string;
  };
}

/**
 * Private memories - persona's internal knowledge
 * Not included in public conversation, but informs responses
 */
export interface PersonaMemory {
  id: UUID;
  type: 'observation' | 'pattern' | 'reflection' | 'preference' | 'goal';
  content: string;
  timestamp: Date;
  relevanceScore: number;  // 0-1, for future retrieval ranking
}

/**
 * Persona identity - who the AI thinks it is
 */
export interface PersonaIdentity {
  name: string;
  bio?: string;
  role?: string;
  systemPrompt: string;  // Detailed instructions for behavior
  capabilities?: string[];
}

/**
 * Recipe strategy - conversation governance rules
 * Loaded from system/recipes/*.json
 */
export interface RecipeStrategy {
  conversationPattern: string;  // 'collaborative', 'human-focused', etc.
  responseRules: string[];  // Rules AIs should follow
  decisionCriteria: string[];  // What to consider when deciding to respond
}

/**
 * Complete RAG context for LLM inference
 * Domain-agnostic - works for any use case
 */
export interface RAGContext {
  domain: RAGDomain;
  contextId: UUID;  // Room ID, training session ID, game session ID, etc.
  personaId: UUID;

  // Who is the persona?
  identity: PersonaIdentity;

  // Conversation governance rules (from recipe)
  recipeStrategy?: RecipeStrategy;

  // Conversation history (public context)
  conversationHistory: LLMMessage[];

  // Attached artifacts (images, files, data)
  artifacts: RAGArtifact[];

  // Private memories (persona's internal knowledge)
  privateMemories: PersonaMemory[];

  // Learning mode configuration (Phase 2: Per-participant learning)
  learningMode?: 'fine-tuning' | 'inference-only';
  genomeId?: UUID;
  participantRole?: string;  // 'student', 'teacher', 'reviewer', etc.

  // Metadata for debugging/logging
  metadata: {
    messageCount: number;
    artifactCount: number;
    memoryCount: number;
    builtAt: Date;
    recipeId?: string;
    recipeName?: string;

    // Bug #5 fix: Two-dimensional budget calculation
    adjustedMaxTokens?: number;  // Dynamically adjusted completion token limit based on input size
    inputTokenCount?: number;    // Estimated tokens in conversationHistory
  };
}

/**
 * Options for building RAG context
 */
export interface RAGBuildOptions {
  maxMessages?: number;  // Limit conversation history (default: model-aware calculation)
  maxMemories?: number;  // Limit private memories (default: 10)
  includeArtifacts?: boolean;  // Include images/files (default: true)
  includeMemories?: boolean;  // Include private memories (default: true)

  // Context markers for clarity - what triggered this response?
  triggeringMessageId?: UUID;  // The specific message that triggered this context build
  triggeringTimestamp?: number;  // Cutoff - exclude messages AFTER this (race condition protection)

  // Current message being responded to (if not yet persisted to database)
  currentMessage?: LLMMessage;  // Include this message even if not in database yet

  // NEW: Task completion tracking - prevent infinite loops
  excludeMessageIds?: UUID[];  // Message IDs to exclude from RAG context (e.g., processed tool results)

  // NEW: Model-aware context budgeting (Bug #5 fix)
  modelId?: string;  // Target model ID for calculating safe message count based on context window
  maxTokens?: number;  // Max completion tokens (default: 3000)
  systemPromptTokens?: number;  // Estimated system prompt tokens (default: 500)

  // NEW: Model capability-aware processing
  modelCapabilities?: ModelCapabilities;  // Target model's capabilities
  preprocessImages?: boolean;  // Force preprocessing even for vision models (default: auto-detect)
  yoloEndpoint?: string;  // YOLO detection service endpoint (default: local)
  imageDescriptionModel?: string;  // Model for generating image descriptions
}
