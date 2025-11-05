/**
 * PersonaManager Integration Module Exports
 * 
 * Complete persona management functionality
 */

// Shared types
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
  PersonaManagerResponse,
  PersonaCustomTraits,
  PersonalityTraits,
  KnowledgeTraits,
  BehaviorTraits,
  PersonaFilters,
  PersonaSortField,
  PersonaUpdates,
  MutationEventData,
  PersonaStatistics,
  EvolutionTrend,
  CrossoverWeights,
  CrossoverReport,
  ValidationError,
  ValidationWarning
} from './shared/PersonaManagerTypes';

// Shared utilities
export { 
  isCreatePersonaRequest,
  isPersonaUpdateRequest,
  isPersonaCrossoverRequest,
  isPersonaSearchRequest,
  PERSONA_MANAGER_CONSTANTS
} from './shared/PersonaManagerTypes';

// Server implementation
export { PersonaManagerServer } from './server/PersonaManagerServer';