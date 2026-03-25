/**
 * OntologyRegistry — In-memory L1 cache backed by DataDaemon (L2)
 *
 * This is the server-side singleton that wires together:
 *  - Data persistence (DataDaemon for concept + mapping entities)
 *  - In-memory cache (L1) for fast lookup during translation
 *  - OntologyEvolutionService for drift detection and re-alignment
 *  - SemanticBridgeService for the translation API
 *
 * Startup flow:
 *  1. Load all existing OntologyConceptEntity rows from DB into L1 cache
 *  2. Seed any ONTOLOGY_SEED_CONCEPTS that are missing
 *  3. Start OntologyEvolutionService (event listener + bootstrap)
 *  4. Expose SemanticBridgeService as the public translation API
 *
 * Access:
 *  import { OntologyRegistry } from '.../OntologyRegistry';
 *  const result = await OntologyRegistry.sharedInstance().bridge.translate(request);
 */

import { Events } from '@system/core/shared/Events';
import { DataList } from '../../../commands/data/list/shared/DataListTypes';
import { DataCreate } from '../../../commands/data/create/shared/DataCreateTypes';
import { DataUpdate } from '../../../commands/data/update/shared/DataUpdateTypes';
import { SemanticBridgeService } from '../shared/SemanticBridgeService';
import { OntologyEvolutionService } from './OntologyEvolutionService';
import type { OntologyRegistryInterface } from './OntologyEvolutionService';
import { OntologyConceptEntity } from '../shared/entities/OntologyConceptEntity';
import { OntologyMappingEntity } from '../shared/entities/OntologyMappingEntity';
import type {
  OntologyConceptNode,
  OntologyMapping,
  ModelKey,
  ConceptSeed,
} from '../shared/OntologyTypes';
import {
  ONTOLOGY_SEED_CONCEPTS,
  ONTOLOGY_CONSTANTS,
  toModelKey,
} from '../shared/OntologyTypes';
import type { UUID } from '../../core/types/CrossPlatformUUID';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ConceptCache {
  entity: OntologyConceptEntity;
  /** Reconstructed OntologyConceptNode view (denormalised for SemanticBridgeService) */
  node: OntologyConceptNode;
}

// ---------------------------------------------------------------------------
// OntologyRegistry
// ---------------------------------------------------------------------------

export class OntologyRegistry implements OntologyRegistryInterface {
  private static _instance: OntologyRegistry | null = null;

  /** L1 in-memory cache: slug → ConceptCache */
  private readonly _concepts = new Map<string, ConceptCache>();

  /** L1 mapping cache: `${sourceKey}::${targetKey}::${slug}` → OntologyMappingEntity */
  private readonly _mappings = new Map<string, OntologyMappingEntity>();

  /** Set of model keys that have at least one expression in the ontology */
  private readonly _registeredModels = new Set<ModelKey>();

  private _evolution: OntologyEvolutionService | null = null;
  private _bridge: SemanticBridgeService | null = null;
  private _initialized = false;

  // -------------------------------------------------------------------------
  // Singleton
  // -------------------------------------------------------------------------

  static sharedInstance(): OntologyRegistry {
    if (!OntologyRegistry._instance) {
      OntologyRegistry._instance = new OntologyRegistry();
    }
    return OntologyRegistry._instance;
  }

  // -------------------------------------------------------------------------
  // Public API
  // -------------------------------------------------------------------------

  get bridge(): SemanticBridgeService {
    if (!this._bridge) throw new Error('OntologyRegistry not initialised — call init() first');
    return this._bridge;
  }

  get evolution(): OntologyEvolutionService {
    if (!this._evolution) throw new Error('OntologyRegistry not initialised — call init() first');
    return this._evolution;
  }

  /**
   * Initialise the registry: load from DB, seed missing concepts, wire services.
   * Called once during server startup (e.g., from AIProviderDaemonServer.initialize).
   */
  async init(opts: {
    embedder: (text: string, modelKey: ModelKey) => Promise<number[]>;
    generator: (prompt: string, systemPrompt: string, modelKey: ModelKey) => Promise<string>;
    log?: (msg: string, level?: 'info' | 'warn' | 'error') => void;
  }): Promise<void> {
    if (this._initialized) return;
    this._initialized = true;

    const log = opts.log ?? ((msg: string, _level?: 'info' | 'warn' | 'error') => console.log(`[OntologyRegistry] ${msg}`));

    // Step 1: Load existing concepts and mappings from DB
    await this.loadFromDB(log);

    // Step 2: Seed missing concepts
    await this.seedConcepts(ONTOLOGY_SEED_CONCEPTS, log);

    // Step 3: Wire SemanticBridgeService
    this._bridge = new SemanticBridgeService(
      {
        getBySlug: (slug) => this.getConceptNode(slug),
        listAll: (domain) => this.listConceptNodes(domain),
        getMapping: (src, tgt, slug) => this.getMappingNode(src, tgt, slug),
        listMappings: (src, tgt) => this.listMappingNodes(src, tgt),
      },
      {
        embed: opts.embedder,
      },
      {
        generate: opts.generator,
      },
    );

    // Step 4: Wire OntologyEvolutionService and start listening
    this._evolution = new OntologyEvolutionService({
      registry: this,
      embedder: opts.embedder,
      generator: opts.generator,
      log,
    });
    this._evolution.start();

    // Step 5: Bootstrap missing embeddings (async — non-blocking)
    this._evolution.bootstrap().then(result => {
      log(`Bootstrap: ${result.conceptsProcessed} concepts, ${result.embeddingsGenerated} embeddings generated`);
    }).catch(err => {
      log(`Bootstrap error: ${err}`, 'warn');
    });

    log(`OntologyRegistry initialised — ${this._concepts.size} concepts, ${this._mappings.size} mappings loaded`);
  }

  // -------------------------------------------------------------------------
  // OntologyRegistryInterface implementation
  // -------------------------------------------------------------------------

  async getAllConcepts(): Promise<OntologyConceptEntity[]> {
    return Array.from(this._concepts.values()).map(c => c.entity);
  }

  async getMappingsBetween(
    sourceKey: ModelKey,
    targetKey: ModelKey,
  ): Promise<OntologyMappingEntity[]> {
    const prefix = `${sourceKey}::${targetKey}::`;
    return Array.from(this._mappings.entries())
      .filter(([k]) => k.startsWith(prefix))
      .map(([, v]) => v);
  }

  async updateConceptEmbedding(
    conceptId: string,
    modelKey: ModelKey,
    vector: number[],
  ): Promise<void> {
    const cache = Array.from(this._concepts.values()).find(c => c.entity.id === conceptId);
    if (!cache) return;

    cache.entity.recordEmbedding(modelKey, vector);
    cache.node.embeddings[modelKey] = vector;
    cache.node.embeddingTimestamps[modelKey] = new Date().toISOString();

    // Persist to DB
    await DataUpdate.execute({
      collection: OntologyConceptEntity.collection,
      id: conceptId as UUID,
      data: {
        embeddings: cache.entity.embeddings,
        embeddingTimestamps: cache.entity.embeddingTimestamps,
        hasStaleEmbeddings: cache.entity.hasStaleEmbeddings,
        updatedAt: cache.entity.updatedAt,
      },
    });
  }

  async upsertMapping(
    conceptId: string,
    conceptSlug: string,
    sourceKey: ModelKey,
    targetKey: ModelKey,
    sourceExpression: string,
    targetExpression: string,
    similarity: number,
  ): Promise<void> {
    const cacheKey = `${sourceKey}::${targetKey}::${conceptSlug}`;
    let entity = this._mappings.get(cacheKey);

    if (entity) {
      entity.sourceExpression = sourceExpression;
      entity.targetExpression = targetExpression;
      entity.updateSimilarity(similarity, ONTOLOGY_CONSTANTS.DRIFT_THRESHOLD);
      await DataUpdate.execute({
        collection: OntologyMappingEntity.collection,
        id: entity.id,
        data: {
          sourceExpression,
          targetExpression,
          similarity: entity.similarity,
          needsReview: entity.needsReview,
          verifiedAt: entity.verifiedAt,
          updatedAt: entity.updatedAt,
        },
      });
    } else {
      entity = new OntologyMappingEntity();
      entity.conceptSlug = conceptSlug;
      entity.conceptId = conceptId as UUID;
      entity.sourceModel = { providerId: sourceKey.split('/')[0], modelId: sourceKey.split('/').slice(1).join('/') };
      entity.sourceModelKey = sourceKey;
      entity.targetModel = { providerId: targetKey.split('/')[0], modelId: targetKey.split('/').slice(1).join('/') };
      entity.targetModelKey = targetKey;
      entity.sourceExpression = sourceExpression;
      entity.targetExpression = targetExpression;
      entity.similarity = similarity;
      entity.verifiedAt = new Date().toISOString();

      const result = await DataCreate.execute<OntologyMappingEntity>({
        collection: OntologyMappingEntity.collection,
        data: entity,
      });

      if (result.success && result.data) {
        entity.id = result.data.id;
      }

      this._mappings.set(cacheKey, entity);
    }

    Events.emit(ONTOLOGY_CONSTANTS.EVENTS.MAPPING_UPDATED, {
      conceptSlug,
      sourceModelKey: sourceKey,
      targetModelKey: targetKey,
      similarity,
    });
  }

  getRegisteredModelKeys(): ModelKey[] {
    return Array.from(this._registeredModels);
  }

  // -------------------------------------------------------------------------
  // Concept registration (called by ontology/concept/register command)
  // -------------------------------------------------------------------------

  async registerConcept(
    slug: string,
    displayName: string,
    domain: OntologyConceptEntity['domain'],
    description: string,
    canonicalExpression: string,
    relatedConcepts: string[],
    initialExpressions: Partial<Record<ModelKey, string>>,
  ): Promise<OntologyConceptEntity> {
    const existing = this._concepts.get(slug);
    if (existing) return existing.entity;

    const entity = new OntologyConceptEntity();
    entity.slug = slug;
    entity.displayName = displayName;
    entity.domain = domain;
    entity.description = description;
    entity.canonicalExpression = canonicalExpression;
    entity.relatedConcepts = relatedConcepts;
    entity.expressions = initialExpressions as Record<ModelKey, string>;
    entity.modelCount = Object.keys(initialExpressions).length;

    // Track model keys
    for (const key of Object.keys(initialExpressions) as ModelKey[]) {
      this._registeredModels.add(key);
    }

    // Persist
    const result = await DataCreate.execute<OntologyConceptEntity>({
      collection: OntologyConceptEntity.collection,
      data: entity,
    });

    if (result.success && result.data) {
      entity.id = result.data.id;
    }

    const node = this.entityToNode(entity);
    this._concepts.set(slug, { entity, node });

    Events.emit(ONTOLOGY_CONSTANTS.EVENTS.CONCEPT_REGISTERED, { conceptSlug: slug });

    return entity;
  }

  // -------------------------------------------------------------------------
  // ConceptStore interface helpers (for SemanticBridgeService)
  // -------------------------------------------------------------------------

  private async getConceptNode(slug: string): Promise<OntologyConceptNode | null> {
    return this._concepts.get(slug)?.node ?? null;
  }

  private async listConceptNodes(domain?: string): Promise<OntologyConceptNode[]> {
    const nodes = Array.from(this._concepts.values()).map(c => c.node);
    if (!domain) return nodes;
    return nodes.filter(n => n.domain === domain);
  }

  private async getMappingNode(
    sourceKey: ModelKey,
    targetKey: ModelKey,
    slug: string,
  ): Promise<OntologyMapping | null> {
    const cacheKey = `${sourceKey}::${targetKey}::${slug}`;
    const entity = this._mappings.get(cacheKey);
    if (!entity) return null;
    return this.entityToMappingNode(entity);
  }

  private async listMappingNodes(
    sourceKey: ModelKey,
    targetKey: ModelKey,
  ): Promise<OntologyMapping[]> {
    const prefix = `${sourceKey}::${targetKey}::`;
    return Array.from(this._mappings.entries())
      .filter(([k]) => k.startsWith(prefix))
      .map(([, v]) => this.entityToMappingNode(v));
  }

  // -------------------------------------------------------------------------
  // DB load and seeding
  // -------------------------------------------------------------------------

  private async loadFromDB(log: (msg: string, level?: 'info' | 'warn' | 'error') => void): Promise<void> {
    try {
      const conceptResult = await DataList.execute<OntologyConceptEntity>({
        collection: OntologyConceptEntity.collection,
        limit: 1000,
      });

      for (const raw of (conceptResult?.items ?? [])) {
        const entity = Object.assign(new OntologyConceptEntity(), raw);
        const node = this.entityToNode(entity);
        this._concepts.set(entity.slug, { entity, node });

        for (const key of Object.keys(entity.expressions ?? {}) as ModelKey[]) {
          this._registeredModels.add(key);
        }
      }

      const mappingResult = await DataList.execute<OntologyMappingEntity>({
        collection: OntologyMappingEntity.collection,
        limit: 5000,
      });

      for (const raw of (mappingResult?.items ?? [])) {
        const entity = Object.assign(new OntologyMappingEntity(), raw);
        const cacheKey = `${entity.sourceModelKey}::${entity.targetModelKey}::${entity.conceptSlug}`;
        this._mappings.set(cacheKey, entity);
      }

      log(`Loaded ${this._concepts.size} concepts and ${this._mappings.size} mappings from DB`);
    } catch (err) {
      log(`DB load failed (new installation?): ${err}`, 'warn');
    }
  }

  private async seedConcepts(seeds: ConceptSeed[], log: (msg: string, level?: 'info' | 'warn' | 'error') => void): Promise<void> {
    let seeded = 0;
    for (const seed of seeds) {
      if (this._concepts.has(seed.slug)) continue;

      await this.registerConcept(
        seed.slug,
        seed.displayName,
        seed.domain,
        seed.description,
        seed.canonicalExpression,
        seed.relatedConcepts,
        {}, // expressions populated by bootstrap
      );
      seeded++;
    }
    if (seeded > 0) log(`Seeded ${seeded} new concepts`);
  }

  // -------------------------------------------------------------------------
  // Entity ↔ Node conversions
  // -------------------------------------------------------------------------

  private entityToNode(entity: OntologyConceptEntity): OntologyConceptNode {
    return {
      slug: entity.slug,
      displayName: entity.displayName,
      domain: entity.domain,
      description: entity.description,
      relatedConcepts: entity.relatedConcepts ?? [],
      expressions: { ...(entity.expressions ?? {}) },
      embeddings: { ...(entity.embeddings ?? {}) },
      embeddingTimestamps: { ...(entity.embeddingTimestamps ?? {}) },
    };
  }

  private entityToMappingNode(entity: OntologyMappingEntity): OntologyMapping {
    return {
      conceptSlug: entity.conceptSlug,
      sourceModel: entity.sourceModel,
      targetModel: entity.targetModel,
      sourceExpression: entity.sourceExpression,
      targetExpression: entity.targetExpression,
      similarity: entity.similarity,
      confidence: entity.confidence,
      verifiedAt: entity.verifiedAt ?? new Date().toISOString(),
      needsReview: entity.needsReview,
      sourceVersion: entity.sourceVersion,
      targetVersion: entity.targetVersion,
    };
  }
}
