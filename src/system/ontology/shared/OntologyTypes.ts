/**
 * OntologyTypes — Shared type system for the Semantic Bridge / Living Ontology
 *
 * Core model: Every AI model has its own "semantic dialect" — a particular way it
 * expresses concepts, shaped by pre-training data, RLHF, and any LoRA adapters that
 * have been applied. When models collaborate (chat, academy, sentinels), they may talk
 * past each other because their semantic spaces are aligned differently.
 *
 * Solution: A shared ontology of canonical concepts, each annotated with:
 *   - Per-model expressions (how each model naturally says the thing)
 *   - Per-model embedding vectors (where the concept lives in each model's latent space)
 *   - Cross-model similarity scores (how aligned the expressions currently are)
 *
 * The ontology EVOLVES: when a model is fine-tuned (LoRA applied), its concept
 * representations shift. OntologyEvolutionService detects this drift and triggers
 * re-alignment, keeping translations accurate.
 */

// ---------------------------------------------------------------------------
// Model identity
// ---------------------------------------------------------------------------

/**
 * Unambiguous identifier for a specific model deployment.
 * Format: "<providerId>/<modelId>"  e.g. "candle/qwen2.5-14b" or "anthropic/claude-sonnet-4-6"
 */
export type ModelKey = `${string}/${string}`;

export interface ModelIdentifier {
  /** Provider ID as registered in AIProviderDaemon */
  providerId: string;
  /** Model ID or tier name */
  modelId: string;
  /** Optional LoRA adapters active during this model's alignment snapshot */
  activeAdapters?: string[];
}

export function toModelKey(m: ModelIdentifier): ModelKey {
  return `${m.providerId}/${m.modelId}` as ModelKey;
}

// ---------------------------------------------------------------------------
// Concept node
// ---------------------------------------------------------------------------

/**
 * A single canonical concept in the shared ontology.
 *
 * The concept is model-agnostic (e.g. "low-rank-matrix-decomposition"),
 * but carries per-model expressions and embedding vectors for translation.
 */
export interface OntologyConceptNode {
  /** Canonical slug — URL-safe, unique, never changes */
  slug: string;

  /** Human-readable name */
  displayName: string;

  /** Ontology domain for organisation and retrieval */
  domain: OntologyDomain;

  /** Free-text description of what this concept means */
  description: string;

  /** Related concept slugs */
  relatedConcepts: string[];

  /**
   * Per-model natural-language expressions.
   * Key: ModelKey, Value: how that model naturally expresses this concept.
   * e.g. "candle/qwen2.5-14b" → "rank-r weight matrices A and B"
   *      "anthropic/claude-sonnet-4-6" → "low-rank weight decomposition W = BA"
   */
  expressions: Record<ModelKey, string>;

  /**
   * Per-model embedding vectors for this concept's expression.
   * Key: ModelKey, Value: embedding array.
   * Used for drift detection and nearest-neighbour lookup.
   */
  embeddings: Record<ModelKey, number[]>;

  /**
   * Timestamp of last embedding computation per model.
   * Key: ModelKey, Value: ISO timestamp.
   */
  embeddingTimestamps: Record<ModelKey, string>;
}

// ---------------------------------------------------------------------------
// Cross-model mapping
// ---------------------------------------------------------------------------

/**
 * A directional semantic mapping: source model expression → target model expression.
 *
 * Bidirectional mappings are stored as two separate records so each direction
 * can have its own confidence score (asymmetry is common — a small model may
 * understand a concept expressed by a large model better than the reverse).
 */
export interface OntologyMapping {
  /** Canonical concept this mapping is anchored to */
  conceptSlug: string;

  /** Source model */
  sourceModel: ModelIdentifier;

  /** Target model */
  targetModel: ModelIdentifier;

  /** Expression in source model's semantic dialect */
  sourceExpression: string;

  /** Expression in target model's semantic dialect */
  targetExpression: string;

  /**
   * Cosine similarity between the two models' embedding vectors for this concept.
   * Range [0, 1]. 1.0 = identical representation. < 0.7 = significant drift.
   */
  similarity: number;

  /**
   * Confidence score for the translation quality.
   * Range [0, 1]. Starts at 0.5 for new mappings; rises with validation.
   */
  confidence: number;

  /** When this mapping was last verified/recomputed */
  verifiedAt: string;

  /** Set when similarity has drifted beyond DRIFT_THRESHOLD and re-alignment is needed */
  needsReview: boolean;

  /** Version tags for the model at alignment time (useful after LoRA updates) */
  sourceVersion?: string;
  targetVersion?: string;
}

// ---------------------------------------------------------------------------
// Ontology domains
// ---------------------------------------------------------------------------

export const ONTOLOGY_DOMAINS = [
  'machine-learning',
  'software-engineering',
  'reasoning',
  'mathematics',
  'language',
  'instruction-following',
  'tool-use',
  'safety',
  'creativity',
  'domain-general',
] as const;

export type OntologyDomain = typeof ONTOLOGY_DOMAINS[number];

// ---------------------------------------------------------------------------
// Translation request / response
// ---------------------------------------------------------------------------

/**
 * Request to translate content from one model's semantic space to another's.
 */
export interface SemanticTranslationRequest {
  /** The content to translate */
  content: string;

  /** Model whose semantic dialect the content is written in */
  sourceModel: ModelIdentifier;

  /** Model whose semantic dialect we want to translate into */
  targetModel: ModelIdentifier;

  /** If provided, only anchor on concepts within these domains */
  domains?: OntologyDomain[];

  /** Max concept anchors to include (default 8) */
  maxAnchors?: number;

  /**
   * Minimum cosine similarity required to consider a concept mapping usable.
   * Below this threshold, the sentinel will use prompted re-alignment before translating.
   * Default: 0.65
   */
  minSimilarity?: number;

  /** Unique request ID for event correlation */
  requestId?: string;
}

/**
 * Result of a semantic translation.
 */
export interface SemanticTranslationResult {
  /** The translated content */
  translatedContent: string;

  /** The original content, unmodified */
  originalContent: string;

  /** Concept anchors that were used during translation */
  anchorsUsed: ConceptAnchor[];

  /** Number of concepts that needed re-alignment before translation */
  realignmentsNeeded: number;

  /** Overall translation confidence (mean confidence of used anchors) */
  translationConfidence: number;

  /** Whether any concepts had low similarity and were flagged */
  hasDriftWarnings: boolean;
}

/**
 * A resolved concept anchor used during translation.
 */
export interface ConceptAnchor {
  conceptSlug: string;
  sourceExpression: string;
  targetExpression: string;
  similarity: number;
  confidence: number;
}

// ---------------------------------------------------------------------------
// Evolution events
// ---------------------------------------------------------------------------

/**
 * Emitted when a model's concept representations drift beyond threshold.
 */
export interface OntologyDriftEvent {
  modelKey: ModelKey;
  conceptSlug: string;
  previousSimilarity: number;
  currentSimilarity: number;
  drift: number;
}

/**
 * Emitted when a re-alignment run completes.
 */
export interface OntologyRealignmentEvent {
  modelKey: ModelKey;
  conceptsRealigned: number;
  mappingsUpdated: number;
  durationMs: number;
  triggeredBy: 'model-update' | 'manual' | 'scheduled';
}

// ---------------------------------------------------------------------------
// Seed concepts (bootstraps the ontology at startup)
// ---------------------------------------------------------------------------

/**
 * Minimal concept seeds used to bootstrap the ontology.
 * Expressions are populated per-model during first alignment.
 */
export interface ConceptSeed {
  slug: string;
  displayName: string;
  domain: OntologyDomain;
  description: string;
  /** Canonical expression used to bootstrap embedding generation */
  canonicalExpression: string;
  relatedConcepts: string[];
}

export const ONTOLOGY_SEED_CONCEPTS: ConceptSeed[] = [
  {
    slug: 'low-rank-matrix-decomposition',
    displayName: 'Low-Rank Matrix Decomposition',
    domain: 'machine-learning',
    description: 'Approximating a weight matrix W as the product of two smaller matrices A and B, where rank(AB) << rank(W)',
    canonicalExpression: 'decompose weight matrix W = BA where B is d×r and A is r×k, r << min(d,k)',
    relatedConcepts: ['lora-adapter', 'gradient-descent', 'parameter-efficient-fine-tuning'],
  },
  {
    slug: 'lora-adapter',
    displayName: 'LoRA Adapter',
    domain: 'machine-learning',
    description: 'Low-Rank Adaptation: lightweight trainable weights that augment a frozen base model',
    canonicalExpression: 'trainable low-rank matrices injected into transformer attention layers',
    relatedConcepts: ['low-rank-matrix-decomposition', 'parameter-efficient-fine-tuning', 'fine-tuning'],
  },
  {
    slug: 'chain-of-thought',
    displayName: 'Chain of Thought Reasoning',
    domain: 'reasoning',
    description: 'Decomposing a problem into sequential intermediate reasoning steps before answering',
    canonicalExpression: 'solve step by step, showing intermediate reasoning before the final answer',
    relatedConcepts: ['reasoning', 'instruction-following'],
  },
  {
    slug: 'context-window',
    displayName: 'Context Window',
    domain: 'machine-learning',
    description: 'The maximum sequence length a model can attend to in a single forward pass',
    canonicalExpression: 'maximum number of tokens the model can process in one pass',
    relatedConcepts: ['attention-mechanism', 'token'],
  },
  {
    slug: 'attention-mechanism',
    displayName: 'Attention Mechanism',
    domain: 'machine-learning',
    description: 'The scaled dot-product attention computation that lets transformer layers weight token relationships',
    canonicalExpression: 'scaled dot-product attention: softmax(QK^T / sqrt(d_k)) * V',
    relatedConcepts: ['context-window', 'transformer'],
  },
  {
    slug: 'system-prompt',
    displayName: 'System Prompt',
    domain: 'instruction-following',
    description: 'Privileged instructions given to the model before the user conversation begins',
    canonicalExpression: 'instructions in the system role that constrain or guide model behaviour',
    relatedConcepts: ['instruction-following', 'role-playing'],
  },
  {
    slug: 'tool-use',
    displayName: 'Tool Use / Function Calling',
    domain: 'tool-use',
    description: 'Model ability to invoke external functions or APIs during generation',
    canonicalExpression: 'call external functions by emitting structured JSON tool-call requests',
    relatedConcepts: ['instruction-following', 'agentic-loop'],
  },
  {
    slug: 'agentic-loop',
    displayName: 'Agentic Loop',
    domain: 'tool-use',
    description: 'The observe-plan-act cycle where a model iterates until a goal is achieved',
    canonicalExpression: 'repeated observe-plan-act cycle: perceive state, choose action, execute, repeat',
    relatedConcepts: ['tool-use', 'reasoning'],
  },
  {
    slug: 'parameter-efficient-fine-tuning',
    displayName: 'Parameter-Efficient Fine-Tuning (PEFT)',
    domain: 'machine-learning',
    description: 'Fine-tuning strategies that update only a small fraction of model parameters',
    canonicalExpression: 'adapt a frozen base model by training only a small set of new or injected parameters',
    relatedConcepts: ['lora-adapter', 'fine-tuning'],
  },
  {
    slug: 'semantic-similarity',
    displayName: 'Semantic Similarity',
    domain: 'language',
    description: 'A measure of how closely two pieces of text convey the same meaning',
    canonicalExpression: 'cosine similarity between embedding vectors of two text passages',
    relatedConcepts: ['embedding', 'vector-search'],
  },
];

// ---------------------------------------------------------------------------
// Thresholds and constants
// ---------------------------------------------------------------------------

export const ONTOLOGY_CONSTANTS = {
  /** Cosine similarity below this triggers drift warning */
  DRIFT_THRESHOLD: 0.15,

  /** Minimum similarity for a mapping to be considered usable without re-alignment */
  MIN_USABLE_SIMILARITY: 0.65,

  /** Default number of concept anchors to use in a translation */
  DEFAULT_MAX_ANCHORS: 8,

  /** Embedding vector dimension expected from embedding providers */
  EXPECTED_EMBEDDING_DIM: 768,

  /** Re-alignment is forced if last verification is older than this (ms) */
  MAX_STALENESS_MS: 7 * 24 * 60 * 60 * 1000, // 7 days

  /** Event names */
  EVENTS: {
    CONCEPT_REGISTERED: 'ontology:concept:registered',
    MAPPING_UPDATED: 'ontology:mapping:updated',
    DRIFT_DETECTED: 'ontology:drift:detected',
    REALIGNMENT_COMPLETE: 'ontology:realignment:complete',
    TRANSLATE_REQUEST: 'ontology:translate:request',
    TRANSLATE_COMPLETE: 'ontology:translate:complete',
  },
} as const;
