/**
 * PersonaManager Integration Types
 * 
 * Shared types for persona management operations across client/server implementations
 */

import { PersonaGenome, PersonaRole, Specialization } from '../../../shared/AcademyTypes';

// ==================== PERSONA MANAGER REQUEST TYPES ====================

export interface CreatePersonaRequest {
  name: string;
  specialization: Specialization;
  role?: PersonaRole;
  parentIds?: string[];
  customTraits?: PersonaCustomTraits;
}

export interface PersonaCustomTraits {
  personality?: Partial<PersonalityTraits>;
  knowledge?: Partial<KnowledgeTraits>;
  behavior?: Partial<BehaviorTraits>;
}

export interface PersonalityTraits {
  creativity: number;
  analytical: number;
  helpfulness: number;
  competitiveness: number;
  patience: number;
  innovation: number;
}

export interface KnowledgeTraits {
  domainExpertise: number;
  learningSpeed: number;
  memoryRetention: number;
  transferability: number;
}

export interface BehaviorTraits {
  adaptability: number;
  collaboration: number;
  communication: number;
  persistence: number;
}

// ==================== PERSONA MANAGER RESPONSE TYPES ====================

export interface CreatePersonaResponse {
  success: boolean;
  persona?: PersonaGenome;
  error?: string;
  warnings?: string[];
}

export interface PersonaSearchRequest {
  filters?: PersonaFilters;
  sortBy?: PersonaSortField;
  sortOrder?: 'asc' | 'desc';
  limit?: number;
  offset?: number;
}

export interface PersonaFilters {
  role?: PersonaRole;
  specialization?: Specialization;
  generation?: number;
  minFitness?: number;
  maxFitness?: number;
  hasAncestors?: boolean;
  hasDescendants?: boolean;
  evolutionStage?: string;
}

export type PersonaSortField = 'name' | 'fitness' | 'generation' | 'created' | 'specialization';

export interface PersonaSearchResponse {
  success: boolean;
  personas: PersonaGenome[];
  total: number;
  hasMore: boolean;
  error?: string;
}

// ==================== PERSONA LIFECYCLE TYPES ====================

export interface PersonaUpdateRequest {
  personaId: string;
  updates: PersonaUpdates;
}

export interface PersonaUpdates {
  fitness?: number;
  role?: PersonaRole;
  experiencePoints?: number;
  newTraits?: string[];
  mutationEvent?: MutationEventData;
}

export interface MutationEventData {
  type: 'spontaneous' | 'induced' | 'crossover';
  trigger: string;
  changes: Record<string, any>;
}

export interface PersonaUpdateResponse {
  success: boolean;
  updatedPersona?: PersonaGenome;
  error?: string;
}

// ==================== PERSONA STATISTICS TYPES ====================

export interface PersonaStatistics {
  totalPersonas: number;
  roleDistribution: Record<PersonaRole, number>;
  specializationDistribution: Record<Specialization, number>;
  generationDistribution: Record<number, number>;
  averageFitness: number;
  topPerformers: PersonaGenome[];
  evolutionTrends: EvolutionTrend[];
}

export interface EvolutionTrend {
  generation: number;
  averageFitness: number;
  diversity: number;
  innovations: number;
  extinctions: number;
}

export interface PersonaStatisticsRequest {
  includeTopPerformers?: boolean;
  includeEvolutionTrends?: boolean;
  generationRange?: [number, number];
}

export interface PersonaStatisticsResponse {
  success: boolean;
  statistics?: PersonaStatistics;
  error?: string;
}

// ==================== PERSONA CROSSOVER TYPES ====================

export interface PersonaCrossoverRequest {
  parent1Id: string;
  parent2Id: string;
  childName?: string;
  inheritanceWeights?: CrossoverWeights;
}

export interface CrossoverWeights {
  personality: number;
  knowledge: number;
  behavior: number;
  specialization: number;
}

export interface PersonaCrossoverResponse {
  success: boolean;
  offspring?: PersonaGenome;
  crossoverReport?: CrossoverReport;
  error?: string;
}

export interface CrossoverReport {
  inheritedTraits: Record<string, string>; // trait -> parent source
  newTraits: string[];
  mutationEvents: string[];
  compatibilityScore: number;
}

// ==================== PERSONA VALIDATION TYPES ====================

export interface PersonaValidationRequest {
  persona: PersonaGenome;
  validationLevel: 'basic' | 'comprehensive' | 'strict';
}

export interface PersonaValidationResponse {
  success: boolean;
  isValid: boolean;
  errors: ValidationError[];
  warnings: ValidationWarning[];
}

export interface ValidationError {
  field: string;
  message: string;
  severity: 'error' | 'warning';
  code: string;
}

export interface ValidationWarning {
  field: string;
  message: string;
  suggestion?: string;
  code: string;
}

// ==================== PERSONA MANAGER OPERATION TYPES ====================

export interface PersonaManagerOperation {
  type: 'create' | 'update' | 'delete' | 'search' | 'crossover' | 'validate';
  timestamp: Date;
  personaId?: string;
  operationId: string;
  success: boolean;
  error?: string;
  metadata?: Record<string, any>;
}

export interface PersonaManagerStatus {
  totalOperations: number;
  successfulOperations: number;
  failedOperations: number;
  recentOperations: PersonaManagerOperation[];
  averageResponseTime: number;
  systemHealth: 'healthy' | 'degraded' | 'unhealthy';
}

// ==================== UTILITY TYPES ====================

export interface PersonaExportRequest {
  personaIds?: string[];
  format: 'json' | 'csv' | 'yaml';
  includeLineage?: boolean;
  includeMetrics?: boolean;
}

export interface PersonaExportResponse {
  success: boolean;
  data?: string;
  filename?: string;
  error?: string;
}

export interface PersonaImportRequest {
  data: string;
  format: 'json' | 'csv' | 'yaml';
  validateBeforeImport?: boolean;
  overwriteExisting?: boolean;
}

export interface PersonaImportResponse {
  success: boolean;
  importedCount: number;
  skippedCount: number;
  errors: string[];
  warnings: string[];
}

// ==================== CONSTANTS ====================

export const PERSONA_MANAGER_CONSTANTS = {
  MAX_SEARCH_RESULTS: 1000,
  DEFAULT_SEARCH_LIMIT: 50,
  MAX_BATCH_SIZE: 100,
  VALIDATION_TIMEOUT: 5000,
  OPERATION_TIMEOUT: 30000,
  DEFAULT_CROSSOVER_WEIGHTS: {
    personality: 0.3,
    knowledge: 0.4,
    behavior: 0.2,
    specialization: 0.1
  } as CrossoverWeights
} as const;

// ==================== TYPE GUARDS ====================

export function isCreatePersonaRequest(obj: any): obj is CreatePersonaRequest {
  return obj && typeof obj.name === 'string' && typeof obj.specialization === 'string';
}

export function isPersonaUpdateRequest(obj: any): obj is PersonaUpdateRequest {
  return obj && typeof obj.personaId === 'string' && obj.updates;
}

export function isPersonaCrossoverRequest(obj: any): obj is PersonaCrossoverRequest {
  return obj && typeof obj.parent1Id === 'string' && typeof obj.parent2Id === 'string';
}

export function isPersonaSearchRequest(obj: any): obj is PersonaSearchRequest {
  return obj && (obj.filters || obj.sortBy || obj.limit !== undefined);
}

// ==================== GENERAL REQUEST/RESPONSE TYPES ====================

export type PersonaManagerRequest = 
  | CreatePersonaRequest
  | PersonaUpdateRequest
  | PersonaCrossoverRequest
  | PersonaSearchRequest
  | PersonaDeleteRequest
  | PersonaExportRequest
  | PersonaImportRequest;

export type PersonaManagerResponse = 
  | CreatePersonaResponse
  | PersonaUpdateResponse
  | PersonaCrossoverResponse
  | PersonaSearchResponse
  | PersonaDeleteResponse
  | PersonaExportResponse
  | PersonaImportResponse;

// ==================== CONSTANTS ====================

export const PERSONA_MANAGER_CONSTANTS = {
  DEFAULT_POPULATION_SIZE: 10,
  DEFAULT_MUTATION_RATE: 0.1,
  DEFAULT_CROSSOVER_RATE: 0.3,
  MIN_FITNESS_THRESHOLD: 0.1,
  MAX_GENERATION_STAGNATION: 5
} as const;