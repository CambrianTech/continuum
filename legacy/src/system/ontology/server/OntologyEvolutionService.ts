/**
 * OntologyEvolutionService — Real-time ontology alignment and drift detection
 *
 * This is the "living" part of the living ontology. It:
 *
 *  1. LISTENS for model-update events (LoRA adapter applied, model fine-tuned)
 *  2. DETECTS embedding drift: recomputes concept embeddings for the updated model
 *     and compares them against stored vectors using cosine distance
 *  3. RE-ALIGNS: for concepts whose drift exceeds threshold, asks each model to
 *     re-express the concept in its own words, then recomputes similarity
 *  4. BOOTSTRAPS: on startup, initialises embeddings for any concept/model pair
 *     that lacks them
 *
 * This runs server-side only — it needs AIProviderDaemon and DataDaemon.
 *
 * Architecture note: OntologyEvolutionService is deliberately NOT a daemon —
 * it is a service instantiated and owned by OntologyRegistry, which itself
 * is bootstrapped during server startup. This keeps lifecycle explicit.
 */

import { Events } from '@system/core/shared/Events';
import { Commands } from '@system/core/shared/Commands';
import { SemanticBridgeService } from '../shared/SemanticBridgeService';
import type { OntologyConceptNode, ModelKey, OntologyDomain } from '../shared/OntologyTypes';
import { ONTOLOGY_CONSTANTS, toModelKey } from '../shared/OntologyTypes';
import type { OntologyConceptEntity } from '../shared/entities/OntologyConceptEntity';
import type { OntologyMappingEntity } from '../shared/entities/OntologyMappingEntity';

// ---------------------------------------------------------------------------
// Dependencies injected at construction — avoids circular imports
// ---------------------------------------------------------------------------

export interface OntologyEvolutionDeps {
  /** Full registry for reading/updating concept and mapping rows */
  registry: OntologyRegistryInterface;
  /** AIProviderDaemon-backed embedding provider */
  embedder: (text: string, modelKey: ModelKey) => Promise<number[]>;
  /** AIProviderDaemon-backed text generator */
  generator: (prompt: string, systemPrompt: string, modelKey: ModelKey) => Promise<string>;
  /** Logger */
  log: (msg: string, level?: 'info' | 'warn' | 'error') => void;
}

/** Minimal interface for what OntologyEvolutionService needs from OntologyRegistry */
export interface OntologyRegistryInterface {
  getAllConcepts(): Promise<OntologyConceptEntity[]>;
  getMappingsBetween(sourceKey: ModelKey, targetKey: ModelKey): Promise<OntologyMappingEntity[]>;
  updateConceptEmbedding(conceptId: string, modelKey: ModelKey, vector: number[]): Promise<void>;
  upsertMapping(
    conceptId: string,
    conceptSlug: string,
    sourceKey: ModelKey,
    targetKey: ModelKey,
    sourceExpression: string,
    targetExpression: string,
    similarity: number,
  ): Promise<void>;
  getRegisteredModelKeys(): ModelKey[];
}

// ---------------------------------------------------------------------------
// OntologyEvolutionService
// ---------------------------------------------------------------------------

export class OntologyEvolutionService {
  private isRunning = false;
  private scheduledRealignments = new Map<ModelKey, ReturnType<typeof setTimeout>>();

  constructor(private readonly deps: OntologyEvolutionDeps) {}

  // -------------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------------

  /**
   * Start listening for model-update events and schedule initial bootstrap.
   */
  start(): void {
    // Model fine-tuned or LoRA adapter applied
    Events.subscribe('ai:model:updated', (event: { providerId: string; modelId: string; version?: string }) => {
      const modelKey = `${event.providerId}/${event.modelId}` as ModelKey;
      this.scheduleRealignment(modelKey, 'model-update');
    });

    // LoRA adapter activated on a persona (adapter changes the model's semantic space)
    Events.subscribe('genome:adapter:activated', (event: { providerId: string; modelId: string; adapterId: string }) => {
      const modelKey = `${event.providerId}/${event.modelId}` as ModelKey;
      this.scheduleRealignment(modelKey, 'model-update');
    });

    // Manual re-alignment trigger (e.g., from ./jtag ontology/realign)
    Events.subscribe('ontology:realign:request', (event: { modelKey?: ModelKey }) => {
      if (event.modelKey) {
        this.scheduleRealignment(event.modelKey, 'manual');
      } else {
        // Re-align all registered models
        for (const key of this.deps.registry.getRegisteredModelKeys()) {
          this.scheduleRealignment(key, 'manual');
        }
      }
    });

    this.deps.log('OntologyEvolutionService started — listening for model update events');
  }

  // -------------------------------------------------------------------------
  // Bootstrap — run once at startup for any missing embeddings
  // -------------------------------------------------------------------------

  /**
   * Ensure every concept has embeddings for every registered model.
   * Called by OntologyRegistry after startup seeding is complete.
   */
  async bootstrap(): Promise<{ conceptsProcessed: number; embeddingsGenerated: number }> {
    const concepts = await this.deps.registry.getAllConcepts();
    const modelKeys = this.deps.registry.getRegisteredModelKeys();

    let embeddingsGenerated = 0;

    for (const concept of concepts) {
      for (const modelKey of modelKeys) {
        if (!concept.hasEmbedding(modelKey)) {
          await this.generateAndStoreEmbedding(concept, modelKey);
          embeddingsGenerated++;
        }
      }
    }

    // Compute missing cross-model mappings
    await this.computeMissingMappings(concepts, modelKeys);

    this.deps.log(`Bootstrap complete: ${concepts.length} concepts, ${embeddingsGenerated} embeddings generated`);
    return { conceptsProcessed: concepts.length, embeddingsGenerated };
  }

  // -------------------------------------------------------------------------
  // Re-alignment — triggered by model updates
  // -------------------------------------------------------------------------

  /**
   * Schedule re-alignment for a model with 2-second debounce.
   * Rapid adapter swaps don't trigger a flood of re-alignment runs.
   */
  private scheduleRealignment(modelKey: ModelKey, trigger: 'model-update' | 'manual' | 'scheduled'): void {
    // Cancel any pending re-alignment for this model
    const existing = this.scheduledRealignments.get(modelKey);
    if (existing) clearTimeout(existing);

    const handle = setTimeout(() => {
      this.scheduledRealignments.delete(modelKey);
      this.runRealignment(modelKey, trigger).catch(err => {
        this.deps.log(`Re-alignment failed for ${modelKey}: ${err}`, 'error');
      });
    }, 2000);

    this.scheduledRealignments.set(modelKey, handle);
    this.deps.log(`Re-alignment scheduled for ${modelKey} (trigger: ${trigger})`);
  }

  /**
   * Full re-alignment run for a single model:
   *  1. Recompute embeddings for all concepts in this model's space
   *  2. For each mapping involving this model, recompute cosine similarity
   *  3. Flag mappings that have drifted beyond DRIFT_THRESHOLD
   *  4. Re-elicit expressions for drifted concepts
   */
  async runRealignment(
    modelKey: ModelKey,
    trigger: 'model-update' | 'manual' | 'scheduled',
  ): Promise<void> {
    if (this.isRunning) {
      this.deps.log(`Re-alignment already in progress, queuing ${modelKey}`, 'warn');
      return;
    }
    this.isRunning = true;
    const startMs = Date.now();

    let conceptsRealigned = 0;
    let mappingsUpdated = 0;

    try {
      const concepts = await this.deps.registry.getAllConcepts();

      // Phase 1: regenerate embeddings for updated model
      for (const concept of concepts) {
        const oldVector = concept.embeddings?.[modelKey];
        const newVector = await this.generateAndStoreEmbedding(concept, modelKey);

        if (oldVector && oldVector.length > 0) {
          const drift = SemanticBridgeService.embeddingDrift(oldVector, newVector);
          if (drift > ONTOLOGY_CONSTANTS.DRIFT_THRESHOLD) {
            Events.emit(ONTOLOGY_CONSTANTS.EVENTS.DRIFT_DETECTED, {
              modelKey,
              conceptSlug: concept.slug,
              previousSimilarity: 1 - drift,
              currentSimilarity: 1,
              drift,
            });
            conceptsRealigned++;
          }
        }
      }

      // Phase 2: recompute cross-model similarities involving this model
      const otherModelKeys = this.deps.registry
        .getRegisteredModelKeys()
        .filter(k => k !== modelKey);

      for (const otherKey of otherModelKeys) {
        const mappings = await this.deps.registry.getMappingsBetween(modelKey, otherKey);

        for (const mapping of mappings) {
          const updatedSimilarity = await this.recomputeMappingSimilarity(mapping, concepts);
          if (updatedSimilarity !== null) {
            const drifted = Math.abs(updatedSimilarity - mapping.similarity) > ONTOLOGY_CONSTANTS.DRIFT_THRESHOLD;
            if (drifted) {
              // Re-elicit expression from the updated model
              await this.realignMapping(mapping, modelKey, concepts);
              mappingsUpdated++;
            }
          }
        }
      }

      const durationMs = Date.now() - startMs;
      Events.emit(ONTOLOGY_CONSTANTS.EVENTS.REALIGNMENT_COMPLETE, {
        modelKey,
        conceptsRealigned,
        mappingsUpdated,
        durationMs,
        triggeredBy: trigger,
      });

      this.deps.log(
        `Re-alignment complete for ${modelKey}: ${conceptsRealigned} concepts, ` +
        `${mappingsUpdated} mappings updated in ${durationMs}ms`
      );
    } finally {
      this.isRunning = false;
    }
  }

  // -------------------------------------------------------------------------
  // Private helpers
  // -------------------------------------------------------------------------

  /**
   * Generate an embedding for a concept's expression under a specific model
   * and persist it via the registry.
   */
  private async generateAndStoreEmbedding(
    concept: OntologyConceptEntity,
    modelKey: ModelKey,
  ): Promise<number[]> {
    // Use the model-specific expression if available, fall back to canonical
    const text = concept.expressions?.[modelKey] ?? concept.canonicalExpression;

    const vector = await this.deps.embedder(text, modelKey);
    await this.deps.registry.updateConceptEmbedding(concept.id, modelKey, vector);
    return vector;
  }

  /**
   * Recompute the cosine similarity for a mapping using the latest stored embeddings.
   * Returns null if either model lacks an embedding for the concept.
   */
  private async recomputeMappingSimilarity(
    mapping: OntologyMappingEntity,
    concepts: OntologyConceptEntity[],
  ): Promise<number | null> {
    const concept = concepts.find(c => c.slug === mapping.conceptSlug);
    if (!concept) return null;

    const sourceVec = concept.embeddings?.[mapping.sourceModelKey];
    const targetVec = concept.embeddings?.[mapping.targetModelKey];
    if (!sourceVec || !targetVec) return null;

    return SemanticBridgeService.cosineSimilarity(sourceVec, targetVec);
  }

  /**
   * Re-elicit concept expression from the updated model using LLM prompting,
   * then update the mapping.
   *
   * "How do YOU express this concept?" → new expression → new similarity.
   */
  private async realignMapping(
    mapping: OntologyMappingEntity,
    updatedModelKey: ModelKey,
    concepts: OntologyConceptEntity[],
  ): Promise<void> {
    const concept = concepts.find(c => c.slug === mapping.conceptSlug);
    if (!concept) return;

    const isSourceUpdated = mapping.sourceModelKey === updatedModelKey;
    const modelToRealign = isSourceUpdated ? mapping.sourceModelKey : mapping.targetModelKey;
    const otherModelKey = isSourceUpdated ? mapping.targetModelKey : mapping.sourceModelKey;

    // Ask the updated model to re-express the concept
    const newExpression = await this.elicitExpression(
      concept.slug,
      concept.displayName,
      concept.description,
      concept.canonicalExpression,
      modelToRealign,
    );

    // Embed the new expression in both models and compute similarity
    const newVector = await this.deps.embedder(newExpression, modelToRealign);
    const otherExpression = concept.expressions?.[otherModelKey] ?? concept.canonicalExpression;
    const otherVector = await this.deps.embedder(otherExpression, otherModelKey);
    const similarity = SemanticBridgeService.cosineSimilarity(newVector, otherVector);

    // Persist updated mapping (direction depends on which model was updated)
    await this.deps.registry.upsertMapping(
      concept.id,
      concept.slug,
      isSourceUpdated ? modelToRealign : otherModelKey,
      isSourceUpdated ? otherModelKey : modelToRealign,
      isSourceUpdated ? newExpression : otherExpression,
      isSourceUpdated ? otherExpression : newExpression,
      similarity,
    );

    Events.emit(ONTOLOGY_CONSTANTS.EVENTS.MAPPING_UPDATED, {
      conceptSlug: concept.slug,
      sourceModelKey: mapping.sourceModelKey,
      targetModelKey: mapping.targetModelKey,
      newSimilarity: similarity,
    });
  }

  /**
   * Ask a model to express a concept in its own words.
   * Uses a structured prompt to ensure consistent, concise output.
   */
  private async elicitExpression(
    slug: string,
    displayName: string,
    description: string,
    canonicalExpression: string,
    modelKey: ModelKey,
  ): Promise<string> {
    const systemPrompt = [
      `You are helping to build a shared semantic ontology for AI model communication.`,
      `Respond with ONLY a single concise sentence (max 25 words) that captures the essence of the concept.`,
      `Use your natural vocabulary — do not paraphrase the description verbatim.`,
    ].join('\n');

    const userPrompt = [
      `Concept: ${displayName} (${slug})`,
      `Description: ${description}`,
      `Reference: "${canonicalExpression}"`,
      ``,
      `How do you naturally express this concept in one sentence?`,
    ].join('\n');

    const response = await this.deps.generator(userPrompt, systemPrompt, modelKey);
    // Trim to first sentence, remove quotes, strip excess whitespace
    return response.split(/[.\n]/)[0].replace(/^["']|["']$/g, '').trim();
  }

  /**
   * Compute mappings between all pairs of registered models for a concept.
   */
  private async computeMissingMappings(
    concepts: OntologyConceptEntity[],
    modelKeys: ModelKey[],
  ): Promise<void> {
    for (const concept of concepts) {
      for (let i = 0; i < modelKeys.length; i++) {
        for (let j = i + 1; j < modelKeys.length; j++) {
          const srcKey = modelKeys[i];
          const tgtKey = modelKeys[j];

          // Skip if both expressions exist but mapping is missing — create it
          const srcExp = concept.expressions?.[srcKey];
          const tgtExp = concept.expressions?.[tgtKey];
          if (!srcExp || !tgtExp) continue;

          const srcVec = concept.embeddings?.[srcKey];
          const tgtVec = concept.embeddings?.[tgtKey];
          if (!srcVec || !tgtVec) continue;

          const similarity = SemanticBridgeService.cosineSimilarity(srcVec, tgtVec);
          await this.deps.registry.upsertMapping(
            concept.id, concept.slug,
            srcKey, tgtKey,
            srcExp, tgtExp,
            similarity,
          );
          // Also create the reverse direction
          await this.deps.registry.upsertMapping(
            concept.id, concept.slug,
            tgtKey, srcKey,
            tgtExp, srcExp,
            similarity,
          );
        }
      }
    }
  }
}

