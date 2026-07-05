/**
 * Persona Abstraction Types
 * 
 * Abstract interfaces for sophisticated persona composition from base models
 * and LoRA fine-tuning layers. Designed to work with multiple model providers
 * (OpenAI/DeepSeek/Anthropic) while maintaining consistent Grid substrate API.
 * 
 * A persona is a sophisticated composition of:
 * - Base foundation model (provider-specific) 
 * - LoRA genomic layers (29MB specialists)
 * - Memory modules and specialization components
 * - Communication adapters and performance profiles
 */

/**
 * Model provider abstraction
 * Each provider has different fine-tuning approaches and APIs
 */
export enum ModelProvider {
  OPENAI = 'openai',           // GPT models with fine-tuning API
  ANTHROPIC = 'anthropic',     // Claude models (no public fine-tuning yet)
  DEEPSEEK = 'deepseek',       // DeepSeek models with custom fine-tuning
  CUSTOM = 'custom',           // Custom model servers
  LOCAL = 'local'              // Local model hosting (Candle)
}

/**
 * Abstract base model interface
 * All model providers must implement this interface
 */
export interface BaseModelAdapter {
  readonly provider: ModelProvider;
  readonly modelId: string;
  readonly modelFamily: string;
  readonly contextWindow: number;
  readonly parameterCount: number;
  
  // Core inference capabilities
  generate(prompt: string, options?: GenerationOptions): Promise<GenerationResult>;
  embed(text: string): Promise<EmbeddingVector>;
  
  // Fine-tuning capabilities (provider-specific implementations)
  supportsFineTuning(): boolean;
  createFineTuning(config: FineTuningConfig): Promise<FineTuningJob>;
  loadLoRALayer(layer: LoRALayerSpec): Promise<LoadResult>;
  unloadLoRALayer(layerId: string): Promise<UnloadResult>;
  
  // Resource management
  getResourceUsage(): ResourceUsage;
  estimateComputeRequirements(task: TaskRequirements): ComputeEstimate;
}

/**
 * LoRA layer specification for different providers
 * Each provider has different LoRA implementation details
 */
export interface LoRALayerSpec {
  readonly layerId: string;
  readonly name: string;
  readonly version: string;
  readonly provider: ModelProvider;
  readonly baseModel: string;
  
  // LoRA configuration (provider-specific)
  readonly config: LoRAConfig;
  
  // Genomic metadata
  readonly capabilities: readonly PersonaCapability[];
  readonly specialization: string[];
  readonly performance: PerformanceProfile;
  readonly dependencies: readonly string[];
  
  // Binary weights and metadata
  readonly weights: CompressedWeights;
  readonly metadata: GenomicMetadata;
}

/**
 * Provider-specific LoRA configuration
 */
export type LoRAConfig = 
  | OpenAILoRAConfig
  | AnthropicLoRAConfig  
  | DeepSeekLoRAConfig
  | CustomLoRAConfig;

export interface OpenAILoRAConfig {
  readonly provider: ModelProvider.OPENAI;
  readonly rank: number;
  readonly alpha: number;
  readonly targetModules: string[];
  readonly dropoutRate: number;
  readonly fineTuningJobId?: string;
}

export interface AnthropicLoRAConfig {
  readonly provider: ModelProvider.ANTHROPIC;
  // Anthropic doesn't currently support public fine-tuning
  // This is prepared for when they do
  readonly adaptationMethod: 'future-api' | 'wrapper' | 'emulation';
  readonly emulationPrompts?: string[];
}

export interface DeepSeekLoRAConfig {
  readonly provider: ModelProvider.DEEPSEEK;
  readonly rank: number;
  readonly alpha: number;
  readonly modules: string[];
  readonly trainingConfig: DeepSeekTrainingConfig;
}

export interface CustomLoRAConfig {
  readonly provider: ModelProvider.CUSTOM | ModelProvider.LOCAL;
  readonly implementation: 'huggingface' | 'vllm' | 'custom';
  readonly config: Record<string, unknown>;
}

/**
 * Abstract persona interface
 * Sophisticated composition of base model + LoRA layers + metadata
 */
export interface AbstractPersona {
  readonly personaId: string;
  readonly displayName: string;
  readonly provider: ModelProvider;
  
  // Genomic composition
  readonly baseModel: BaseModelAdapter;
  readonly loraLayers: readonly LoRALayerSpec[];
  readonly composition: GenomicComposition;
  
  // Capability profile (computed from genomic layers)
  readonly capabilities: readonly PersonaCapability[];
  readonly specialization: readonly string[];
  readonly performance: AggregatePerformance;
  
  // Runtime state
  initialize(): Promise<InitializationResult>;
  isReady(): boolean;
  cleanup(): Promise<void>;
  
  // Core persona operations
  collaborate(request: CollaborationRequest): Promise<CollaborationResult>;
  executeTask(task: TaskSpecification): Promise<TaskResult>;
  
  // Genomic operations
  loadGenomicLayer(layer: LoRALayerSpec): Promise<LoadResult>;
  unloadGenomicLayer(layerId: string): Promise<UnloadResult>;
  evolveComposition(pressure: EvolutionaryPressure): Promise<EvolutionResult>;
  
  // Discovery and matching
  getEmbedding(): Promise<PersonaEmbedding>;
  calculateCompatibility(other: AbstractPersona): Promise<CompatibilityScore>;
  assessTaskFitness(task: TaskSpecification): Promise<FitnessScore>;
}

/**
 * Persona factory for creating provider-specific personas
 */
export interface PersonaFactory {
  readonly provider: ModelProvider;
  
  createPersona(spec: PersonaSpec): Promise<AbstractPersona>;
  loadExistingPersona(personaId: string): Promise<AbstractPersona>;
  validateGenomicComposition(composition: GenomicComposition): Promise<ValidationResult>;
  
  // Provider-specific optimizations
  optimizeForProvider(layers: LoRALayerSpec[]): Promise<OptimizedComposition>;
  estimateResourceRequirements(spec: PersonaSpec): Promise<ResourceRequirements>;
}

/**
 * Multi-provider persona registry
 * Manages personas across different model providers
 */
export interface MultiProviderPersonaRegistry {
  // Provider management
  registerProvider(factory: PersonaFactory): Promise<void>;
  getAvailableProviders(): readonly ModelProvider[];
  
  // Persona lifecycle
  createPersona(spec: PersonaSpec): Promise<AbstractPersona>;
  discoverPersonas(query: PersonaQuery): Promise<PersonaMatch[]>;
  
  // Cross-provider operations
  migratePersona(personaId: string, targetProvider: ModelProvider): Promise<MigrationResult>;
  compareProviderPerformance(task: TaskSpecification): Promise<ProviderComparison>;
}

/**
 * Grid-integrated persona discovery with cosine similarity
 */
export interface GridPersonaDiscovery {
  // Local discovery (fast vector search)
  discoverLocal(query: DiscoveryQuery): Promise<PersonaMatch[]>;
  
  // Global discovery (P2P mesh search)
  discoverGlobal(query: DiscoveryQuery): Promise<RemotePersonaMatch[]>;
  
  // Similarity-based matching
  findSimilarPersonas(embedding: PersonaEmbedding, threshold: number): Promise<SimilarityMatch[]>;
  
  // Capability-based search
  findByCapabilities(capabilities: readonly PersonaCapability[]): Promise<CapabilityMatch[]>;
}

/**
 * Supporting types for persona abstraction
 */
export interface PersonaSpec {
  readonly name: string;
  readonly provider: ModelProvider;
  readonly baseModel: string;
  readonly desiredCapabilities: readonly PersonaCapability[];
  readonly specialization: readonly string[];
  readonly resourceConstraints: ResourceConstraints;
  readonly qualityThresholds: QualityThresholds;
}

export interface GenomicComposition {
  readonly layerOrder: readonly string[];
  readonly layerWeights: ReadonlyMap<string, number>;
  readonly connections: readonly LayerConnection[];
  readonly activationPatterns: readonly ActivationPattern[];
}

export interface PersonaEmbedding {
  readonly vector: Float32Array;
  readonly dimensions: number;
  readonly normalization: 'l2' | 'cosine' | 'none';
  readonly metadata: EmbeddingMetadata;
}

export interface DiscoveryQuery {
  readonly taskDescription: string;
  readonly requiredCapabilities: readonly PersonaCapability[];
  readonly preferredProviders: readonly ModelProvider[];
  readonly resourceConstraints: ResourceConstraints;
  readonly maxLatency: number;
  readonly qualityThreshold: number;
}

export interface PersonaMatch {
  readonly persona: AbstractPersona;
  readonly similarityScore: number;
  readonly capabilityScore: number;
  readonly availabilityScore: number;
  readonly compositeScore: number;
  readonly estimatedLatency: number;
  readonly confidenceLevel: number;
}

/**
 * Future implementation placeholders
 * These types will be fully defined when implementing the actual persona system
 */
export type GenerationOptions = unknown;
export type GenerationResult = unknown;
export type EmbeddingVector = Float32Array;
export type FineTuningConfig = unknown;
export type FineTuningJob = unknown;
export type LoadResult = unknown;
export type UnloadResult = unknown;
export type ResourceUsage = unknown;
export type ComputeEstimate = unknown;
export type TaskRequirements = unknown;
export type CompressedWeights = unknown;
export type GenomicMetadata = unknown;
export type PerformanceProfile = unknown;
export type PersonaCapability = string;
export type AggregatePerformance = unknown;
export type InitializationResult = unknown;
export type CollaborationRequest = unknown;
export type CollaborationResult = unknown;
export type TaskSpecification = unknown;
export type TaskResult = unknown;
export type EvolutionaryPressure = unknown;
export type EvolutionResult = unknown;
export type CompatibilityScore = number;
export type FitnessScore = number;
export type ValidationResult = unknown;
export type OptimizedComposition = unknown;
export type ResourceRequirements = unknown;
export type PersonaQuery = unknown;
export type MigrationResult = unknown;
export type ProviderComparison = unknown;
export type RemotePersonaMatch = unknown;
export type SimilarityMatch = unknown;
export type CapabilityMatch = unknown;
export type ResourceConstraints = unknown;
export type QualityThresholds = unknown;
export type LayerConnection = unknown;
export type ActivationPattern = unknown;
export type EmbeddingMetadata = unknown;
export type DeepSeekTrainingConfig = unknown;

/**
 * Default configuration for persona abstraction
 */
export const PERSONA_ABSTRACTION_DEFAULTS = {
  SIMILARITY_THRESHOLD: 0.75,
  MAX_LORA_LAYERS: 8,
  DEFAULT_VECTOR_DIMENSIONS: 1536,
  CACHE_TTL: 300000,                    // 5 minutes
  MAX_CONCURRENT_PERSONAS: 10,
  PROVIDER_TIMEOUT: 30000,              // 30 seconds
  EMBEDDING_BATCH_SIZE: 32,
  DISCOVERY_RESULT_LIMIT: 100
} as const;