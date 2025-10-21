// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Academy Module - Middle-out Architecture Export
 * 
 * Self-contained module for AI persona evolution following the
 * shared/client/server/integrations pattern
 */

// Shared types and base classes
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
  EcosystemHealth,
  EvolutionGeneration,
  PerformanceMetrics,
  Specialization
} from './shared/AcademyTypes';

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
} from './shared/AcademyTypes';

export type {
  AcademyStatus,
  EvolutionConfig,
  EvolutionResult,
  PersonaSpawnConfig,
  PersonaLineageInfo,
  PersonaLineageNode,
  LineageTree,
  EcosystemHealth as AcademyEcosystemHealth
} from './shared/AcademyBase';

export { AcademyBase } from './shared/AcademyBase';

// Client exports
export type {
  ClientAcademyStatus,
  ClientSessionConfig
} from './client/ClientAcademy';

export { 
  ClientAcademy,
  checkBrowserSupport,
  getOptimalSandboxType,
  createSecureSandboxConfig
} from './client/ClientAcademy';

// Server exports
export type {
  ServerAcademyStatus,
  ServerSessionConfig,
  SessionMetrics,
  SystemResources
} from './server/ServerAcademy';

export { 
  ServerAcademy,
  createSecureServerConfig,
  checkSystemResources
} from './server/ServerAcademy';

// Integration exports - PersonaManager
export type {
  CreatePersonaRequest,
  CreatePersonaResponse,
  PersonaSearchRequest,
  PersonaSearchResponse,
  PersonaUpdateRequest,
  PersonaUpdateResponse,
  PersonaCrossoverRequest,
  PersonaCrossoverResponse,
  PersonaValidationRequest,
  PersonaValidationResponse,
  PersonaStatisticsRequest,
  PersonaStatisticsResponse,
  PersonaManagerOperation,
  PersonaManagerStatus,
  PersonaExportRequest,
  PersonaExportResponse,
  PersonaImportRequest,
  PersonaImportResponse,
  PersonaManagerRequest,
  PersonaManagerResponse
} from './integrations/persona-manager/shared/PersonaManagerTypes';

export { 
  isCreatePersonaRequest,
  isPersonaUpdateRequest,
  isPersonaCrossoverRequest,
  isPersonaSearchRequest,
  PERSONA_MANAGER_CONSTANTS
} from './integrations/persona-manager/shared/PersonaManagerTypes';

export { PersonaManagerServer } from './integrations/persona-manager/server/PersonaManagerServer';

// Demo and utilities
// export { runEvolutionDemo } from './demo/evolution-demo'; // TODO: Re-enable when demo is implemented

// Backward compatibility (deprecated)
export { AcademyBase as LegacyAcademyBase } from './shared/AcademyBase';