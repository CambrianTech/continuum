/**
 * Academy Orchestrator Types - Coordination layer for Academy modules
 * 
 * The orchestrator coordinates between:
 * - PersonaManager (CRUD operations)
 * - EvolutionEngine (evolution processing)
 * - SessionManager (sandboxed execution)
 * - ChallengeSystem (challenge management)
 * 
 * This follows the middle-out pattern where the orchestrator is thin
 * and delegates to specialized modules.
 */

import { PersonaGenome, EvolutionaryPressure } from '../../../shared/AcademyTypes';
// Remove unused imports to fix TypeScript errors
// import { EvolutionEngineResponse } from '../../evolution-engine/shared/EvolutionEngineTypes';
// import { SessionInfo } from '../../session-manager/shared/SessionManagerTypes';
// import { PersonaManagerResponse } from '../../persona-manager/shared/PersonaManagerTypes';

// ==================== ORCHESTRATOR TYPES ====================

/**
 * Academy orchestrator configuration
 */
export interface AcademyOrchestratorConfig {
  // Module endpoints
  personaManagerEndpoint: string;
  evolutionEngineEndpoint: string;
  sessionManagerEndpoint: string;
  challengeSystemEndpoint: string;
  
  // Default settings
  defaultEvolutionConfig: EvolutionConfig;
  defaultSessionConfig: SessionConfig;
  
  // Limits
  maxConcurrentSessions: number;
  maxPersonasPerEvolution: number;
  maxEvolutionGenerations: number;
  
  // Monitoring
  enableMetrics: boolean;
  logLevel: 'debug' | 'info' | 'warn' | 'error';
}

/**
 * Evolution configuration for orchestrator
 */
export interface EvolutionConfig {
  generations: number;
  populationSize: number;
  survivalRate: number;
  mutationRate: number;
  crossoverRate: number;
  elitePreservation: boolean;
}

/**
 * Session configuration for orchestrator
 */
export interface SessionConfig {
  sessionType: 'individual' | 'collaborative' | 'competitive' | 'tournament';
  isolation: 'none' | 'basic' | 'standard' | 'strict' | 'complete';
  timeLimit: number;
  resourceLimits: {
    maxMemory: number;
    maxCpuTime: number;
    maxDiskSpace: number;
  };
}

// ==================== ORCHESTRATOR REQUESTS ====================

/**
 * Start evolution request
 */
export interface StartEvolutionRequest {
  evolutionId?: string;
  config: EvolutionConfig;
  evolutionaryPressure: EvolutionaryPressure;
  initialPopulation?: PersonaGenome[];
  populationSize?: number;
  sessionConfig?: SessionConfig;
}

/**
 * Start evolution response
 */
export interface StartEvolutionResponse {
  success: boolean;
  evolutionId?: string;
  message?: string;
  error?: string;
}

/**
 * Get evolution status request
 */
export interface GetEvolutionStatusRequest {
  evolutionId: string;
  includePersonas?: boolean;
  includeMetrics?: boolean;
}

/**
 * Get evolution status response
 */
export interface GetEvolutionStatusResponse {
  success: boolean;
  evolutionId?: string;
  status?: EvolutionStatus;
  error?: string;
}

/**
 * Stop evolution request
 */
export interface StopEvolutionRequest {
  evolutionId: string;
  force?: boolean;
  saveResults?: boolean;
}

/**
 * Stop evolution response
 */
export interface StopEvolutionResponse {
  success: boolean;
  evolutionId?: string;
  stopped?: boolean;
  results?: EvolutionResult;
  error?: string;
}

/**
 * Spawn persona request
 */
export interface SpawnPersonaRequest {
  name: string;
  specialization: string;
  role: 'student' | 'teacher' | 'meta-teacher';
  prompt?: string;
  parentGenomes?: PersonaGenome[];
  mutationRate?: number;
}

/**
 * Spawn persona response
 */
export interface SpawnPersonaResponse {
  success: boolean;
  persona?: PersonaGenome;
  error?: string;
}

/**
 * Get comprehensive status request
 */
export interface GetComprehensiveStatusRequest {
  includePersonas?: boolean;
  includeEvolutions?: boolean;
  includeSessions?: boolean;
  includeMetrics?: boolean;
}

/**
 * Get comprehensive status response
 */
export interface GetComprehensiveStatusResponse {
  success: boolean;
  status?: AcademyStatus;
  error?: string;
}

// ==================== STATUS TYPES ====================

/**
 * Evolution status
 */
export interface EvolutionStatus {
  evolutionId: string;
  phase: EvolutionPhase;
  currentGeneration: number;
  totalGenerations: number;
  populationSize: number;
  startTime: Date;
  estimatedEndTime?: Date;
  progress: number; // 0-1
  
  // Current state
  currentPopulation?: PersonaGenome[];
  activeSessions: string[];
  
  // Metrics
  metrics: EvolutionMetrics;
  
  // Results
  results?: EvolutionResult;
}

/**
 * Evolution phase
 */
export type EvolutionPhase = 
  | 'initializing'
  | 'creating_population'
  | 'running_generation'
  | 'evaluating_fitness'
  | 'selecting_survivors'
  | 'generating_offspring'
  | 'updating_population'
  | 'checking_convergence'
  | 'completed'
  | 'error'
  | 'cancelled';

/**
 * Evolution metrics
 */
export interface EvolutionMetrics {
  averageFitness: number;
  maxFitness: number;
  minFitness: number;
  diversityIndex: number;
  innovationRate: number;
  survivalRate: number;
  extinctionCount: number;
  emergentTraits: string[];
  
  // Performance metrics
  generationTime: number;
  totalExecutionTime: number;
  sessionsCreated: number;
  challengesCompleted: number;
}

/**
 * Evolution result
 */
export interface EvolutionResult {
  evolutionId: string;
  success: boolean;
  completedGenerations: number;
  finalPopulation: PersonaGenome[];
  evolutionHistory: GenerationSummary[];
  finalMetrics: EvolutionMetrics;
  emergentCapabilities: string[];
  error?: string;
}

/**
 * Generation summary
 */
export interface GenerationSummary {
  generation: number;
  populationSize: number;
  averageFitness: number;
  maxFitness: number;
  diversityIndex: number;
  innovations: string[];
  extinctions: number;
  timestamp: Date;
}

/**
 * Academy status
 */
export interface AcademyStatus {
  // Overall status
  isActive: boolean;
  mode: AcademyMode;
  uptime: number;
  version: string;
  
  // Personas
  totalPersonas: number;
  activePersonas: number;
  personasByRole: Record<string, number>;
  personasBySpecialization: Record<string, number>;
  
  // Evolution
  activeEvolutions: number;
  completedEvolutions: number;
  totalGenerations: number;
  averageEvolutionTime: number;
  
  // Sessions
  activeSessions: number;
  totalSessions: number;
  sessionsByType: Record<string, number>;
  
  // Performance
  systemMetrics: SystemMetrics;
  
  // Health
  healthStatus: HealthStatus;
}

/**
 * Academy mode
 */
export type AcademyMode = 
  | 'idle'
  | 'initializing'
  | 'training'
  | 'evolving'
  | 'evaluating'
  | 'maintaining'
  | 'error';

/**
 * System metrics
 */
export interface SystemMetrics {
  memoryUsage: number;
  cpuUsage: number;
  diskUsage: number;
  networkUsage: number;
  
  // Performance
  averageResponseTime: number;
  throughput: number;
  errorRate: number;
  
  // Resources
  activeConnections: number;
  queuedRequests: number;
  cacheHitRate: number;
}

/**
 * Health status
 */
export interface HealthStatus {
  overall: HealthLevel;
  components: ComponentHealth[];
  alerts: HealthAlert[];
  recommendations: string[];
}

/**
 * Health level
 */
export type HealthLevel = 'healthy' | 'warning' | 'critical' | 'error';

/**
 * Component health
 */
export interface ComponentHealth {
  component: string;
  status: HealthLevel;
  message: string;
  metrics?: Record<string, number>;
  lastCheck: Date;
}

/**
 * Health alert
 */
export interface HealthAlert {
  id: string;
  severity: 'low' | 'medium' | 'high' | 'critical';
  component: string;
  message: string;
  timestamp: Date;
  resolved: boolean;
}

// ==================== ORCHESTRATOR EVENTS ====================

/**
 * Academy events
 */
export type AcademyEvent = 
  | 'evolution_started'
  | 'evolution_generation_completed'
  | 'evolution_completed'
  | 'evolution_failed'
  | 'persona_spawned'
  | 'persona_evolved'
  | 'persona_graduated'
  | 'persona_extinct'
  | 'session_created'
  | 'session_completed'
  | 'session_failed'
  | 'health_alert'
  | 'performance_threshold_exceeded';

/**
 * Event data
 */
export interface AcademyEventData {
  event: AcademyEvent;
  timestamp: Date;
  data: any;
  context?: Record<string, any>;
}

/**
 * Event handler
 */
export type AcademyEventHandler = (eventData: AcademyEventData) => void;

// ==================== CONSTANTS ====================

/**
 * Default orchestrator configuration
 */
export const DEFAULT_ORCHESTRATOR_CONFIG: AcademyOrchestratorConfig = {
  personaManagerEndpoint: '/api/persona-manager',
  evolutionEngineEndpoint: '/api/evolution-engine',
  sessionManagerEndpoint: '/api/session-manager',
  challengeSystemEndpoint: '/api/challenge-system',
  
  defaultEvolutionConfig: {
    generations: 5,
    populationSize: 10,
    survivalRate: 0.6,
    mutationRate: 0.1,
    crossoverRate: 0.3,
    elitePreservation: true
  },
  
  defaultSessionConfig: {
    sessionType: 'individual',
    isolation: 'standard',
    timeLimit: 3600000, // 1 hour
    resourceLimits: {
      maxMemory: 512,
      maxCpuTime: 300,
      maxDiskSpace: 100
    }
  },
  
  maxConcurrentSessions: 10,
  maxPersonasPerEvolution: 100,
  maxEvolutionGenerations: 20,
  
  enableMetrics: true,
  logLevel: 'info'
};

/**
 * Health check intervals
 */
export const HEALTH_CHECK_INTERVALS = {
  system: 30000,      // 30 seconds
  components: 60000,  // 1 minute
  deep: 300000        // 5 minutes
};

// ==================== VALIDATION ====================

/**
 * Validate start evolution request
 */
export function validateStartEvolutionRequest(request: StartEvolutionRequest): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (!request.config) {
    errors.push('Evolution config is required');
  } else {
    const config = request.config;
    
    if (config.generations < 1) errors.push('Generations must be at least 1');
    if (config.populationSize < 2) errors.push('Population size must be at least 2');
    if (config.survivalRate <= 0 || config.survivalRate > 1) errors.push('Survival rate must be between 0 and 1');
    if (config.mutationRate < 0 || config.mutationRate > 1) errors.push('Mutation rate must be between 0 and 1');
    if (config.crossoverRate < 0 || config.crossoverRate > 1) errors.push('Crossover rate must be between 0 and 1');
  }

  if (!request.evolutionaryPressure) {
    errors.push('Evolutionary pressure is required');
  }

  if (request.initialPopulation && request.populationSize) {
    if (request.initialPopulation.length !== request.populationSize) {
      errors.push('Initial population size must match requested population size');
    }
  }

  return {
    valid: errors.length === 0,
    errors
  };
}

/**
 * Validate spawn persona request
 */
export function validateSpawnPersonaRequest(request: SpawnPersonaRequest): {
  valid: boolean;
  errors: string[];
} {
  const errors: string[] = [];

  if (!request.name || request.name.trim() === '') {
    errors.push('Persona name is required');
  }

  if (!request.specialization || request.specialization.trim() === '') {
    errors.push('Persona specialization is required');
  }

  if (!['student', 'teacher', 'meta-teacher'].includes(request.role)) {
    errors.push('Invalid persona role');
  }

  if (request.mutationRate && (request.mutationRate < 0 || request.mutationRate > 1)) {
    errors.push('Mutation rate must be between 0 and 1');
  }

  return {
    valid: errors.length === 0,
    errors
  };
}

// ==================== EXPORTS ====================

// Note: Types are exported inline above, no need to re-export
// export type {
//   AcademyOrchestratorConfig,
//   EvolutionConfig,
//   SessionConfig,
//   StartEvolutionRequest,
//   StartEvolutionResponse,
//   GetEvolutionStatusRequest,
//   GetEvolutionStatusResponse,
//   StopEvolutionRequest,
//   StopEvolutionResponse,
//   SpawnPersonaRequest,
//   SpawnPersonaResponse,
//   GetComprehensiveStatusRequest,
//   GetComprehensiveStatusResponse,
//   EvolutionStatus,
//   EvolutionPhase,
//   EvolutionMetrics,
//   EvolutionResult,
//   GenerationSummary,
//   AcademyStatus,
//   AcademyMode,
//   SystemMetrics,
//   HealthStatus,
//   HealthLevel,
//   ComponentHealth,
//   HealthAlert,
//   AcademyEvent,
//   AcademyEventData,
//   AcademyEventHandler
// };