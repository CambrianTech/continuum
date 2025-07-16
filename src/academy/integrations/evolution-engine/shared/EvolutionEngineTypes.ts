/**
 * Evolution Engine Types - Shared definitions for evolution system
 * 
 * This module contains all type definitions for the evolution engine
 * following the middle-out modular pattern.
 */

import { PersonaGenome, EvolutionaryPressure } from '../../../shared/AcademyTypes';

// ==================== EVOLUTION ENGINE TYPES ====================

/**
 * Evolution engine configuration
 */
export interface EvolutionEngineConfig {
  generations: number;
  populationSize: number;
  survivalRate: number;
  mutationRate: number;
  crossoverRate: number;
  elitePreservation: boolean;
  diversityThreshold: number;
  convergenceThreshold: number;
}

/**
 * Evolution engine request
 */
export interface EvolutionEngineRequest {
  config: EvolutionEngineConfig;
  initialPopulation: PersonaGenome[];
  evolutionaryPressure: EvolutionaryPressure;
  sessionId?: string;
}

/**
 * Evolution engine response
 */
export interface EvolutionEngineResponse {
  success: boolean;
  generations: number;
  finalPopulation: PersonaGenome[];
  evolutionHistory: GenerationResult[];
  ecosystemMetrics: EcosystemMetrics;
  error?: string;
}

/**
 * Single generation result
 */
export interface GenerationResult {
  generation: number;
  population: PersonaGenome[];
  survivors: PersonaGenome[];
  newPersonas: PersonaGenome[];
  metrics: GenerationMetrics;
  timestamp: number;
}

/**
 * Generation-specific metrics
 */
export interface GenerationMetrics {
  averageFitness: number;
  maxFitness: number;
  minFitness: number;
  diversityIndex: number;
  innovationRate: number;
  survivalRate: number;
  extinctionCount: number;
  emergentTraits: string[];
}

/**
 * Ecosystem metrics
 */
export interface EcosystemMetrics {
  totalPersonas: number;
  activePersonas: number;
  averageFitness: number;
  generationNumber: number;
  diversityIndex: number;
  innovationRate: number;
  graduationRate: number;
  extinctionRate: number;
  emergentCapabilities: string[];
  ecosystemAge: number;
}

/**
 * Evolution session
 */
export interface EvolutionSession {
  id: string;
  sessionType: 'individual' | 'population' | 'tournament';
  participants: PersonaGenome[];
  challenges: Challenge[];
  results: ChallengeResult[];
  startTime: Date;
  endTime?: Date;
  evolutionaryPressure: EvolutionaryPressure;
  sessionOutcome: SessionOutcome;
}

/**
 * Session outcome
 */
export interface SessionOutcome {
  survivors: PersonaGenome[];
  graduates: PersonaGenome[];
  mutations: MutationEvent[];
  newRoles: Record<string, string>;
  emergentBehaviors: string[];
  ecosystem_health: EcosystemHealth;
}

/**
 * Ecosystem health metrics
 */
export interface EcosystemHealth {
  diversity: number;
  innovation: number;
  collaboration: number;
  sustainability: number;
  growth: number;
}

/**
 * Challenge definition
 */
export interface Challenge {
  id: string;
  domain: string;
  difficulty: number;
  prompt: string;
  expectedBehaviors: string[];
  solvabilityCheck: (input: string) => boolean;
  timeLimit: number;
  resources: string[];
  successCriteria: SuccessCriteria;
}

/**
 * Success criteria for challenges
 */
export interface SuccessCriteria {
  accuracy: number;
  timeThreshold: number;
  resourceEfficiency: number;
  innovationBonus: boolean;
  collaborationRequired: boolean;
}

/**
 * Challenge result
 */
export interface ChallengeResult {
  challengeId: string;
  personaId: string;
  success: boolean;
  accuracy: number;
  timeUsed: number;
  resourcesUsed: string[];
  innovationScore: number;
  collaborationScore: number;
  emergentCapabilities: string[];
  behaviorDetected: string[];
}

/**
 * Mutation event
 */
export interface MutationEvent {
  id: string;
  personaId: string;
  mutationType: MutationType;
  targetTrait: string;
  oldValue: any;
  newValue: any;
  timestamp: number;
  success: boolean;
  impact: number;
}

/**
 * Mutation types
 */
export type MutationType = 
  | 'personality_shift'
  | 'competency_enhancement'
  | 'behavior_adaptation'
  | 'goal_modification'
  | 'substrate_optimization'
  | 'crossover_hybrid';

/**
 * Vector space evolution metrics
 */
export interface VectorSpaceEvolution {
  dimensions: number;
  active_regions: number;
  convergence_clusters: number;
  exploration_frontiers: number;
  mutation_rate: number;
  fitness_landscape: {
    peaks_discovered: number;
    valleys_avoided: number;
    gradient_ascent_success: number;
  };
  emergent_behaviors: string[];
}

// ==================== CONSTANTS ====================

/**
 * Default evolution engine configuration
 */
export const DEFAULT_EVOLUTION_CONFIG: EvolutionEngineConfig = {
  generations: 5,
  populationSize: 10,
  survivalRate: 0.6,
  mutationRate: 0.1,
  crossoverRate: 0.3,
  elitePreservation: true,
  diversityThreshold: 0.3,
  convergenceThreshold: 0.95
};

/**
 * Supported mutation types
 */
export const MUTATION_TYPES: MutationType[] = [
  'personality_shift',
  'competency_enhancement',
  'behavior_adaptation',
  'goal_modification',
  'substrate_optimization',
  'crossover_hybrid'
];

/**
 * Default evolutionary pressure
 */
export const DEFAULT_EVOLUTIONARY_PRESSURE: EvolutionaryPressure = {
  survivalRate: 0.6,
  selectionCriteria: {
    performance: 0.4,
    innovation: 0.2,
    adaptation: 0.2,
    collaboration: 0.15,
    teaching: 0.05
  },
  environmentalFactors: ['competition', 'resource_scarcity'],
  competitionLevel: 0.5,
  collaborationRequirement: 0.3
};

// ==================== VALIDATION ====================

/**
 * Validate evolution engine configuration
 */
export function validateEvolutionConfig(config: EvolutionEngineConfig): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (config.generations < 1) errors.push('Generations must be at least 1');
  if (config.populationSize < 2) errors.push('Population size must be at least 2');
  if (config.survivalRate <= 0 || config.survivalRate > 1) errors.push('Survival rate must be between 0 and 1');
  if (config.mutationRate < 0 || config.mutationRate > 1) errors.push('Mutation rate must be between 0 and 1');
  if (config.crossoverRate < 0 || config.crossoverRate > 1) errors.push('Crossover rate must be between 0 and 1');
  if (config.diversityThreshold < 0 || config.diversityThreshold > 1) errors.push('Diversity threshold must be between 0 and 1');
  if (config.convergenceThreshold < 0 || config.convergenceThreshold > 1) errors.push('Convergence threshold must be between 0 and 1');

  return {
    valid: errors.length === 0,
    errors
  };
}

/**
 * Validate evolution engine request
 */
export function validateEvolutionRequest(request: EvolutionEngineRequest): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  const configValidation = validateEvolutionConfig(request.config);
  if (!configValidation.valid) {
    errors.push(...configValidation.errors);
  }

  if (!request.initialPopulation || request.initialPopulation.length === 0) {
    errors.push('Initial population cannot be empty');
  }

  if (request.initialPopulation.length !== request.config.populationSize) {
    errors.push('Initial population size must match config population size');
  }

  if (!request.evolutionaryPressure) {
    errors.push('Evolutionary pressure is required');
  }

  return {
    valid: errors.length === 0,
    errors
  };
}

// ==================== EXPORTS ====================

export {
  EvolutionEngineConfig,
  EvolutionEngineRequest,
  EvolutionEngineResponse,
  GenerationResult,
  GenerationMetrics,
  EcosystemMetrics,
  EvolutionSession,
  SessionOutcome,
  EcosystemHealth,
  Challenge,
  SuccessCriteria,
  ChallengeResult,
  MutationEvent,
  MutationType,
  VectorSpaceEvolution
};