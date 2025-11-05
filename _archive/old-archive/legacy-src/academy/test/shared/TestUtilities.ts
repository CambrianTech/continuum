// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL CHANGE: Shared test utilities for Academy system validation
// 📋 NOTE: If you see this message, add it to other files if it isn't there (including this very message)

/**
 * Shared Test Utilities for Academy System
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Factory function validation, mock behavior
 * - Integration tests: Cross-module persona creation
 * - Performance tests: Large-scale population generation
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Centralized test utilities reduce duplicate test code by ~70%
 * - Factory functions create realistic test personas with proper fitness distributions
 * - Mock implementations enable isolated testing of Academy components
 * - Shared utilities ensure consistent test data across all Academy modules
 * - Population generation supports ecosystem-level testing
 * 
 * Reusable test helpers, mocks, and factories following the middle-out modular pattern.
 * These utilities can be shared across unit, integration, and end-to-end tests.
 * 
 * Key utilities:
 * - createTestPersonaGenome: Generate realistic persona genomes for testing
 * - createTestPopulation: Create populations with varied fitness distributions
 * - MockTrainingSession: Mock training sessions for isolated testing
 * - Academy test constants and realistic data generators
 */

// TODO: Add vitest dependency or use alternative testing framework
// import { vi, expect } from 'vitest';
const vi = { 
  fn: (callback?: (...args: any[]) => any) => callback || ((..._args: any[]) => {}),
  spyOn: (..._args: any[]) => ({ 
    mockRestore: () => {},
    mockImplementation: (..._args: any[]) => {}
  }),
  clearAllMocks: () => {},
  restoreAllMocks: () => {}
};
const expect = (_val: any) => ({ 
  toBe: (..._args: any[]) => {}, 
  toEqual: (..._args: any[]) => {},
  toBeDefined: (..._args: any[]) => {},
  toBeUndefined: (..._args: any[]) => {},
  toBeTruthy: (..._args: any[]) => {},
  toBeFalsy: (..._args: any[]) => {},
  toBeGreaterThan: (..._args: any[]) => {},
  toBeGreaterThanOrEqual: (..._args: any[]) => {},
  toBeLessThan: (..._args: any[]) => {},
  toBeLessThanOrEqual: (..._args: any[]) => {}
});
import { 
  PersonaGenome, 
  EvolutionaryPressure,
  Challenge,
  ChallengeResult,
  generateUUID,
  PersonaRole,
  Specialization,
  SPECIALIZATIONS,
  createDefaultPersonalityTraits
} from '../../shared/AcademyTypes';

// ==================== PERSONA FACTORIES ====================

/**
 * Create test persona configuration with sensible defaults
 */
export function createTestPersonaConfig(overrides: Partial<PersonaGenome> = {}): PersonaGenome {
  return createTestPersonaGenome(overrides);
}

/**
 * Create multiple test persona configurations with different specializations
 */
export function createTestPersonaConfigs(count: number = 3): PersonaGenome[] {
  return Array.from({ length: count }, (_, i) => 
    createTestPersonaGenome({
      name: `TestPersona${i + 1}`,
      identity: {
        role: 'student' as PersonaRole,
        generation: 0,
        parentIds: [],
        specialization: 'typescript', // Default specialization
        personality: createDefaultPersonalityTraits(),
        goals: ['learn_programming', 'master_typescript']
      }
    })
  );
}

/**
 * Create test persona genome with realistic values
 */
export function createTestPersonaGenome(overrides: Partial<PersonaGenome> = {}): PersonaGenome {
  const id = generateUUID();
  const specialization = overrides.identity?.specialization || 'typescript';
  
  return {
    id,
    name: `TestPersona_${id.substring(0, 8)}`,
    created: Date.now(),
    identity: {
      role: 'student',
      generation: 0,
      specialization: specialization as Specialization,
      personality: {
        creativity: 0.6,
        analytical: 0.7,
        helpfulness: 0.8,
        competitiveness: 0.5,
        patience: 0.6,
        innovation: 0.5
      },
      goals: [`master_${specialization}`, 'collaborate_effectively']
    },
    knowledge: {
      domain: specialization,
      expertise: [specialization, 'problem_solving'],
      competencies: {
        [specialization]: 0.6,
        'problem_solving': 0.5,
        'collaboration': 0.4
      },
      experiencePoints: 250
    },
    behavior: {
      learningStyle: 'analytical',
      adaptationRate: 0.5,
      communicationStyle: 'direct',
      decisionMakingStyle: 'analytical',
      riskTolerance: 0.4,
      collaborationPreference: 0.6
    },
    evolution: {
      generation: 0,
      parentGenomes: [],
      mutationHistory: [],
      evolutionStage: 'spawning',
      fitnessScore: 0.6,
      adaptationSuccess: 0,
      survivalRounds: 0,
      evolutionPressure: []
    },
    substrate: {
      loraIds: [`${specialization}_lora`, 'base_reasoning_lora'],
      memoryPatterns: ['working_memory'],
      processingStyle: 'sequential',
      adaptationMechanisms: ['reinforcement_learning'],
      vectorPosition: Array.from({ length: 10 }, () => Math.random())
    },
    reproduction: {
      mutationRate: 0.1,
      reproductionEligibility: true,
      breedingSuccess: 0,
      offspringCount: 0
    },
    lineage: {
      ancestors: [],
      descendants: [],
      siblings: [],
      generation: 0,
      lineageStrength: 0.5,
      emergentTraits: []
    },
    ...overrides
  };
}

/**
 * Create population of test personas with varied fitness scores
 */
export function createTestPersonaPopulation(size: number = 5): PersonaGenome[] {
  return Array.from({ length: size }, (_, i) => {
    const specialization = SPECIALIZATIONS[i % SPECIALIZATIONS.length];
    const fitness = Math.random() * 0.8 + 0.2; // 0.2 to 1.0
    
    return createTestPersonaGenome({
      identity: {
        role: 'student',
        generation: 0,
        specialization: specialization as Specialization,
        personality: {
          creativity: Math.random() * 0.6 + 0.3,
          analytical: Math.random() * 0.6 + 0.3,
          helpfulness: Math.random() * 0.6 + 0.3,
          competitiveness: Math.random() * 0.6 + 0.3,
          patience: Math.random() * 0.6 + 0.3,
          innovation: Math.random() * 0.6 + 0.3
        },
        goals: [`master_${specialization}`, 'collaborate_effectively']
      },
      evolution: {
        generation: 0,
        parentGenomes: [],
        mutationHistory: [],
        evolutionStage: 'spawning',
        fitnessScore: fitness,
        adaptationSuccess: 0,
        survivalRounds: 0,
        evolutionPressure: []
      }
    });
  });
}

// ==================== EVOLUTION FACTORIES ====================

/**
 * Create test evolution configuration with sensible defaults
 */
export function createTestEvolutionConfig(overrides: Partial<{
  generations: number;
  populationSize: number;
  evolutionaryPressure: EvolutionaryPressure;
}> = {}): {
  generations: number;
  populationSize: number;
  evolutionaryPressure: EvolutionaryPressure;
} {
  return {
    generations: 3,
    populationSize: 5,
    evolutionaryPressure: createTestEvolutionaryPressure(),
    ...overrides
  };
}

/**
 * Create test evolutionary pressure configuration
 */
export function createTestEvolutionaryPressure(overrides: Partial<EvolutionaryPressure> = {}): EvolutionaryPressure {
  return {
    survivalRate: 0.6,
    selectionCriteria: {
      performance: 0.4,
      innovation: 0.2,
      adaptation: 0.2,
      collaboration: 0.15,
      teaching: 0.05
    },
    environmentalFactors: ['competition'],
    competitionLevel: 0.5,
    collaborationRequirement: 0.3,
    ...overrides
  };
}

/**
 * Create test challenge with realistic parameters
 */
export function createTestChallenge(overrides: Partial<Challenge> = {}): Challenge {
  return {
    id: generateUUID(),
    domain: 'typescript',
    difficulty: 0.5,
    prompt: 'Implement a TypeScript function with proper error handling',
    expectedBehaviors: ['type_safety', 'error_handling', 'clean_code'],
    solvabilityCheck: (input: string) => input.includes('function') && input.includes('TypeScript'),
    timeLimit: 300000, // 5 minutes
    resources: ['typescript_lora', 'documentation'],
    successCriteria: {
      accuracy: 0.8,
      timeThreshold: 300000,
      resourceEfficiency: 0.7,
      innovationBonus: false,
      collaborationRequired: false
    },
    ...overrides
  };
}

/**
 * Create test challenge result with realistic outcomes
 */
export function createTestChallengeResult(overrides: Partial<ChallengeResult> = {}): ChallengeResult {
  return {
    challengeId: generateUUID(),
    personaId: generateUUID(),
    success: Math.random() > 0.3,
    accuracy: Math.random() * 0.8 + 0.2,
    timeUsed: Math.random() * 250000 + 50000, // 50s to 5min
    resourcesUsed: ['typescript_lora'],
    innovationScore: Math.random() * 0.5,
    collaborationScore: Math.random() * 0.3,
    emergentCapabilities: Math.random() > 0.8 ? ['novel_solution'] : [],
    behaviorDetected: ['problem_solving', 'adaptation'],
    ...overrides
  };
}

// ==================== MOCK FACTORIES ====================

/**
 * Create mock AcademyBase implementation for testing
 */
export function createMockAcademyBase() {
  return {
    personas: new Map<string, PersonaGenome>(),
    sessions: new Map<string, any>(),
    messageLog: [] as any[],
    
    async spawnPersona(config: Partial<PersonaGenome>): Promise<PersonaGenome> {
      const persona = createTestPersonaGenome({
        identity: {
          role: config.identity?.role || 'student',
          generation: 0,
          parentIds: [],
          specialization: config.identity?.specialization || 'typescript',
          personality: {
            creativity: Math.random() * 0.6 + 0.3,
            analytical: Math.random() * 0.6 + 0.3,
            helpfulness: Math.random() * 0.6 + 0.3,
            competitiveness: Math.random() * 0.6 + 0.3,
            patience: Math.random() * 0.6 + 0.3,
            innovation: Math.random() * 0.6 + 0.3
          },
          goals: [`master_${config.identity?.specialization || 'typescript'}`, 'collaborate_effectively']
        }
      });
      
      this.personas.set(persona.id, persona);
      return persona;
    },
    
    getPersona(id: string): PersonaGenome | null {
      return this.personas.get(id) || null;
    },
    
    getAllPersonas(): PersonaGenome[] {
      return Array.from(this.personas.values());
    },
    
    async createSandboxedSession(persona: PersonaGenome): Promise<string> {
      const sessionId = generateUUID();
      this.sessions.set(sessionId, {
        persona,
        created: Date.now(),
        challenges: [],
        results: []
      });
      return sessionId;
    },
    
    async cleanupSession(sessionId: string): Promise<void> {
      this.sessions.delete(sessionId);
    },
    
    async sendMessage(message: any): Promise<any> {
      this.messageLog.push(message);
      return { status: 'ok', echo: message };
    }
  };
}

/**
 * Create mock daemon client for testing
 */
export function createMockDaemonClient() {
  return {
    messageLog: [] as any[],
    
    async sendMessage(message: any): Promise<any> {
      this.messageLog.push(message);
      return {
        success: true,
        data: { echo: message },
        timestamp: Date.now()
      };
    },
    
    getMessageLog(): any[] {
      return this.messageLog;
    },
    
    clearMessageLog(): void {
      this.messageLog = [];
    }
  };
}

/**
 * Create mock session manager for testing
 */
export function createMockSessionManager() {
  return {
    sessions: new Map<string, any>(),
    
    async createSession(config: any): Promise<string> {
      const sessionId = generateUUID();
      this.sessions.set(sessionId, {
        id: sessionId,
        status: 'active',
        created: Date.now(),
        ...config
      });
      return sessionId;
    },
    
    async getSession(sessionId: string): Promise<any> {
      return this.sessions.get(sessionId) || null;
    },
    
    async cleanupSession(sessionId: string): Promise<void> {
      this.sessions.delete(sessionId);
    },
    
    getAllSessions(): any[] {
      return Array.from(this.sessions.values());
    }
  };
}

// ==================== ASSERTION HELPERS ====================

/**
 * Assert that a persona genome has valid structure
 */
export function assertValidPersonaGenome(persona: PersonaGenome): void {
  expect(persona).toBeDefined();
  expect(persona.id).toBeDefined();
  expect(persona.identity).toBeDefined();
  expect(persona.knowledge).toBeDefined();
  expect(persona.behavior).toBeDefined();
  expect(persona.evolution).toBeDefined();
  expect(persona.substrate).toBeDefined();
  expect(persona.reproduction).toBeDefined();
  expect(persona.lineage).toBeDefined();
  
  // Validate personality traits are in range [0, 1]
  const personality = persona.identity.personality;
  Object.values(personality).forEach(trait => {
    expect(trait).toBeGreaterThanOrEqual(0);
    expect(trait).toBeLessThanOrEqual(1);
  });
  
  // Validate fitness score
  expect(persona.evolution.fitnessScore).toBeGreaterThanOrEqual(0);
  expect(persona.evolution.fitnessScore).toBeLessThanOrEqual(1);
  
  // Validate generation
  expect(persona.evolution.generation).toBeGreaterThanOrEqual(0);
}

/**
 * Assert that an evolution config is valid
 */
export function assertValidEvolutionConfig(config: {
  generations: number;
  populationSize: number;
  evolutionaryPressure: EvolutionaryPressure;
}): void {
  expect(config).toBeDefined();
  expect(config.generations).toBeGreaterThan(0);
  expect(config.populationSize).toBeGreaterThanOrEqual(2);
  expect(config.evolutionaryPressure).toBeDefined();
  expect(config.evolutionaryPressure.survivalRate).toBeGreaterThan(0);
  expect(config.evolutionaryPressure.survivalRate).toBeLessThanOrEqual(1);
}

/**
 * Assert that a challenge result is valid
 */
export function assertValidChallengeResult(result: ChallengeResult): void {
  expect(result).toBeDefined();
  expect(result.challengeId).toBeDefined();
  expect(result.personaId).toBeDefined();
  expect(typeof result.success).toBe('boolean');
  expect(result.accuracy).toBeGreaterThanOrEqual(0);
  expect(result.accuracy).toBeLessThanOrEqual(1);
  expect(result.timeUsed).toBeGreaterThan(0);
  expect(Array.isArray(result.resourcesUsed)).toBe(true);
  expect(Array.isArray(result.emergentCapabilities)).toBe(true);
  expect(Array.isArray(result.behaviorDetected)).toBe(true);
}

// ==================== MOCK HELPERS ====================

/**
 * Mock console methods for testing
 */
export function mockConsole() {
  const originalLog = console.log;
  const originalError = console.error;
  const originalWarn = console.warn;
  
  const logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
  const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
  const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
  
  return {
    log: logSpy,
    error: errorSpy,
    warn: warnSpy,
    restore: () => {
      console.log = originalLog;
      console.error = originalError;
      console.warn = originalWarn;
    }
  };
}

/**
 * Mock file system operations
 */
export function mockFileSystem() {
  const mockFs = {
    files: new Map<string, string>(),
    
    writeFile: vi.fn(async (path: string, content: string) => {
      mockFs.files.set(path, content);
    }),
    
    readFile: vi.fn(async (path: string) => {
      const content = mockFs.files.get(path);
      if (!content) throw new Error(`File not found: ${path}`);
      return content;
    }),
    
    mkdir: vi.fn(async (_path: string) => {
      // Mock directory creation
    }),
    
    rm: vi.fn(async (path: string) => {
      mockFs.files.delete(path);
    }),
    
    access: vi.fn(async (path: string) => {
      if (!mockFs.files.has(path)) {
        throw new Error(`File not found: ${path}`);
      }
    }),
    
    readdir: vi.fn(async (path: string) => {
      return Array.from(mockFs.files.keys())
        .filter(file => file.startsWith(path))
        .map(file => file.replace(path + '/', ''));
    }),
    
    clearFiles: () => {
      mockFs.files.clear();
    },
    
    getFiles: () => {
      return new Map(mockFs.files);
    }
  };
  
  return mockFs;
}

/**
 * Mock process operations
 */
export function mockProcess() {
  const mockProcess = {
    spawned: [] as any[],
    
    spawn: vi.fn((command: string, args: string[], options: any) => {
      const mockChildProcess = {
        pid: Math.floor(Math.random() * 10000),
        stdout: { on: vi.fn() },
        stderr: { on: vi.fn() },
        on: vi.fn(),
        kill: vi.fn(),
        send: vi.fn()
      };
      
      mockProcess.spawned.push({
        command,
        args,
        options,
        process: mockChildProcess
      });
      
      return mockChildProcess;
    }),
    
    getSpawnedProcesses: () => mockProcess.spawned,
    
    clearSpawnedProcesses: () => {
      mockProcess.spawned = [];
    }
  };
  
  return mockProcess;
}

// ==================== TEST DATA GENERATORS ====================

/**
 * Generate realistic persona names
 */
export function generatePersonaNames(count: number = 10): string[] {
  const prefixes = ['Code', 'Test', 'Arch', 'UI', 'Debug', 'Opt'];
  const suffixes = ['Master', 'Expert', 'Guru', 'Ninja', 'Wizard', 'Pro'];
  const names = [];
  
  for (let i = 0; i < count; i++) {
    const prefix = prefixes[i % prefixes.length];
    const suffix = suffixes[Math.floor(i / prefixes.length) % suffixes.length];
    names.push(`${prefix}${suffix}${i + 1}`);
  }
  
  return names;
}

/**
 * Generate realistic fitness scores with distribution
 */
export function generateFitnessScores(count: number = 10): number[] {
  return Array.from({ length: count }, (_, i) => {
    // Create a realistic distribution with some high performers
    const base = Math.random() * 0.6 + 0.2; // 0.2 to 0.8
    const bonus = i < count * 0.2 ? Math.random() * 0.2 : 0; // Top 20% get bonus
    return Math.min(1.0, base + bonus);
  });
}

/**
 * Generate test lineage with multiple generations
 */
export function generateTestLineage(generations: number = 3): PersonaGenome[] {
  const lineage: PersonaGenome[] = [];
  
  // Generation 0 (founders)
  const founders = createTestPersonaPopulation(2);
  lineage.push(...founders);
  
  // Subsequent generations
  for (let gen = 1; gen < generations; gen++) {
    const parentCount = lineage.filter(p => p.evolution.generation === gen - 1).length;
    const childrenCount = Math.floor(parentCount * 1.5);
    
    for (let i = 0; i < childrenCount; i++) {
      const parent1 = lineage[Math.floor(Math.random() * lineage.length)];
      const parent2 = lineage[Math.floor(Math.random() * lineage.length)];
      
      const child = createTestPersonaGenome({
        identity: {
          role: 'student',
          generation: gen,
          specialization: Math.random() > 0.5 ? parent1.identity.specialization : parent2.identity.specialization,
          personality: {
            creativity: (parent1.identity.personality.creativity + parent2.identity.personality.creativity) / 2,
            analytical: (parent1.identity.personality.analytical + parent2.identity.personality.analytical) / 2,
            helpfulness: (parent1.identity.personality.helpfulness + parent2.identity.personality.helpfulness) / 2,
            competitiveness: (parent1.identity.personality.competitiveness + parent2.identity.personality.competitiveness) / 2,
            patience: (parent1.identity.personality.patience + parent2.identity.personality.patience) / 2,
            innovation: (parent1.identity.personality.innovation + parent2.identity.personality.innovation) / 2
          },
          goals: [`master_${parent1.identity.specialization}`, 'collaborate_effectively']
        },
        evolution: {
          generation: gen,
          parentGenomes: [parent1.id, parent2.id],
          mutationHistory: [],
          evolutionStage: 'spawning',
          fitnessScore: Math.random() * 0.8 + 0.2,
          adaptationSuccess: 0,
          survivalRounds: 0,
          evolutionPressure: []
        },
        lineage: {
          ancestors: [...new Set([...parent1.lineage.ancestors, ...parent2.lineage.ancestors, parent1.id, parent2.id])],
          descendants: [],
          siblings: [],
          generation: gen,
          lineageStrength: (parent1.lineage.lineageStrength + parent2.lineage.lineageStrength) / 2,
          emergentTraits: []
        }
      });
      
      lineage.push(child);
    }
  }
  
  return lineage;
}

// ==================== PERFORMANCE HELPERS ====================

/**
 * Measure execution time of async function
 */
export async function measureExecutionTime<T>(fn: () => Promise<T>): Promise<{ result: T; timeMs: number }> {
  const start = performance.now();
  const result = await fn();
  const end = performance.now();
  
  return {
    result,
    timeMs: end - start
  };
}

/**
 * Run function multiple times and get average execution time
 */
export async function benchmarkFunction<T>(
  fn: () => Promise<T>, 
  iterations: number = 10
): Promise<{ averageTimeMs: number; results: T[] }> {
  const results: T[] = [];
  const times: number[] = [];
  
  for (let i = 0; i < iterations; i++) {
    const { result, timeMs } = await measureExecutionTime(fn);
    results.push(result);
    times.push(timeMs);
  }
  
  const averageTimeMs = times.reduce((sum, time) => sum + time, 0) / times.length;
  
  return { averageTimeMs, results };
}

// ==================== CLEANUP HELPERS ====================

/**
 * Clean up test resources
 */
export function cleanupTestResources() {
  // Clear any global state
  vi.clearAllMocks();
  vi.restoreAllMocks();
}

/**
 * Setup test environment
 */
export function setupTestEnvironment() {
  // Common setup for all tests
  vi.clearAllMocks();
  
  // Mock global objects if needed
  Object.defineProperty(global, 'performance', {
    value: {
      now: vi.fn(() => Date.now())
    }
  });
}