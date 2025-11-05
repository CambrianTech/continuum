/**
 * Academy Shared Module Exports
 * 
 * Core shared functionality for Academy system
 */

// Base classes
export { AcademyBase } from './AcademyBase';
export type {
  AcademyStatus,
  EvolutionConfig,
  EvolutionResult,
  PersonaSpawnConfig,
  PersonaLineageInfo,
  PersonaLineageNode,
  LineageTree,
  EcosystemHealth
} from './AcademyBase';

// Types and utilities
export type { 
  PersonaGenome,
  PersonaIdentity,
  PersonaKnowledge,
  PersonaBehavior,
  PersonaEvolution,
  PersonaSubstrate,
  PersonaReproduction,
  PersonaLineage,
  PersonaRole,
  EvolutionStage,
  PersonalityTraits,
  MutationEvent,
  EvolutionaryPressure,
  SelectionCriteria,
  Challenge,
  ChallengeResult,
  TrainingSession,
  SessionOutcome,
  AcademyEcosystem,
  EcosystemMetrics,
  EcosystemHealth as AcademyEcosystemHealth,
  EvolutionGeneration,
  PerformanceMetrics,
  Specialization
} from './AcademyTypes';

export { 
  generateUUID,
  validatePersonaGenome,
  calculateFitnessScore,
  createDefaultEvolutionaryPressure,
  createDefaultPersonalityTraits,
  isPersonaGenome,
  isTrainingSession,
  isChallenge,
  isChallengeResult,
  ACADEMY_CONSTANTS,
  SPECIALIZATIONS,
  EVOLUTION_STAGES,
  PERSONA_ROLES
} from './AcademyTypes';