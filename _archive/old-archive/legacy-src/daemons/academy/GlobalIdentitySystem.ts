/**
 * Global Identity System - UUIDs for P2P sharing of personas, layers, and resources
 * 
 * Everything that can be shared needs a global UUID:
 * - Personas
 * - LoRA layers  
 * - Training resources
 * - Datasets
 * - Prompts
 * - Compositions
 */

// TODO: Add @types/uuid package for proper typing
// eslint-disable-next-line @typescript-eslint/no-var-requires
const { v4: uuidv4, v5: uuidv5 } = require('uuid'); //use import here
import * as crypto from 'crypto';

// Global namespace UUIDs for different types of entities
export const CONTINUUM_NAMESPACES = {
  PERSONA: '6ba7b810-9dad-11d1-80b4-00c04fd430c8',           // For persona UUIDs
  LORA_LAYER: '6ba7b811-9dad-11d1-80b4-00c04fd430c8',        // For individual LoRA layers
  TRAINING_RESOURCE: '6ba7b812-9dad-11d1-80b4-00c04fd430c8', // For datasets/prompts
  COMPOSITION: '6ba7b813-9dad-11d1-80b4-00c04fd430c8',       // For layer compositions
  PROMPT_TEMPLATE: '6ba7b814-9dad-11d1-80b4-00c04fd430c8',   // For reusable prompts
  TRAINING_SESSION: '6ba7b815-9dad-11d1-80b4-00c04fd430c8'   // For training sessions
};

export interface GlobalPersonaIdentity {
  uuid: string;                         // Global UUID for P2P sharing
  local_id: string;                     // Local system ID
  name: string;                         // Human-readable name
  version: number;                      // Version number for updates
  creator_node: string;                 // Node that created this persona
  creation_timestamp: Date;
  content_hash: string;                 // Hash of actual persona content
  parent_personas?: string[];           // UUIDs of parent personas (for bred personas)
  derivation_type?: 'original' | 'trained' | 'bred' | 'fine_tuned' | 'merged';
}

export interface GlobalLoRALayerIdentity {
  uuid: string;                         // Global UUID for P2P sharing
  layer_name: string;                   // Human-readable name
  creation_prompt: string;              // THE PROMPT that created this layer
  target_capability: string;            // What this layer was meant to achieve
  domain: string;                       // Primary domain
  rank: number;                         // LoRA rank
  alpha: number;                        // LoRA alpha
  base_model_hash: string;              // Hash of the base model this works with
  training_dataset_uuid?: string;       // UUID of training dataset used
  creator_node: string;                 // Node that created this layer
  creation_timestamp: Date;
  layer_hash: string;                   // Hash of actual layer weights
  parent_layers?: string[];             // UUIDs of layers this was derived from
  performance_metrics: LayerPerformanceMetrics;
}

export interface LayerPerformanceMetrics {
  benchmark_scores: Record<string, number>;  // benchmark_name -> score
  capability_improvement: number;            // 0-1 improvement over base
  specialization_strength: number;          // 0-1 how specialized vs general
  training_stability: number;               // 0-1 how stable during training
  cross_domain_transfer: number;            // 0-1 how well it transfers
}

export interface GlobalTrainingResourceIdentity {
  uuid: string;                         // Global UUID for P2P sharing
  resource_type: 'dataset' | 'prompt_collection' | 'conversation_log' | 'code_repository';
  name: string;                         // Human-readable name
  description: string;                  // What this resource contains
  creation_prompt?: string;             // Prompt used to create/curate this
  content_hash: string;                 // Hash of resource content
  size_bytes: number;                   // Size for P2P transfer planning
  creator_node: string;                 // Node that created/curated this
  creation_timestamp: Date;
  license: string;                      // Usage rights
  quality_score: number;                // 0-1 assessed quality
  domain_tags: string[];                // Searchable domain tags
}

export interface PromptLayerBinding {
  layer_uuid: string;                   // UUID of the LoRA layer
  creation_prompt: string;              // THE PROMPT that created this layer
  prompt_context: string;               // Context around why this prompt was used
  target_behavior: string;              // What behavior this was meant to create
  success_criteria: string[];           // How to measure if it worked
  actual_outcomes: string[];            // What actually emerged
  refinement_prompts?: string[];        // Follow-up prompts used for refinement
  prompt_metadata: PromptMetadata;
}

export interface PromptMetadata {
  prompt_author: string;                // Who wrote this prompt
  prompt_version: number;               // Version of this prompt
  effectiveness_score: number;          // 0-1 how effective this prompt was
  reusability_score: number;           // 0-1 how reusable for other contexts
  complexity_level: number;            // 1-10 complexity
  domain_specificity: number;          // 0-1 how domain-specific vs general
  language_style: string;              // 'technical', 'conversational', 'instructional'
  prompt_patterns: string[];           // Patterns used (e.g., 'chain_of_thought', 'few_shot')
}

export interface PersonaCompositionIdentity {
  uuid: string;                         // Global UUID for this composition
  name: string;                         // Human-readable name
  component_layer_uuids: string[];      // UUIDs of all LoRA layers
  composition_strategy: string;         // How layers are combined
  composition_prompt: string;           // Prompt describing desired composition
  expected_capabilities: string[];      // What this composition should be able to do
  creator_node: string;
  creation_timestamp: Date;
  composition_hash: string;             // Hash of the composition configuration
  performance_validation?: CompositionValidation;
}

export interface CompositionValidation {
  validation_prompts: string[];         // Prompts used to test the composition
  validation_results: ValidationResult[];
  overall_success_rate: number;        // 0-1
  validation_timestamp: Date;
}

export interface ValidationResult {
  test_prompt: string;
  expected_response_pattern: string;
  actual_response: string;
  success: boolean;
  score: number;                       // 0-1
  notes: string;
}

/**
 * Global Identity System
 * 
 * Manages UUIDs and identity for all shareable Academy components.
 * 
 * Key principles:
 * 1. Every shareable entity gets a global UUID
 * 2. Content-addressable hashing for integrity
 * 3. Versioning for evolution tracking
 * 4. Provenance tracking for trust
 * 5. Prompt-per-layer binding for understanding
 */
export class GlobalIdentitySystem {
  private personaRegistry: Map<string, GlobalPersonaIdentity> = new Map();
  private layerRegistry: Map<string, GlobalLoRALayerIdentity> = new Map();
  private resourceRegistry: Map<string, GlobalTrainingResourceIdentity> = new Map();
  private promptLayerBindings: Map<string, PromptLayerBinding> = new Map(); // layer_uuid -> binding
  private compositionRegistry: Map<string, PersonaCompositionIdentity> = new Map();
  
  private nodeId: string;

  constructor(nodeId?: string) {
    this.nodeId = nodeId || this.generateNodeId();
  }

  /**
   * Register a new persona with global identity
   */
  registerPersona(
    localId: string,
    name: string,
    personaContent: any,
    parentPersonas?: string[]
  ): GlobalPersonaIdentity {
    // Generate deterministic UUID based on content + creator
    const contentHash = this.calculateContentHash(personaContent);
    const seedString = `${this.nodeId}:${name}:${contentHash}`;
    const uuid = uuidv5(seedString, CONTINUUM_NAMESPACES.PERSONA);
    // TODO: Use uuidv4 for random IDs when needed
    if (false) console.log('TODO: Random UUID option:', uuidv4());

    const identity: GlobalPersonaIdentity = {
      uuid,
      local_id: localId,
      name,
      version: 1,
      creator_node: this.nodeId,
      creation_timestamp: new Date(),
      content_hash: contentHash,
      derivation_type: parentPersonas ? 'bred' : 'original',
      ...(parentPersonas && { parent_personas: parentPersonas })
    };

    this.personaRegistry.set(uuid, identity);
    
    console.log(`🆔 Registered persona: ${name} (${uuid})`);
    return identity;
  }

  /**
   * Register a new LoRA layer with its creation prompt
   */
  registerLoRALayer(
    layerName: string,
    creationPrompt: string,
    targetCapability: string,
    domain: string,
    layerConfig: { rank: number; alpha: number },
    layerWeights: any,
    baseModelHash: string,
    trainingDatasetUuid?: string
  ): GlobalLoRALayerIdentity {
    // Generate deterministic UUID based on prompt + config + weights hash
    const layerHash = this.calculateContentHash(layerWeights);
    const seedString = `${creationPrompt}:${domain}:${layerConfig.rank}:${layerConfig.alpha}:${layerHash}`;
    const uuid = uuidv5(seedString, CONTINUUM_NAMESPACES.LORA_LAYER);

    const identity: GlobalLoRALayerIdentity = {
      uuid,
      layer_name: layerName,
      creation_prompt: creationPrompt,
      target_capability: targetCapability,
      domain,
      rank: layerConfig.rank,
      alpha: layerConfig.alpha,
      base_model_hash: baseModelHash,
      ...(trainingDatasetUuid && { training_dataset_uuid: trainingDatasetUuid }),
      creator_node: this.nodeId,
      creation_timestamp: new Date(),
      layer_hash: layerHash,
      performance_metrics: {
        benchmark_scores: {},
        capability_improvement: 0,
        specialization_strength: 0,
        training_stability: 0,
        cross_domain_transfer: 0
      }
    };

    this.layerRegistry.set(uuid, identity);

    // Create prompt-layer binding
    const binding: PromptLayerBinding = {
      layer_uuid: uuid,
      creation_prompt: creationPrompt,
      prompt_context: `Layer created for ${targetCapability} in ${domain}`,
      target_behavior: targetCapability,
      success_criteria: [`Improved performance in ${domain}`, `Specialized for ${targetCapability}`],
      actual_outcomes: [], // To be filled in later
      prompt_metadata: {
        prompt_author: this.nodeId,
        prompt_version: 1,
        effectiveness_score: 0, // To be measured
        reusability_score: 0.5, // Default assumption
        complexity_level: this.assessPromptComplexity(creationPrompt),
        domain_specificity: domain === 'general' ? 0.2 : 0.8,
        language_style: this.detectLanguageStyle(creationPrompt),
        prompt_patterns: this.extractPromptPatterns(creationPrompt)
      }
    };

    this.promptLayerBindings.set(uuid, binding);

    console.log(`🧩 Registered LoRA layer: ${layerName} (${uuid})`);
    console.log(`📝 Creation prompt: "${creationPrompt.substring(0, 100)}..."`);
    
    return identity;
  }

  /**
   * Register a training resource with global identity
   */
  registerTrainingResource(
    name: string,
    description: string,
    resourceType: string,
    content: any,
    creationPrompt?: string,
    license: string = 'MIT'
  ): GlobalTrainingResourceIdentity {
    const contentHash = this.calculateContentHash(content);
    const seedString = `${this.nodeId}:${name}:${contentHash}`;
    const uuid = uuidv5(seedString, CONTINUUM_NAMESPACES.TRAINING_RESOURCE);

    const identity: GlobalTrainingResourceIdentity = {
      uuid,
      resource_type: resourceType as any,
      name,
      description,
      ...(creationPrompt && { creation_prompt: creationPrompt }),
      content_hash: contentHash,
      size_bytes: JSON.stringify(content).length,
      creator_node: this.nodeId,
      creation_timestamp: new Date(),
      license,
      quality_score: 0.7, // Default assumption
      domain_tags: this.extractDomainTags(description)
    };

    this.resourceRegistry.set(uuid, identity);

    console.log(`📚 Registered training resource: ${name} (${uuid})`);
    return identity;
  }

  /**
   * Register a persona composition with its creation intent
   */
  registerComposition(
    name: string,
    layerUuids: string[],
    compositionStrategy: string,
    compositionPrompt: string,
    expectedCapabilities: string[]
  ): PersonaCompositionIdentity {
    const seedString = `${this.nodeId}:${name}:${layerUuids.sort().join(':')}:${compositionStrategy}`;
    const uuid = uuidv5(seedString, CONTINUUM_NAMESPACES.COMPOSITION);

    const compositionHash = this.calculateContentHash({
      layers: layerUuids,
      strategy: compositionStrategy,
      capabilities: expectedCapabilities
    });

    const identity: PersonaCompositionIdentity = {
      uuid,
      name,
      component_layer_uuids: layerUuids,
      composition_strategy: compositionStrategy,
      composition_prompt: compositionPrompt,
      expected_capabilities: expectedCapabilities,
      creator_node: this.nodeId,
      creation_timestamp: new Date(),
      composition_hash: compositionHash
    };

    this.compositionRegistry.set(uuid, identity);

    console.log(`🧬 Registered composition: ${name} (${uuid}) with ${layerUuids.length} layers`);
    return identity;
  }

  /**
   * Find layers by creation prompt similarity
   */
  findLayersByPrompt(queryPrompt: string, similarityThreshold: number = 0.7): GlobalLoRALayerIdentity[] {
    const results: GlobalLoRALayerIdentity[] = [];

    for (const [_uuid, layer] of this.layerRegistry) {
      const similarity = this.calculatePromptSimilarity(queryPrompt, layer.creation_prompt);
      if (similarity >= similarityThreshold) {
        results.push(layer);
      }
    }

    return results.sort((a, b) => 
      this.calculatePromptSimilarity(queryPrompt, b.creation_prompt) - 
      this.calculatePromptSimilarity(queryPrompt, a.creation_prompt)
    );
  }

  /**
   * Get prompt-layer binding for understanding layer purpose
   */
  getPromptLayerBinding(layerUuid: string): PromptLayerBinding | undefined {
    return this.promptLayerBindings.get(layerUuid);
  }

  /**
   * Update layer performance metrics after validation
   */
  updateLayerPerformance(
    layerUuid: string,
    benchmarkResults: Record<string, number>,
    actualOutcomes: string[]
  ): void {
    const layer = this.layerRegistry.get(layerUuid);
    const binding = this.promptLayerBindings.get(layerUuid);

    if (layer) {
      layer.performance_metrics.benchmark_scores = {
        ...layer.performance_metrics.benchmark_scores,
        ...benchmarkResults
      };

      // Calculate overall improvement
      const scores = Object.values(benchmarkResults);
      layer.performance_metrics.capability_improvement = scores.reduce((sum, score) => sum + score, 0) / scores.length;
    }

    if (binding) {
      binding.actual_outcomes = [...binding.actual_outcomes, ...actualOutcomes];
      
      // Update prompt effectiveness based on results
      const avgScore = Object.values(benchmarkResults).reduce((sum, score) => sum + score, 0) / Object.values(benchmarkResults).length;
      binding.prompt_metadata.effectiveness_score = avgScore;
    }

    console.log(`📊 Updated performance for layer ${layerUuid}`);
  }

  /**
   * Export identities for P2P sharing
   */
  exportForP2P(): P2PIdentityExport {
    return {
      node_id: this.nodeId,
      export_timestamp: new Date(),
      personas: Array.from(this.personaRegistry.values()),
      lora_layers: Array.from(this.layerRegistry.values()),
      training_resources: Array.from(this.resourceRegistry.values()),
      compositions: Array.from(this.compositionRegistry.values()),
      prompt_bindings: Array.from(this.promptLayerBindings.values())
    };
  }

  /**
   * Import identities from P2P network
   */
  importFromP2P(export_data: P2PIdentityExport): ImportResult {
    const result: ImportResult = {
      imported_personas: 0,
      imported_layers: 0,
      imported_resources: 0,
      imported_compositions: 0,
      conflicts: [],
      new_nodes_discovered: []
    };

    // Import personas
    for (const persona of export_data.personas) {
      if (!this.personaRegistry.has(persona.uuid)) {
        this.personaRegistry.set(persona.uuid, persona);
        result.imported_personas++;
      }
    }

    // Import LoRA layers
    for (const layer of export_data.lora_layers) {
      if (!this.layerRegistry.has(layer.uuid)) {
        this.layerRegistry.set(layer.uuid, layer);
        result.imported_layers++;
      }
    }

    // Import prompt bindings
    for (const binding of export_data.prompt_bindings) {
      if (!this.promptLayerBindings.has(binding.layer_uuid)) {
        this.promptLayerBindings.set(binding.layer_uuid, binding);
      }
    }

    // Track new nodes
    const knownNodes = new Set([this.nodeId]);
    for (const persona of export_data.personas) {
      if (!knownNodes.has(persona.creator_node)) {
        result.new_nodes_discovered.push(persona.creator_node);
        knownNodes.add(persona.creator_node);
      }
    }

    console.log(`📥 P2P Import: ${result.imported_personas} personas, ${result.imported_layers} layers`);
    return result;
  }

  // Private utility methods

  private generateNodeId(): string {
    // Generate a unique node ID for this Continuum instance
    return `node_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  private calculateContentHash(content: any): string {
    const jsonString = JSON.stringify(content, Object.keys(content).sort());
    return crypto.createHash('sha256').update(jsonString).digest('hex');
  }

  private assessPromptComplexity(prompt: string): number {
    // Simple heuristic for prompt complexity
    const factors = [
      prompt.length / 100, // Length factor
      (prompt.match(/\b(if|when|unless|provided|given)\b/gi) || []).length, // Conditional complexity
      (prompt.match(/\b(step|first|then|next|finally)\b/gi) || []).length, // Sequential complexity
      (prompt.match(/\b(example|instance|such as|like)\b/gi) || []).length // Example complexity
    ];

    return Math.min(10, factors.reduce((sum, factor) => sum + factor, 0));
  }

  private detectLanguageStyle(prompt: string): string {
    if (prompt.includes('implement') || prompt.includes('code') || prompt.includes('function')) {
      return 'technical';
    }
    if (prompt.includes('explain') || prompt.includes('teach') || prompt.includes('show')) {
      return 'instructional';
    }
    return 'conversational';
  }

  private extractPromptPatterns(prompt: string): string[] {
    const patterns: string[] = [];
    
    if (prompt.includes('step by step') || prompt.includes('first') && prompt.includes('then')) {
      patterns.push('chain_of_thought');
    }
    if (prompt.includes('example') || prompt.includes('for instance')) {
      patterns.push('few_shot');
    }
    if (prompt.includes('think about') || prompt.includes('consider')) {
      patterns.push('reflective');
    }
    if (prompt.includes('role') || prompt.includes('act as')) {
      patterns.push('role_playing');
    }

    return patterns;
  }

  private extractDomainTags(description: string): string[] {
    const domainKeywords = [
      'programming', 'software', 'web', 'mobile', 'data', 'machine learning',
      'biophysics', 'chemistry', 'physics', 'biology', 'geology', 'mathematics',
      'business', 'finance', 'marketing', 'legal', 'medical', 'education'
    ];

    return domainKeywords.filter(keyword => 
      description.toLowerCase().includes(keyword)
    );
  }

  private calculatePromptSimilarity(prompt1: string, prompt2: string): number {
    // Simple similarity based on shared words
    const words1 = new Set(prompt1.toLowerCase().split(/\s+/));
    const words2 = new Set(prompt2.toLowerCase().split(/\s+/));
    
    const intersection = new Set([...words1].filter(word => words2.has(word)));
    const union = new Set([...words1, ...words2]);
    
    return intersection.size / union.size; // Jaccard similarity
  }
}

// Supporting interfaces

export interface P2PIdentityExport {
  node_id: string;
  export_timestamp: Date;
  personas: GlobalPersonaIdentity[];
  lora_layers: GlobalLoRALayerIdentity[];
  training_resources: GlobalTrainingResourceIdentity[];
  compositions: PersonaCompositionIdentity[];
  prompt_bindings: PromptLayerBinding[];
}

export interface ImportResult {
  imported_personas: number;
  imported_layers: number;
  imported_resources: number;
  imported_compositions: number;
  conflicts: string[];
  new_nodes_discovered: string[];
}