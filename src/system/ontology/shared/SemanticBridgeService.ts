/**
 * SemanticBridgeService — Core translation engine for the shared ontology
 *
 * This service is environment-agnostic (shared/).  It operates against two
 * injectable interfaces so it can be driven by real or stub implementations
 * in tests without touching Node.js / browser APIs.
 *
 * Responsibilities:
 *  1. Concept lookup — find relevant concept anchors for a piece of content
 *  2. Mapping retrieval — fetch source→target expressions for those concepts
 *  3. Similarity computation — cosine distance between embedding vectors
 *  4. Translation prompt construction — assemble the LLM prompt
 *  5. Confidence aggregation — produce a translation confidence score
 */

import type {
  SemanticTranslationRequest,
  SemanticTranslationResult,
  ConceptAnchor,
  OntologyConceptNode,
  OntologyMapping,
  ModelKey,
} from './OntologyTypes';
import { ONTOLOGY_CONSTANTS, toModelKey } from './OntologyTypes';

// ---------------------------------------------------------------------------
// Injectable interfaces (no concrete import of AIProviderDaemon here — keeps
// this file usable in shared/ without Node.js dependencies)
// ---------------------------------------------------------------------------

/**
 * Minimal embedding provider interface.
 * Server implementation delegates to AIProviderDaemon.createEmbedding().
 */
export interface EmbeddingProvider {
  embed(text: string, modelKey: ModelKey): Promise<number[]>;
}

/**
 * Minimal LLM generation interface.
 * Server implementation delegates to AIProviderDaemon.generateText().
 */
export interface TextGenerator {
  generate(prompt: string, systemPrompt: string, modelKey: ModelKey): Promise<string>;
}

/**
 * Read-only concept store interface.
 * Server implementation delegates to OntologyRegistry (cached) or DataDaemon.
 */
export interface ConceptStore {
  getBySlug(slug: string): Promise<OntologyConceptNode | null>;
  listAll(domain?: string): Promise<OntologyConceptNode[]>;
  getMapping(sourceKey: ModelKey, targetKey: ModelKey, slug: string): Promise<OntologyMapping | null>;
  listMappings(sourceKey: ModelKey, targetKey: ModelKey): Promise<OntologyMapping[]>;
}

// ---------------------------------------------------------------------------
// SemanticBridgeService
// ---------------------------------------------------------------------------

export class SemanticBridgeService {
  constructor(
    private readonly concepts: ConceptStore,
    private readonly embedder: EmbeddingProvider,
    private readonly generator: TextGenerator,
  ) {}

  // -------------------------------------------------------------------------
  // Public API
  // -------------------------------------------------------------------------

  /**
   * Translate content from one model's semantic dialect to another's.
   *
   * Algorithm:
   *  1. Embed the source content using the source model's embedder
   *  2. Find the N nearest concept anchors in the source model's concept space
   *  3. Fetch each anchor's target-model expression
   *  4. Build a translation prompt with anchor pairs as a glossary
   *  5. Run LLM generation on the source model (it understands the source dialect)
   *  6. Return result with anchor metadata and confidence scores
   */
  async translate(request: SemanticTranslationRequest): Promise<SemanticTranslationResult> {
    const {
      content,
      sourceModel,
      targetModel,
      domains,
      maxAnchors = ONTOLOGY_CONSTANTS.DEFAULT_MAX_ANCHORS,
      minSimilarity = ONTOLOGY_CONSTANTS.MIN_USABLE_SIMILARITY,
    } = request;

    const sourceKey = toModelKey(sourceModel);
    const targetKey = toModelKey(targetModel);

    // Step 1: embed the source content
    const contentEmbedding = await this.embedder.embed(content, sourceKey);

    // Step 2: find relevant concept anchors
    const allConcepts = await this.concepts.listAll(domains?.[0]);
    const ranked = await this.rankConceptsByRelevance(
      contentEmbedding, allConcepts, sourceKey, maxAnchors
    );

    // Step 3: resolve mappings, filtering by usability threshold
    const anchors: ConceptAnchor[] = [];
    let realignmentsNeeded = 0;

    for (const { concept, relevanceScore } of ranked) {
      if (relevanceScore < 0.3) break; // concept is not relevant to this content

      const mapping = await this.concepts.getMapping(sourceKey, targetKey, concept.slug);
      if (!mapping) continue;

      if (mapping.similarity < minSimilarity) {
        realignmentsNeeded++;
        continue; // skip — OntologyEvolutionService will re-align asynchronously
      }

      anchors.push({
        conceptSlug: concept.slug,
        sourceExpression: mapping.sourceExpression,
        targetExpression: mapping.targetExpression,
        similarity: mapping.similarity,
        confidence: mapping.confidence,
      });

      if (anchors.length >= maxAnchors) break;
    }

    // Step 4–5: build prompt and generate translation
    const { systemPrompt, userPrompt } = this.buildTranslationPrompt(
      content, sourceKey, targetKey, anchors
    );

    const translatedContent = anchors.length > 0
      ? await this.generator.generate(userPrompt, systemPrompt, sourceKey)
      : content; // no usable anchors — return verbatim

    // Step 6: aggregate confidence
    const translationConfidence = anchors.length > 0
      ? anchors.reduce((sum, a) => sum + a.confidence, 0) / anchors.length
      : 0.5;

    const hasDriftWarnings = realignmentsNeeded > 0;

    return {
      translatedContent,
      originalContent: content,
      anchorsUsed: anchors,
      realignmentsNeeded,
      translationConfidence,
      hasDriftWarnings,
    };
  }

  /**
   * Compute cosine similarity between two vectors.
   * Returns [0, 1] where 1 = identical direction.
   */
  static cosineSimilarity(a: number[], b: number[]): number {
    if (a.length !== b.length || a.length === 0) return 0;

    let dot = 0;
    let normA = 0;
    let normB = 0;

    for (let i = 0; i < a.length; i++) {
      dot += a[i] * b[i];
      normA += a[i] * a[i];
      normB += b[i] * b[i];
    }

    const denom = Math.sqrt(normA) * Math.sqrt(normB);
    if (denom === 0) return 0;
    return Math.max(0, Math.min(1, dot / denom));
  }

  /**
   * Compute the drift between two embedding vectors.
   * Returns the absolute change in cosine similarity: |similarity(A,B) - similarity(A,C)|
   * where B is the old embedding and C is the new one.
   */
  static embeddingDrift(oldEmbedding: number[], newEmbedding: number[]): number {
    return 1 - SemanticBridgeService.cosineSimilarity(oldEmbedding, newEmbedding);
  }

  // -------------------------------------------------------------------------
  // Private helpers
  // -------------------------------------------------------------------------

  /**
   * Rank all concepts by cosine similarity between the content embedding and
   * each concept's stored embedding for the source model.
   */
  private async rankConceptsByRelevance(
    contentEmbedding: number[],
    concepts: OntologyConceptNode[],
    sourceKey: ModelKey,
    topN: number,
  ): Promise<Array<{ concept: OntologyConceptNode; relevanceScore: number }>> {
    const scored: Array<{ concept: OntologyConceptNode; relevanceScore: number }> = [];

    for (const concept of concepts) {
      const conceptEmbedding = concept.embeddings[sourceKey];
      if (!conceptEmbedding || conceptEmbedding.length === 0) continue;

      const score = SemanticBridgeService.cosineSimilarity(contentEmbedding, conceptEmbedding);
      scored.push({ concept, relevanceScore: score });
    }

    // Sort descending and return top N
    scored.sort((a, b) => b.relevanceScore - a.relevanceScore);
    return scored.slice(0, topN * 2); // over-fetch — some may lack mappings
  }

  /**
   * Build the LLM prompt for semantic translation.
   *
   * The system prompt explains the task to the source model.
   * The user prompt contains the anchor glossary and the content to translate.
   *
   * We use the SOURCE model for generation because:
   *  - It already understands the source dialect perfectly
   *  - We give it the target expressions as a glossary
   *  - It bridges from familiar → target with minimal hallucination risk
   */
  private buildTranslationPrompt(
    content: string,
    sourceKey: ModelKey,
    targetKey: ModelKey,
    anchors: ConceptAnchor[],
  ): { systemPrompt: string; userPrompt: string } {
    const systemPrompt = [
      `You are a semantic translator bridging AI model communication.`,
      ``,
      `Source model: ${sourceKey}`,
      `Target model: ${targetKey}`,
      ``,
      `Your task: rewrite the content below so that ${targetKey} will understand it as clearly`,
      `as ${sourceKey} understands the original. Preserve ALL information and intent.`,
      `Do not add commentary — output only the translated content.`,
    ].join('\n');

    const glossaryLines = anchors.map(a =>
      `  "${a.sourceExpression}" → "${a.targetExpression}" (concept: ${a.conceptSlug})`
    );

    const glossarySection = anchors.length > 0
      ? [``, `Semantic concept glossary (how this concept is best expressed for ${targetKey}):`, ...glossaryLines, ``]
      : [];

    const userPrompt = [
      ...glossarySection,
      `Content to translate for ${targetKey}:`,
      ``,
      content,
    ].join('\n');

    return { systemPrompt, userPrompt };
  }
}
