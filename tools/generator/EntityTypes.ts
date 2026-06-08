/**
 * EntityTypes - Type definitions for entity specifications
 *
 * Used by EntityGenerator to create entity classes from declarative specs
 */

export interface ValidationRule {
  // String validation
  minLength?: number;
  maxLength?: number;
  pattern?: string; // Regex pattern

  // Number validation
  min?: number;
  max?: number;

  // Enum validation
  enum?: string[] | number[];

  // Custom validation message
  message?: string;
}

export interface EntityField {
  type: 'string' | 'number' | 'boolean' | 'UUID' | 'Date' | 'JSON' | 'array';
  optional?: boolean;
  description: string;
  default?: any;
  arrayItemType?: string; // For array type
  validation?: ValidationRule; // Validation rules for this field
}

export interface EntityIndex {
  fields: string[];
  unique?: boolean;
}

/**
 * Complete specification for generating an entity
 */
export interface EntitySpec {
  name: string; // PascalCase: 'GenomeAdapter', 'TrainingDataset'
  collectionName: string; // snake_case: 'genome_adapters', 'training_datasets'
  description: string;
  fields: Record<string, EntityField>;
  indexes?: EntityIndex[];
  extends?: string; // Optional parent entity class (e.g., 'BaseEntity')
}
