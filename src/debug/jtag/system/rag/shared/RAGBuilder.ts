/**
 * RAGBuilder - Abstract interface for building LLM context
 *
 * Adapter pattern: Each domain (chat, academy, game) implements this interface
 * to provide context in a standardized format.
 *
 * PersonaUser doesn't care HOW context is built - it just calls buildContext()
 * and gets back a RAGContext with everything needed for LLM inference.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { RAGContext, RAGBuildOptions, RAGDomain } from './RAGTypes';

/**
 * Abstract RAG builder interface
 * Each domain implements this to provide context for LLM inference
 */
export abstract class RAGBuilder {
  /**
   * Domain identifier (chat, academy, game, etc.)
   */
  abstract readonly domain: RAGDomain;

  /**
   * Build RAG context for a persona in a given context
   *
   * @param contextId - Room ID, training session ID, game session ID, etc.
   * @param personaId - The persona requesting context
   * @param options - Optional configuration for context building
   * @returns Complete RAG context ready for LLM inference
   */
  abstract buildContext(
    contextId: UUID,
    personaId: UUID,
    options?: RAGBuildOptions
  ): Promise<RAGContext>;

  /**
   * Get a human-readable description of this builder
   * Used for logging and debugging
   */
  abstract getDescription(): string;
}

/**
 * RAG Builder Factory
 * Returns the appropriate builder for a given domain
 */
export class RAGBuilderFactory {
  private static builders: Map<RAGDomain, RAGBuilder> = new Map();

  /**
   * Register a builder for a domain
   */
  static register(domain: RAGDomain, builder: RAGBuilder): void {
    this.builders.set(domain, builder);
    console.log(`📚 RAG Builder registered: ${domain} (${builder.getDescription()})`);
  }

  /**
   * Get builder for a domain
   * Throws if no builder registered
   */
  static getBuilder(domain: RAGDomain): RAGBuilder {
    const builder = this.builders.get(domain);
    if (!builder) {
      throw new Error(`No RAG builder registered for domain: ${domain}`);
    }
    return builder;
  }

  /**
   * Check if a builder is registered for a domain
   */
  static hasBuilder(domain: RAGDomain): boolean {
    return this.builders.has(domain);
  }

  /**
   * Get all registered domains
   */
  static getRegisteredDomains(): RAGDomain[] {
    return Array.from(this.builders.keys());
  }
}
