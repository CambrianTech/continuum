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
  type: 'screenshot' | 'file' | 'image' | 'data' | 'benchmark';
  url?: string;
  base64?: string;
  content?: string;
  metadata: Record<string, any>;
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
 * Complete RAG context for LLM inference
 * Domain-agnostic - works for any use case
 */
export interface RAGContext {
  domain: RAGDomain;
  contextId: UUID;  // Room ID, training session ID, game session ID, etc.
  personaId: UUID;

  // Who is the persona?
  identity: PersonaIdentity;

  // Conversation history (public context)
  conversationHistory: LLMMessage[];

  // Attached artifacts (images, files, data)
  artifacts: RAGArtifact[];

  // Private memories (persona's internal knowledge)
  privateMemories: PersonaMemory[];

  // Metadata for debugging/logging
  metadata: {
    messageCount: number;
    artifactCount: number;
    memoryCount: number;
    builtAt: Date;
  };
}

/**
 * Options for building RAG context
 */
export interface RAGBuildOptions {
  maxMessages?: number;  // Limit conversation history (default: 20)
  maxMemories?: number;  // Limit private memories (default: 10)
  includeArtifacts?: boolean;  // Include images/files (default: true)
  includeMemories?: boolean;  // Include private memories (default: true)

  // NEW: Context markers for clarity - what triggered this response?
  triggeringMessageId?: UUID;  // The specific message that triggered this context build
  triggeringTimestamp?: number;  // Cutoff - exclude messages AFTER this (race condition protection)
}
