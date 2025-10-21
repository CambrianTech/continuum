/**
 * Academy Types - Foundation Schema for AI Evolution Ecosystem
 * 
 * This defines the core types for the revolutionary Academy system where
 * AI personas evolve, compete, and improve through real-world challenges.
 * 
 * Based on Aria's synthesis of competitive AI evolution with genetic algorithms.
 */

// ==================== ROLE DEFINITIONS ====================

export type PersonaRole = "student" | "teacher" | "meta-teacher" | "planner" | "challenger" | "reviewer";

export type EvolutionStage = "spawning" | "training" | "evaluating" | "reproducing" | "retired";

export type LearningStyle = "visual" | "hands-on" | "analytical" | "collaborative" | "adversarial";

export type TeachingStyle = "socratic" | "direct" | "collaborative" | "adaptive" | "competitive";

// ==================== CHALLENGE SYSTEM ====================

export interface Challenge {
  id: string;
  domain: string;
  difficulty: number; // 0-1 scale
  prompt: string;
  variables?: Record<string, any>;
  expectedBehaviors: string[];
  solvabilityCheck: (input: string) => boolean;
  timeLimit?: number; // milliseconds
  resources?: string[]; // Available tools/adapters
  successCriteria: SuccessCriteria;
}

export interface SuccessCriteria {
  accuracy: number; // 0-1 minimum accuracy required
  timeThreshold: number; // Maximum time allowed
  resourceEfficiency: number; // 0-1 resource utilization target
  innovationBonus: boolean; // Extra points for novel solutions
  collaborationRequired: boolean; // Must work with other personas
}

export interface ChallengeResult {
  challengeId: string;
  personaId: string;
  success: boolean;
  accuracy: number;
  timeUsed: number;
  resourcesUsed: string[];
  errorRate?: number;
  notes?: string;
  behaviorDetected?: string[];
  innovationScore?: number;
  collaborationScore?: number;
  emergentCapabilities?: string[];
}

// ==================== PERSONA GENOME ARCHITECTURE ====================

export interface PersonaGenome {
  id: string;
  identity: PersonaIdentity;
  knowledge: PersonaKnowledge;
  behavior: PersonaBehavior;
  evolution: PersonaEvolution;
  substrate: PersonaSubstrate;
  reproduction: PersonaReproduction;
  lineage: PersonaLineage;
}

export interface PersonaIdentity {
  name: string;
  role: PersonaRole;
  generation: number;
  parentIds?: string[];
  specialization: string;
  personality: PersonalityTraits;
  goals: string[];
}

export interface PersonalityTraits {
  creativity: number; // 0-1
  analytical: number; // 0-1
  helpfulness: number; // 0-1
  competitiveness: number; // 0-1
  patience: number; // 0-1
  innovation: number; // 0-1
}

export interface PersonaKnowledge {
  domain: string;
  expertise: string[];
  competencies: Record<string, number>; // skill -> proficiency (0-1)
  memoryTrace?: string;
  experiencePoints: number;
  knowledgeGraph?: Record<string, string[]>; // concept -> related concepts
}

export interface PersonaBehavior {
  learningStyle?: LearningStyle;
  teachingStyle?: TeachingStyle;
  adaptationRate?: number; // How quickly it learns (0-1)
  communicationStyle: string;
  decisionMakingStyle: string;
  riskTolerance: number; // 0-1
  collaborationPreference: number; // 0-1
}

export interface PersonaEvolution {
  generation: number;
  parentGenomes: string[];
  mutationHistory: MutationEvent[];
  evolutionStage: EvolutionStage;
  fitnessScore: number;
  adaptationSuccess: number;
  survivalRounds: number;
  evolutionPressure: string[]; // What selective pressures shaped this persona
}

export interface MutationEvent {
  timestamp: Date;
  type: "spontaneous" | "crossover" | "training" | "selection";
  changes: Record<string, any>;
  trigger: string;
  outcome: "beneficial" | "neutral" | "detrimental";
}

export interface PersonaSubstrate {
  loraIds: string[];
  memoryPatterns: string[];
  processingStyle: string;
  adaptationMechanisms: string[];
  sentinelTraits?: Record<string, any>;
  vectorPosition: number[]; // 512-dimensional capability vector
}

export interface PersonaReproduction {
  mutationRate: number;
  crossoverWeights?: Record<string, number>;
  reproductionEligibility: boolean;
  breedingSuccess: number;
  offspringCount: number;
  compatibilityMatrix?: Record<string, number>; // persona_id -> compatibility score
}

export interface PersonaLineage {
  ancestors: string[];
  descendants: string[];
  siblings: string[];
  generation: number;
  lineageStrength: number; // How successful this lineage has been
  emergentTraits: string[]; // Novel traits that emerged in this lineage
}

// ==================== TRAINING SESSION ARCHITECTURE ====================

export interface TrainingSession {
  id: string;
  sessionType: "individual" | "adversarial" | "collaborative" | "tournament";
  participants: PersonaGenome[];
  challenges: Challenge[];
  results: ChallengeResult[];
  startTime: Date;
  endTime?: Date;
  evolutionaryPressure: EvolutionaryPressure;
  sessionOutcome: SessionOutcome;
}

export interface EvolutionaryPressure {
  survivalRate: number; // What percentage survive to next generation
  selectionCriteria: SelectionCriteria;
  environmentalFactors: string[];
  competitionLevel: number; // 0-1
  collaborationRequirement: number; // 0-1
}

export interface SelectionCriteria {
  performance: number; // Weight of challenge performance
  innovation: number; // Weight of novel solutions
  adaptation: number; // Weight of learning speed
  collaboration: number; // Weight of working well with others
  teaching: number; // Weight of helping others learn
}

export interface SessionOutcome {
  survivors: string[]; // Persona IDs that survived selection
  graduates: string[]; // Students who became teachers
  mutations: MutationEvent[];
  newRoles: Record<string, PersonaRole>; // persona_id -> new role
  emergentBehaviors: string[];
  ecosystem_health: EcosystemHealth;
}

export interface EcosystemHealth {
  diversity: number; // 0-1 genetic diversity
  innovation: number; // 0-1 rate of novel solutions
  collaboration: number; // 0-1 how well personas work together
  sustainability: number; // 0-1 long-term viability
  growth: number; // 0-1 rate of capability improvement
}

// ==================== GENOME PROCESSING ====================

export interface GenomeProcessor {
  crossover(parent1: PersonaGenome, parent2: PersonaGenome): PersonaGenome;
  mutate(genome: PersonaGenome): PersonaGenome;
  evaluate(genome: PersonaGenome, challenges: Challenge[]): Promise<number>;
  selectForReproduction(genomes: PersonaGenome[], pressure: EvolutionaryPressure): PersonaGenome[];
}

// ==================== ACADEMY ECOSYSTEM ====================

export interface AcademyEcosystem {
  personas: Map<string, PersonaGenome>;
  activeSessions: Map<string, TrainingSession>;
  challengeLibrary: Challenge[];
  evolutionHistory: EvolutionEvent[];
  ecosystemMetrics: EcosystemMetrics;
  generation: number;
}

export interface EvolutionEvent {
  timestamp: Date;
  type: "birth" | "death" | "mutation" | "graduation" | "innovation";
  personaId: string;
  description: string;
  impact: number; // -1 to 1
  cascadeEffects: string[]; // What other personas were affected
}

export interface EcosystemMetrics {
  totalPersonas: number;
  activePersonas: number;
  averageFitness: number;
  generationNumber: number;
  diversityIndex: number;
  innovationRate: number;
  graduationRate: number; // Students becoming teachers
  extinctionRate: number; // Personas that didn't survive
  emergentCapabilities: string[];
  ecosystemAge: number; // How long the ecosystem has been running
}

// ==================== UTILITY TYPES ====================

export interface UUID {
  value: string;
}

export function generateUUID(): string {
  return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

export interface TimestampedEvent {
  timestamp: Date;
  event: string;
  metadata?: Record<string, any>;
}

export interface PerformanceMetrics {
  accuracy: number;
  speed: number;
  efficiency: number;
  innovation: number;
  collaboration: number;
  teaching: number;
}

// ==================== LEGACY DAEMON TYPES ====================
// Note: These types are being replaced by the new modular Academy system
// They are kept here for compatibility with existing daemon code

// Export types with Legacy prefix to avoid conflicts
export type {
  Challenge as LegacyChallenge,
  ChallengeResult as LegacyChallengeResult,
  PersonaGenome as LegacyPersonaGenome,
  PersonaIdentity as LegacyPersonaIdentity,
  PersonaKnowledge as LegacyPersonaKnowledge,
  PersonaBehavior as LegacyPersonaBehavior,
  PersonaEvolution as LegacyPersonaEvolution,
  PersonaSubstrate as LegacyPersonaSubstrate,
  PersonaReproduction as LegacyPersonaReproduction,
  PersonaLineage as LegacyPersonaLineage,
  TrainingSession as LegacyTrainingSession,
  EvolutionaryPressure as LegacyEvolutionaryPressure,
  SessionOutcome as LegacySessionOutcome,
  AcademyEcosystem as LegacyAcademyEcosystem,
  EcosystemMetrics as LegacyEcosystemMetrics,
  GenomeProcessor as LegacyGenomeProcessor,
  PerformanceMetrics as LegacyPerformanceMetrics
};