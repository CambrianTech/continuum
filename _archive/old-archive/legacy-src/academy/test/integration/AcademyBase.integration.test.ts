/**
 * Integration tests for Academy system with full daemon integration
 * 
 * These tests validate the complete flow of Academy evolution system:
 * - Academy daemon startup and lifecycle
 * - Persona spawning and management via PersonaManager
 * - Evolution process with challenge execution
 * - Session management and sandboxing
 * - Inter-daemon communication patterns
 * 
 * This follows the middle-out testing methodology for layer 4 validation.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { AcademyBase } from '../../shared/AcademyBase';
import { 
  PersonaGenome, 
  PersonaSpawnConfig, 
  EvolutionConfig, 
  Challenge,
  EvolutionaryPressure,
  generateUUID,
  SPECIALIZATIONS
} from '../../shared/AcademyTypes';
import { 
  createTestPersonaConfig,
  createTestPersonaPopulation,
  createTestEvolutionConfig,
  createTestChallenge,
  createMockDaemonClient,
  createMockSessionManager,
  assertValidPersonaGenome,
  assertValidEvolutionConfig,
  measureExecutionTime,
  setupTestEnvironment,
  cleanupTestResources
} from '../shared/TestUtilities';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

// Mock Academy implementation for integration testing
class MockAcademy extends AcademyBase<any, any> {
  private sessionManager: any;
  private daemonClient: any;
  public sandboxedSessions: Map<string, any> = new Map();
  public executionLog: any[] = [];

  constructor(sessionManager: any, daemonClient: any) {
    super();
    this.sessionManager = sessionManager;
    this.daemonClient = daemonClient;
  }

  async createSandboxedSession(persona: PersonaGenome): Promise<string> {
    const sessionId = await this.sessionManager.createSession({
      persona,
      type: 'evolution',
      isolated: true
    });
    
    this.sandboxedSessions.set(sessionId, {
      persona,
      created: Date.now(),
      challenges: [],
      results: []
    });
    
    return sessionId;
  }

  async cleanupSession(sessionId: string): Promise<void> {
    await this.sessionManager.cleanupSession(sessionId);
    this.sandboxedSessions.delete(sessionId);
  }

  async executeChallengeInSandbox(sessionId: string, challenge: Challenge): Promise<any> {
    const session = this.sandboxedSessions.get(sessionId);
    if (!session) {
      throw new Error(`Session not found: ${sessionId}`);
    }

    // Simulate challenge execution
    const executionResult = {
      challengeId: challenge.id,
      sessionId,
      personaId: session.persona.id,
      success: Math.random() > 0.3,
      accuracy: Math.random() * 0.8 + 0.2,
      timeUsed: Math.random() * 250000 + 50000,
      resourcesUsed: challenge.resources || [],
      innovationScore: Math.random() * 0.5,
      collaborationScore: Math.random() * 0.3,
      emergentCapabilities: Math.random() > 0.8 ? ['novel_solution'] : [],
      behaviorDetected: ['problem_solving', 'adaptation']
    };

    this.executionLog.push({
      type: 'challenge_execution',
      sessionId,
      challengeId: challenge.id,
      result: executionResult,
      timestamp: Date.now()
    });

    return executionResult;
  }

  async sendMessage(message: any): Promise<any> {
    return await this.daemonClient.sendMessage(message);
  }
}

describe('Academy Integration Tests', () => {
  let mockAcademy: MockAcademy;
  let mockSessionManager: any;
  let mockDaemonClient: any;
  let tempDir: string;

  beforeEach(async () => {
    setupTestEnvironment();
    
    // Create temporary directory for integration tests
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'academy-integration-'));
    
    // Setup mock dependencies
    mockSessionManager = createMockSessionManager();
    mockDaemonClient = createMockDaemonClient();
    
    // Create academy instance
    mockAcademy = new MockAcademy(mockSessionManager, mockDaemonClient);
  });

  afterEach(async () => {
    cleanupTestResources();
    
    // Cleanup temporary directory
    try {
      await fs.rm(tempDir, { recursive: true, force: true });
    } catch (error) {
      console.warn('Failed to cleanup temp directory:', error);
    }
  });

  describe('Academy Daemon Integration', () => {
    it('should integrate with daemon messaging system', async () => {
      // Test daemon communication
      const testMessage = {
        type: 'academy_status',
        data: { version: '1.0.0' },
        timestamp: Date.now()
      };

      const response = await mockAcademy.sendMessage(testMessage);
      
      expect(response).toBeDefined();
      expect(response.success).toBe(true);
      expect(response.data.echo).toEqual(testMessage);
      
      // Verify message was logged
      const messageLog = mockDaemonClient.getMessageLog();
      expect(messageLog).toHaveLength(1);
      expect(messageLog[0]).toEqual(testMessage);
    });

    it('should handle daemon communication errors gracefully', async () => {
      // Mock daemon client error
      const failingClient = {
        sendMessage: vi.fn().mockRejectedValue(new Error('Daemon connection failed'))
      };
      
      const failingAcademy = new MockAcademy(mockSessionManager, failingClient);
      
      await expect(failingAcademy.sendMessage({ type: 'test' }))
        .rejects.toThrow('Daemon connection failed');
    });
  });

  describe('Persona Management Integration', () => {
    it('should spawn persona and validate through PersonaManager', async () => {
      const personaConfig = createTestPersonaConfig({
        name: 'TypeScriptExpert',
        specialization: 'typescript',
        role: 'teacher'
      });

      const { result: persona, timeMs } = await measureExecutionTime(async () => {
        return await mockAcademy.spawnPersona(personaConfig);
      });

      // Validate persona structure
      assertValidPersonaGenome(persona);
      expect(persona.identity.name).toBe('TypeScriptExpert');
      expect(persona.identity.specialization).toBe('typescript');
      expect(persona.identity.role).toBe('teacher');
      
      // Validate performance (should be fast)
      expect(timeMs).toBeLessThan(100);
      
      // Verify persona is stored
      const retrievedPersona = mockAcademy.getPersona(persona.id);
      expect(retrievedPersona).toEqual(persona);
    });

    it('should manage persona population lifecycle', async () => {
      const populationSize = 5;
      const personas: PersonaGenome[] = [];

      // Spawn population
      for (let i = 0; i < populationSize; i++) {
        const config = createTestPersonaConfig({
          name: `Persona${i + 1}`,
          specialization: SPECIALIZATIONS[i % SPECIALIZATIONS.length]
        });
        
        const persona = await mockAcademy.spawnPersona(config);
        personas.push(persona);
      }

      // Verify population
      const allPersonas = mockAcademy.getAllPersonas();
      expect(allPersonas).toHaveLength(populationSize);
      
      // Verify each persona is valid
      allPersonas.forEach(persona => {
        assertValidPersonaGenome(persona);
      });
      
      // Verify population diversity
      const specializations = new Set(allPersonas.map(p => p.identity.specialization));
      expect(specializations.size).toBeGreaterThan(1);
    });
  });

  describe('Evolution Process Integration', () => {
    it('should execute complete evolution cycle with session management', async () => {
      // Setup evolution
      const evolutionConfig = createTestEvolutionConfig({
        generations: 2,
        populationSize: 3
      });
      
      assertValidEvolutionConfig(evolutionConfig);

      // Create initial population
      const initialPopulation = createTestPersonaPopulation(evolutionConfig.populationSize);
      
      // Start evolution
      const evolutionPromise = mockAcademy.startEvolution(evolutionConfig);
      
      // Verify evolution started
      expect(evolutionPromise).toBeInstanceOf(Promise);
      
      // Wait for evolution to complete
      const evolutionResult = await evolutionPromise;
      
      // Validate evolution result
      expect(evolutionResult).toBeDefined();
      expect(evolutionResult.generations).toBe(evolutionConfig.generations);
      expect(evolutionResult.finalPopulation).toHaveLength(evolutionConfig.populationSize);
      
      // Verify final population has evolved
      evolutionResult.finalPopulation.forEach(persona => {
        assertValidPersonaGenome(persona);
        expect(persona.evolution.generation).toBeGreaterThanOrEqual(0);
      });
    });

    it('should handle evolution with challenge execution', async () => {
      // Create test challenge
      const challenge = createTestChallenge({
        domain: 'typescript',
        difficulty: 0.6,
        prompt: 'Implement a TypeScript generic utility function',
        expectedBehaviors: ['type_safety', 'generics', 'error_handling']
      });

      // Create persona
      const persona = await mockAcademy.spawnPersona(createTestPersonaConfig());
      
      // Create sandboxed session
      const sessionId = await mockAcademy.createSandboxedSession(persona);
      
      // Execute challenge
      const result = await mockAcademy.executeChallengeInSandbox(sessionId, challenge);
      
      // Validate challenge execution
      expect(result).toBeDefined();
      expect(result.challengeId).toBe(challenge.id);
      expect(result.personaId).toBe(persona.id);
      expect(result.sessionId).toBe(sessionId);
      expect(typeof result.success).toBe('boolean');
      expect(result.accuracy).toBeGreaterThanOrEqual(0);
      expect(result.accuracy).toBeLessThanOrEqual(1);
      
      // Verify execution was logged
      expect(mockAcademy.executionLog).toHaveLength(1);
      const logEntry = mockAcademy.executionLog[0];
      expect(logEntry.type).toBe('challenge_execution');
      expect(logEntry.challengeId).toBe(challenge.id);
      
      // Cleanup session
      await mockAcademy.cleanupSession(sessionId);
      expect(mockAcademy.sandboxedSessions.has(sessionId)).toBe(false);
    });
  });

  describe('Session Management Integration', () => {
    it('should create and manage sandboxed sessions', async () => {
      const persona = await mockAcademy.spawnPersona(createTestPersonaConfig());
      
      // Create session
      const sessionId = await mockAcademy.createSandboxedSession(persona);
      
      expect(sessionId).toBeDefined();
      expect(typeof sessionId).toBe('string');
      
      // Verify session exists
      expect(mockAcademy.sandboxedSessions.has(sessionId)).toBe(true);
      
      const session = mockAcademy.sandboxedSessions.get(sessionId);
      expect(session.persona).toEqual(persona);
      expect(session.type).toBe('evolution');
      expect(session.isolated).toBe(true);
      
      // Cleanup session
      await mockAcademy.cleanupSession(sessionId);
      expect(mockAcademy.sandboxedSessions.has(sessionId)).toBe(false);
    });

    it('should handle multiple concurrent sessions', async () => {
      const sessionCount = 5;
      const sessions: { sessionId: string; persona: PersonaGenome }[] = [];

      // Create multiple sessions
      for (let i = 0; i < sessionCount; i++) {
        const persona = await mockAcademy.spawnPersona(createTestPersonaConfig({
          name: `ConcurrentPersona${i + 1}`
        }));
        
        const sessionId = await mockAcademy.createSandboxedSession(persona);
        sessions.push({ sessionId, persona });
      }

      // Verify all sessions exist
      expect(mockAcademy.sandboxedSessions.size).toBe(sessionCount);
      
      // Verify session isolation
      sessions.forEach(({ sessionId, persona }) => {
        const session = mockAcademy.sandboxedSessions.get(sessionId);
        expect(session.persona.id).toBe(persona.id);
      });

      // Cleanup all sessions
      await Promise.all(sessions.map(({ sessionId }) => 
        mockAcademy.cleanupSession(sessionId)
      ));
      
      expect(mockAcademy.sandboxedSessions.size).toBe(0);
    });
  });

  describe('Performance and Scalability Integration', () => {
    it('should handle evolution with performance constraints', async () => {
      const performanceConfig = createTestEvolutionConfig({
        generations: 3,
        populationSize: 10,
        evolutionaryPressure: {
          survivalRate: 0.5,
          selectionCriteria: {
            performance: 0.6,
            innovation: 0.2,
            adaptation: 0.1,
            collaboration: 0.1,
            teaching: 0.0
          },
          environmentalFactors: ['competition', 'resource_scarcity'],
          competitionLevel: 0.8,
          collaborationRequirement: 0.2
        }
      });

      const { result: evolutionResult, timeMs } = await measureExecutionTime(async () => {
        return await mockAcademy.startEvolution(performanceConfig);
      });

      // Verify evolution completed
      expect(evolutionResult).toBeDefined();
      expect(evolutionResult.generations).toBe(3);
      
      // Verify performance (should complete within reasonable time)
      expect(timeMs).toBeLessThan(5000); // 5 seconds max
      
      // Verify population was properly selected
      const finalPopulation = evolutionResult.finalPopulation;
      expect(finalPopulation).toHaveLength(10);
      
      // Verify fitness distribution (should have some variation)
      const fitnessScores = finalPopulation.map(p => p.evolution.fitnessScore);
      const avgFitness = fitnessScores.reduce((sum, score) => sum + score, 0) / fitnessScores.length;
      expect(avgFitness).toBeGreaterThan(0.3);
      expect(avgFitness).toBeLessThan(1.0);
    });

    it('should handle resource cleanup under load', async () => {
      const loadTestConfig = {
        concurrentEvolutions: 3,
        populationSize: 5,
        generations: 2
      };

      const evolutionPromises = [];
      
      // Start multiple concurrent evolutions
      for (let i = 0; i < loadTestConfig.concurrentEvolutions; i++) {
        const config = createTestEvolutionConfig({
          generations: loadTestConfig.generations,
          populationSize: loadTestConfig.populationSize
        });
        
        evolutionPromises.push(mockAcademy.startEvolution(config));
      }

      // Wait for all evolutions to complete
      const results = await Promise.all(evolutionPromises);
      
      // Verify all evolutions completed successfully
      results.forEach(result => {
        expect(result).toBeDefined();
        expect(result.generations).toBe(loadTestConfig.generations);
        expect(result.finalPopulation).toHaveLength(loadTestConfig.populationSize);
      });
      
      // Verify no session leaks
      expect(mockAcademy.sandboxedSessions.size).toBe(0);
    });
  });

  describe('Error Handling Integration', () => {
    it('should handle session manager failures gracefully', async () => {
      // Mock session manager failure
      const failingSessionManager = {
        createSession: vi.fn().mockRejectedValue(new Error('Session creation failed')),
        cleanupSession: vi.fn(),
        getSession: vi.fn(),
        getAllSessions: vi.fn().mockReturnValue([])
      };
      
      const failingAcademy = new MockAcademy(failingSessionManager, mockDaemonClient);
      const persona = await failingAcademy.spawnPersona(createTestPersonaConfig());
      
      // Should handle session creation failure
      await expect(failingAcademy.createSandboxedSession(persona))
        .rejects.toThrow('Session creation failed');
    });

    it('should handle challenge execution failures', async () => {
      const persona = await mockAcademy.spawnPersona(createTestPersonaConfig());
      const sessionId = await mockAcademy.createSandboxedSession(persona);
      
      // Test with invalid challenge
      const invalidChallenge = {
        ...createTestChallenge(),
        id: '', // Invalid ID
        solvabilityCheck: null as any
      };
      
      // Should handle gracefully
      await expect(mockAcademy.executeChallengeInSandbox(sessionId, invalidChallenge))
        .rejects.toThrow();
        
      // Should handle invalid session
      await expect(mockAcademy.executeChallengeInSandbox('invalid-session', createTestChallenge()))
        .rejects.toThrow('Session not found');
    });
  });

  describe('Middle-Out Layer 4 Validation', () => {
    it('should validate Academy architecture integration', () => {
      // Verify Academy follows middle-out pattern
      expect(mockAcademy).toBeInstanceOf(AcademyBase);
      
      // Verify required methods are implemented
      expect(typeof mockAcademy.createSandboxedSession).toBe('function');
      expect(typeof mockAcademy.cleanupSession).toBe('function');
      expect(typeof mockAcademy.executeChallengeInSandbox).toBe('function');
      expect(typeof mockAcademy.sendMessage).toBe('function');
      
      // Verify shared functionality is inherited
      expect(typeof mockAcademy.spawnPersona).toBe('function');
      expect(typeof mockAcademy.startEvolution).toBe('function');
      expect(typeof mockAcademy.getPersona).toBe('function');
      expect(typeof mockAcademy.getAllPersonas).toBe('function');
    });

    it('should validate integration with external systems', async () => {
      // Test PersonaManager integration
      const persona = await mockAcademy.spawnPersona(createTestPersonaConfig());
      assertValidPersonaGenome(persona);
      
      // Test session management integration
      const sessionId = await mockAcademy.createSandboxedSession(persona);
      expect(mockSessionManager.getAllSessions()).toHaveLength(1);
      
      // Test daemon communication integration
      const message = { type: 'test', data: { test: true } };
      const response = await mockAcademy.sendMessage(message);
      expect(response.success).toBe(true);
      
      // Verify integration consistency
      expect(mockDaemonClient.getMessageLog()).toHaveLength(1);
      expect(mockAcademy.sandboxedSessions.has(sessionId)).toBe(true);
    });
  });
});