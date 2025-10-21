/**
 * Academy Database Schema - Packaged with Academy module
 * 
 * Defines collections, indices, and schema for Academy-specific data
 */

export interface AcademyCollectionSchema {
  name: string;
  schema: any;
  indices?: IndexDefinition[];
  relationships?: RelationshipDefinition[];
}

export interface IndexDefinition {
  name: string;
  fields: string[];
  unique?: boolean;
  sparse?: boolean;
}

export interface RelationshipDefinition {
  type: 'one_to_many' | 'many_to_many' | 'one_to_one';
  target_collection: string;
  foreign_key: string;
  cascade_delete?: boolean;
}

/**
 * Academy Database Schema Definition
 * 
 * All collections needed for Academy functionality
 */
export const ACADEMY_SCHEMA: AcademyCollectionSchema[] = [
  {
    name: 'optimization_records',
    schema: {
      id: { type: 'string', required: true, primary: true },
      timestamp: { type: 'datetime', required: true },
      original_composition: { type: 'object', required: true },
      optimized_composition: { type: 'object', required: true },
      optimization_steps: { type: 'array', required: true },
      impact_metrics: {
        type: 'object',
        required: true,
        properties: {
          compression_gained: { type: 'number' },
          performance_change: { type: 'number' },
          memory_reduction: { type: 'number' },
          inference_speedup: { type: 'number' }
        }
      },
      benchmark_results_before: { type: 'array', required: false },
      benchmark_results_after: { type: 'array', required: false },
      success_rating: { type: 'number', required: false }
    },
    indices: [
      { name: 'timestamp_idx', fields: ['timestamp'] },
      { name: 'success_rating_idx', fields: ['success_rating'] },
      { name: 'compression_idx', fields: ['impact_metrics.compression_gained'] }
    ]
  },

  {
    name: 'persona_genomes',
    schema: {
      id: { type: 'string', required: true, primary: true },
      uuid: { type: 'string', required: true, unique: true },
      name: { type: 'string', required: true },
      creator_node: { type: 'string', required: true },
      creation_timestamp: { type: 'datetime', required: true },
      content_hash: { type: 'string', required: true },
      derivation_type: { type: 'string', enum: ['original', 'trained', 'bred', 'fine_tuned', 'merged'] },
      genome: {
        type: 'object',
        required: true,
        properties: {
          identity: { type: 'object' },
          knowledge: { type: 'object' },
          behavior: { type: 'object' },
          evolution: { type: 'object' },
          substrate: { type: 'object' },
          reproduction: { type: 'object' }
        }
      },
      parent_personas: { type: 'array', required: false }
    },
    indices: [
      { name: 'uuid_idx', fields: ['uuid'], unique: true },
      { name: 'creator_node_idx', fields: ['creator_node'] },
      { name: 'derivation_type_idx', fields: ['derivation_type'] },
      { name: 'creation_timestamp_idx', fields: ['creation_timestamp'] }
    ]
  },

  {
    name: 'lora_layers',
    schema: {
      uuid: { type: 'string', required: true, primary: true },
      layer_name: { type: 'string', required: true },
      creation_prompt: { type: 'text', required: true },
      target_capability: { type: 'string', required: true },
      domain: { type: 'string', required: true },
      rank: { type: 'number', required: true },
      alpha: { type: 'number', required: true },
      base_model_hash: { type: 'string', required: true },
      creator_node: { type: 'string', required: true },
      creation_timestamp: { type: 'datetime', required: true },
      layer_hash: { type: 'string', required: true },
      training_dataset_uuid: { type: 'string', required: false },
      performance_metrics: {
        type: 'object',
        properties: {
          benchmark_scores: { type: 'object' },
          capability_improvement: { type: 'number' },
          specialization_strength: { type: 'number' },
          training_stability: { type: 'number' },
          cross_domain_transfer: { type: 'number' }
        }
      }
    },
    indices: [
      { name: 'domain_idx', fields: ['domain'] },
      { name: 'target_capability_idx', fields: ['target_capability'] },
      { name: 'creator_node_idx', fields: ['creator_node'] },
      { name: 'creation_timestamp_idx', fields: ['creation_timestamp'] },
      { name: 'prompt_fulltext_idx', fields: ['creation_prompt'] } // Full-text search
    ]
  },

  {
    name: 'lora_compositions',
    schema: {
      uuid: { type: 'string', required: true, primary: true },
      name: { type: 'string', required: true },
      component_layer_uuids: { type: 'array', required: true },
      composition_strategy: { type: 'string', required: true },
      composition_prompt: { type: 'text', required: true },
      expected_capabilities: { type: 'array', required: true },
      creator_node: { type: 'string', required: true },
      creation_timestamp: { type: 'datetime', required: true },
      composition_hash: { type: 'string', required: true },
      performance_validation: { type: 'object', required: false }
    },
    indices: [
      { name: 'creator_node_idx', fields: ['creator_node'] },
      { name: 'creation_timestamp_idx', fields: ['creation_timestamp'] },
      { name: 'composition_strategy_idx', fields: ['composition_strategy'] }
    ],
    relationships: [
      {
        type: 'many_to_many',
        target_collection: 'lora_layers',
        foreign_key: 'component_layer_uuids'
      }
    ]
  },

  {
    name: 'training_resources',
    schema: {
      uuid: { type: 'string', required: true, primary: true },
      resource_type: { type: 'string', enum: ['dataset', 'prompt_collection', 'conversation_log', 'code_repository'] },
      name: { type: 'string', required: true },
      description: { type: 'text', required: true },
      creation_prompt: { type: 'text', required: false },
      content_hash: { type: 'string', required: true },
      size_bytes: { type: 'number', required: true },
      creator_node: { type: 'string', required: true },
      creation_timestamp: { type: 'datetime', required: true },
      license: { type: 'string', required: true },
      quality_score: { type: 'number', required: true },
      domain_tags: { type: 'array', required: true }
    },
    indices: [
      { name: 'resource_type_idx', fields: ['resource_type'] },
      { name: 'domain_tags_idx', fields: ['domain_tags'] },
      { name: 'quality_score_idx', fields: ['quality_score'] },
      { name: 'creator_node_idx', fields: ['creator_node'] },
      { name: 'size_bytes_idx', fields: ['size_bytes'] }
    ]
  },

  {
    name: 'prompt_bindings',
    schema: {
      layer_uuid: { type: 'string', required: true, primary: true },
      creation_prompt: { type: 'text', required: true },
      prompt_context: { type: 'text', required: true },
      target_behavior: { type: 'string', required: true },
      success_criteria: { type: 'array', required: true },
      actual_outcomes: { type: 'array', required: false },
      refinement_prompts: { type: 'array', required: false },
      prompt_metadata: {
        type: 'object',
        properties: {
          prompt_author: { type: 'string' },
          prompt_version: { type: 'number' },
          effectiveness_score: { type: 'number' },
          reusability_score: { type: 'number' },
          complexity_level: { type: 'number' },
          domain_specificity: { type: 'number' },
          language_style: { type: 'string' },
          prompt_patterns: { type: 'array' }
        }
      }
    },
    indices: [
      { name: 'target_behavior_idx', fields: ['target_behavior'] },
      { name: 'effectiveness_score_idx', fields: ['prompt_metadata.effectiveness_score'] },
      { name: 'prompt_fulltext_idx', fields: ['creation_prompt'] }
    ],
    relationships: [
      {
        type: 'one_to_one',
        target_collection: 'lora_layers',
        foreign_key: 'layer_uuid'
      }
    ]
  },

  {
    name: 'benchmark_results',
    schema: {
      batch_id: { type: 'string', required: true, primary: true },
      timestamp: { type: 'datetime', required: true },
      benchmarks: {
        type: 'array',
        required: true,
        items: {
          type: 'object',
          properties: {
            benchmark_id: { type: 'string' },
            domain: { type: 'string' },
            task_type: { type: 'string' },
            baseline_score: { type: 'number' },
            current_score: { type: 'number' },
            improvement: { type: 'number' },
            execution_time_ms: { type: 'number' },
            memory_usage_mb: { type: 'number' }
          }
        }
      }
    },
    indices: [
      { name: 'timestamp_idx', fields: ['timestamp'] },
      { name: 'benchmark_domain_idx', fields: ['benchmarks.domain'] }
    ]
  }
];

/**
 * Academy Database Migration Scripts
 */
export const ACADEMY_MIGRATIONS = {
  '1.0.0': {
    description: 'Initial Academy schema creation',
    up: [
      // SQL or NoSQL commands to create collections
      'CREATE_COLLECTIONS',
      'CREATE_INDICES',
      'SET_PERMISSIONS'
    ],
    down: [
      'DROP_COLLECTIONS'
    ]
  },

  '1.1.0': {
    description: 'Add performance validation to compositions',
    up: [
      'ALTER_COLLECTION lora_compositions ADD FIELD performance_validation OBJECT'
    ],
    down: [
      'ALTER_COLLECTION lora_compositions DROP FIELD performance_validation'
    ]
  }
};

/**
 * Academy Collection Manager
 * 
 * Handles schema creation and validation for Academy collections
 */
export class AcademyCollectionManager {
  private databaseDaemon: any;

  constructor(databaseDaemon: any) {
    this.databaseDaemon = databaseDaemon;
  }

  /**
   * Initialize all Academy collections with schema validation
   */
  async initializeCollections(): Promise<void> {
    console.log('🏗️ Initializing Academy database collections...');

    for (const collectionDef of ACADEMY_SCHEMA) {
      try {
        const result = await this.databaseDaemon.handleMessage({
          type: 'create_collection',
          data: {
            collection_name: `academy_${collectionDef.name}`,
            schema: collectionDef.schema,
            indices: collectionDef.indices,
            relationships: collectionDef.relationships
          }
        });

        if (result.success) {
          console.log(`✅ Created collection: academy_${collectionDef.name}`);
        } else {
          console.log(`ℹ️ Collection academy_${collectionDef.name} already exists`);
        }

      } catch (error) {
        console.warn(`⚠️ Failed to create collection academy_${collectionDef.name}:`, error);
      }
    }

    console.log('✅ Academy collections initialized');
  }

  /**
   * Validate data against schema before saving
   */
  validateData(collectionName: string, data: any): { valid: boolean; errors: string[] } {
    const schema = ACADEMY_SCHEMA.find(s => s.name === collectionName)?.schema;
    
    if (!schema) {
      return { valid: false, errors: [`Unknown collection: ${collectionName}`] };
    }

    // Simple validation - in production would use proper schema validator
    const errors: string[] = [];
    
    for (const [field, rules] of Object.entries(schema)) {
      const fieldRules = rules as any;
      
      if (fieldRules.required && !(field in data)) {
        errors.push(`Missing required field: ${field}`);
      }
      
      if (fieldRules.type && data[field] !== undefined) {
        const actualType = typeof data[field];
        if (fieldRules.type === 'datetime' && !(data[field] instanceof Date)) {
          errors.push(`Field ${field} must be a Date`);
        } else if (fieldRules.type === 'array' && !Array.isArray(data[field])) {
          errors.push(`Field ${field} must be an array`);
        } else if (fieldRules.type === 'object' && actualType !== 'object') {
          errors.push(`Field ${field} must be an object`);
        } else if (fieldRules.type === 'string' && actualType !== 'string') {
          errors.push(`Field ${field} must be a string`);
        } else if (fieldRules.type === 'number' && actualType !== 'number') {
          errors.push(`Field ${field} must be a number`);
        }
      }
    }

    return { valid: errors.length === 0, errors };
  }

  /**
   * Get collection schema
   */
  getCollectionSchema(collectionName: string): any {
    return ACADEMY_SCHEMA.find(s => s.name === collectionName)?.schema;
  }

  /**
   * Get all Academy collection names
   */
  getCollectionNames(): string[] {
    return ACADEMY_SCHEMA.map(s => `academy_${s.name}`);
  }
}