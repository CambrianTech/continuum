/**
 * Academy Types - Shared type definitions for Academy system
 * 
 * This file contains all type definitions shared across client/server/integrations
 * in the Academy module, following the middle-out modular pattern.
 */

import { PersonaBase } from './PersonaBase';

// ==================== PERSONA GENOME TYPES ====================

/**
 * PersonaGenome - Enhanced persona with evolutionary capabilities
 * 
 * This extends PersonaBase with genomic features for the Academy evolution system.
 * Key principle: PersonaBase is the foundation, PersonaGenome adds evolution.
 */
export interface PersonaGenome extends PersonaBase {
  // Inherited from PersonaBase:
  // - id: string
  // - name: string  
  // - prompt: string
  // - description?: string
  // - created: number
  // - rag?: PersonaRAG
  // - metadata?: PersonaMetadata

  // Enhanced identity with genomic features
  identity: PersonaIdentity;
  
  // Academy-specific enhancements
  knowledge: PersonaKnowledge;
  behavior: PersonaBehavior;
  evolution: PersonaEvolution;
  substrate: PersonaSubstrate;
  reproduction: PersonaReproduction;
  lineage: PersonaLineage;
}

export interface PersonaIdentity {
  // name is inherited from PersonaBase
  role: PersonaRole;
  generation: number;
  parentIds?: string[];
  specialization: string;
  personality: PersonalityTraits;
  goals: string[];
}

export interface PersonaKnowledge {
  domain: string;
  expertise: string[];
  competencies: Record<string, number>;
  experiencePoints: number;
  knowledgeGraph?: Record<string, string[]>;
}

export interface PersonaBehavior {
  learningStyle?: string;
  teachingStyle?: string;
  adaptationRate?: number;
  communicationStyle: string;
  decisionMakingStyle: string;
  riskTolerance: number;
  collaborationPreference: number;
}

export interface PersonaEvolution {
  generation: number;
  parentGenomes: string[];
  mutationHistory: MutationEvent[];
  evolutionStage: EvolutionStage;
  fitnessScore: number;
  adaptationSuccess: number;
  survivalRounds: number;
  evolutionPressure: string[];
}

export interface PersonaSubstrate {
  loraIds: string[];
  memoryPatterns: string[];
  processingStyle: string;
  adaptationMechanisms: string[];
  sentinelTraits?: Record<string, any>;
  vectorPosition: number[];
}

export interface PersonaReproduction {
  mutationRate: number;
  crossoverWeights?: Record<string, number>;
  reproductionEligibility: boolean;
  breedingSuccess: number;
  offspringCount: number;
  compatibilityMatrix?: Record<string, number>;
}

export interface PersonaLineage {
  ancestors: string[];
  descendants: string[];
  siblings: string[];
  generation: number;
  lineageStrength: number;
  emergentTraits: string[];
}

// ==================== PERSONA ATTRIBUTE TYPES ====================

export type PersonaRole = 'student' | 'teacher' | 'meta-teacher';

export type EvolutionStage = 'spawning' | 'learning' | 'competing' | 'reproducing' | 'extinct';

export interface PersonalityTraits {
  creativity: number;
  analytical: number;
  helpfulness: number;
  competitiveness: number;
  patience: number;
  innovation: number;
}

export interface MutationEvent {
  timestamp: Date;
  type: 'spontaneous' | 'induced' | 'crossover';
  changes: Record<string, any>;
  trigger: string;
  outcome: 'beneficial' | 'neutral' | 'harmful';
}

// ==================== EVOLUTION SYSTEM TYPES ====================

export interface EvolutionaryPressure {
  survivalRate: number;
  selectionCriteria: SelectionCriteria;
  environmentalFactors: string[];
  competitionLevel: number;
  collaborationRequirement: number;
}

export interface SelectionCriteria {
  performance: number;
  innovation: number;
  adaptation: number;
  collaboration: number;
  teaching: number;
}

export interface Challenge {
  id: string;
  domain: string;
  difficulty: number;
  prompt: string;
  expectedBehaviors: string[];
  solvabilityCheck: (input: string) => boolean;
  timeLimit: number;
  resources: string[];
  successCriteria: ChallengeSuccessCriteria;
}

export interface ChallengeSuccessCriteria {
  accuracy: number;
  timeThreshold: number;
  resourceEfficiency: number;
  innovationBonus: boolean;
  collaborationRequired: boolean;
}

export interface ChallengeResult {
  challengeId: string;
  personaId: string;
  success: boolean;
  accuracy: number;
  timeUsed: number;
  resourcesUsed: string[];
  innovationScore?: number;
  collaborationScore?: number;
  emergentCapabilities: string[];
  behaviorDetected: string[];
}

// ==================== TRAINING SESSION TYPES ====================

export interface TrainingSession {
  id: string;
  sessionType: 'individual' | 'collaborative' | 'competitive';
  participants: PersonaGenome[];
  challenges: Challenge[];
  results: ChallengeResult[];
  startTime: Date;
  endTime?: Date;
  evolutionaryPressure: EvolutionaryPressure;
  sessionOutcome: SessionOutcome;
}

export interface SessionOutcome {
  survivors: PersonaGenome[];
  graduates: PersonaGenome[];
  mutations: MutationEvent[];
  newRoles: Record<string, PersonaRole>;
  emergentBehaviors: string[];
  ecosystem_health: EcosystemHealth;
}

// ==================== ECOSYSTEM TYPES ====================

export interface AcademyEcosystem {
  personas: Map<string, PersonaGenome>;
  activeSessions: Map<string, TrainingSession>;
  challengeLibrary: Challenge[];
  evolutionHistory: EvolutionGeneration[];
  ecosystemMetrics: EcosystemMetrics;
}

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

export interface EcosystemHealth {
  diversity: number;
  innovation: number;
  collaboration: number;
  sustainability: number;
  growth: number;
}

export interface EvolutionGeneration {
  generation: number;
  timestamp: Date;
  populationSize: number;
  survivors: number;
  offspring: number;
  mutations: number;
  graduations: number;
  extinctions: number;
  averageFitness: number;
  topPerformers: string[];
}

// ==================== PERFORMANCE METRICS ====================

export interface PerformanceMetrics {
  accuracy: number;
  speed: number;
  efficiency: number;
  innovation: number;
  collaboration: number;
  teaching: number;
}

// ==================== UTILITY FUNCTIONS ====================

/**
 * Generate unique identifier
 */
export function generateUUID(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
    const r = Math.random() * 16 | 0;
    const v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

/**
 * Validate persona genome structure
 */
export function validatePersonaGenome(genome: PersonaGenome): boolean {
  return !!(
    genome.id &&
    genome.identity &&
    genome.knowledge &&
    genome.behavior &&
    genome.evolution &&
    genome.substrate &&
    genome.reproduction &&
    genome.lineage
  );
}

/**
 * Calculate fitness score from performance metrics
 */
export function calculateFitnessScore(metrics: PerformanceMetrics): number {
  return (
    metrics.accuracy * 0.3 +
    metrics.speed * 0.2 +
    metrics.efficiency * 0.2 +
    metrics.innovation * 0.15 +
    metrics.collaboration * 0.1 +
    metrics.teaching * 0.05
  );
}

/**
 * Create default evolutionary pressure
 */
export function createDefaultEvolutionaryPressure(): EvolutionaryPressure {
  return {
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
}

/**
 * Create default personality traits
 */
export function createDefaultPersonalityTraits(): PersonalityTraits {
  return {
    creativity: 0.5,
    analytical: 0.5,
    helpfulness: 0.8,
    competitiveness: 0.4,
    patience: 0.6,
    innovation: 0.5
  };
}

// ==================== TYPE GUARDS ====================

export function isPersonaGenome(obj: any): obj is PersonaGenome {
  return obj && typeof obj.id === 'string' && obj.identity && obj.knowledge;
}

export function isTrainingSession(obj: any): obj is TrainingSession {
  return obj && typeof obj.id === 'string' && obj.sessionType && obj.participants;
}

export function isChallenge(obj: any): obj is Challenge {
  return obj && typeof obj.id === 'string' && obj.domain && obj.prompt;
}

export function isChallengeResult(obj: any): obj is ChallengeResult {
  return obj && typeof obj.challengeId === 'string' && typeof obj.personaId === 'string';
}

// ==================== CONSTANTS ====================

export const ACADEMY_CONSTANTS = {
  MAX_POPULATION_SIZE: 1000,
  MIN_POPULATION_SIZE: 2,
  MAX_GENERATIONS: 100,
  MIN_GENERATIONS: 1,
  DEFAULT_MUTATION_RATE: 0.1,
  DEFAULT_SURVIVAL_RATE: 0.6,
  VECTOR_DIMENSIONS: 512,
  MAX_LINEAGE_DEPTH: 20,
  FITNESS_THRESHOLD: 0.85,
  GRADUATION_THRESHOLD: 0.9
} as const;

export const SPECIALIZATIONS = [
  'typescript',
  'testing',
  'architecture',
  'ui_design',
  'debugging',
  'optimization',
  'security',
  'performance',
  'devops',
  'documentation'
] as const;

export type Specialization = typeof SPECIALIZATIONS[number];

export const EVOLUTION_STAGES: EvolutionStage[] = [
  'spawning',
  'learning',
  'competing',
  'reproducing',
  'extinct'
] as const;

export const PERSONA_ROLES: PersonaRole[] = [
  'student',
  'teacher',
  'meta-teacher'
] as const;