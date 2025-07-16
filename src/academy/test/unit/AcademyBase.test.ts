/**
 * AcademyBase Unit Tests
 * 
 * Tests for the core Academy base class functionality following the middle-out testing methodology.
 * 
 * Test Coverage:
 * - Persona genome creation and validation
 * - Evolution configuration processing
 * - Session management abstractions
 * - Survivor selection algorithms
 * - Lineage tracking and family trees
 * - Ecosystem metrics calculation
 * - Error handling and edge cases
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { AcademyBase, AcademyStatus, EvolutionConfig, PersonaSpawnConfig } from '../../shared/AcademyBase';
import { PersonaGenome, EvolutionaryPressure, generateUUID } from '../../shared/AcademyTypes';

// Test implementation of AcademyBase
class TestAcademyBase extends AcademyBase<any, any> {
  private testSessions: Map<string, any> = new Map();
  private mockMessageLog: any[] = [];

  async createSandboxedSession(persona: PersonaGenome): Promise<string> {
    const sessionId = generateUUID();
    this.testSessions.set(sessionId, { 
      persona, 
      created: Date.now(),
      challenges: [],
      results: []
    });
    return sessionId;
  }

  async cleanupSession(sessionId: string): Promise<void> {
    this.testSessions.delete(sessionId);
  }

  async executeChallengeInSandbox(sessionId: string, challenge: any): Promise<any> {
    const session = this.testSessions.get(sessionId);
    if (!session) throw new Error(`Session not found: ${sessionId}`);
    
    // Simulate challenge execution with realistic results
    const result = {
      success: Math.random() > 0.3,
      accuracy: Math.random() * 0.8 + 0.2, // 0.2 to 1.0
      timeUsed: Math.random() * 10000 + 1000, // 1-11 seconds
      sessionId,
      challengeId: challenge.id,
      emergentCapabilities: Math.random() > 0.8 ? ['novel_solution'] : []
    };
    
    session.challenges.push(challenge);
    session.results.push(result);
    
    return result;
  }

  async sendMessage(message: any): Promise<any> {
    this.mockMessageLog.push(message);
    return { 
      status: 'ok', 
      echo: message,
      timestamp: Date.now()
    };
  }

  // Expose protected methods for testing
  public testCreatePersonaGenome(config: PersonaSpawnConfig): PersonaGenome {
    return this.createPersonaGenome(config);
  }

  public testValidateEvolutionConfig(config: EvolutionConfig): void {
    return this.validateEvolutionConfig(config);
  }

  public testSelectSurvivors(personas: PersonaGenome[], pressure: EvolutionaryPressure): PersonaGenome[] {
    return this.selectSurvivors(personas, pressure);
  }

  public testReproduceOffspring(survivors: PersonaGenome[]): Promise<PersonaGenome[]> {
    return this.reproduceOffspring(survivors);
  }

  public testUpdateEcosystem(personas: PersonaGenome[]): void {
    return this.updateEcosystem(personas);
  }

  public testSimpleOffspring(parent1: PersonaGenome, parent2: PersonaGenome): PersonaGenome {
    return this.simpleOffspring(parent1, parent2);
  }

  public testCalculateEcosystemHealth(): any {
    return this.calculateEcosystemHealth();
  }

  public getTestSessions(): Map<string, any> {
    return this.testSessions;
  }

  public getMockMessageLog(): any[] {
    return this.mockMessageLog;
  }
}

describe('AcademyBase', () => {
  let academy: TestAcademyBase;

  beforeEach(() => {
    academy = new TestAcademyBase();
    vi.clearAllMocks();
  });

  afterEach(() => {
    // Clean up any resources
    academy.getAllPersonas().forEach(p => {
      // Clean up test data
    });
    vi.restoreAllMocks();
  });

  describe('Constructor and Initialization', () => {
    it('should initialize with default idle status', () => {
      const status = academy.getAcademyStatus();
      
      expect(status.mode).toBe('idle');
      expect(status.isActive).toBe(false);
      expect(status.totalPersonas).toBe(0);
      expect(status.activeSessions).toBe(0);
      expect(status.uptime).toBeTypeOf('number');
      expect(status.ecosystemMetrics).toBeDefined();
    });

    it('should initialize empty persona collection', () => {
      const personas = academy.getAllPersonas();
      expect(personas).toHaveLength(0);
    });

    it('should initialize ecosystem metrics with default values', () => {
      const status = academy.getAcademyStatus();
      const metrics = status.ecosystemMetrics;
      
      expect(metrics.totalPersonas).toBe(0);
      expect(metrics.activePersonas).toBe(0);
      expect(metrics.averageFitness).toBe(0);
      expect(metrics.generationNumber).toBe(0);
      expect(metrics.diversityIndex).toBe(0);
      expect(metrics.innovationRate).toBe(0);
      expect(metrics.graduationRate).toBe(0);
      expect(metrics.extinctionRate).toBe(0);
      expect(metrics.emergentCapabilities).toEqual([]);
      expect(metrics.ecosystemAge).toBe(0);
    });
  });

  describe('Persona Management', () => {
    describe('spawnPersona', () => {
      it('should create persona with valid configuration', async () => {
        const config: PersonaSpawnConfig = {
          name: 'TestPersona',
          specialization: 'typescript',
          role: 'student'
        };

        const persona = await academy.spawnPersona(config);

        expect(persona.identity.name).toBe('TestPersona');
        expect(persona.identity.specialization).toBe('typescript');
        expect(persona.identity.role).toBe('student');
        expect(persona.id).toBeDefined();
        expect(persona.evolution.generation).toBe(0);
        expect(persona.evolution.evolutionStage).toBe('spawning');
      });

      it('should create persona with default role when not specified', async () => {
        const config: PersonaSpawnConfig = {
          name: 'TestPersona',
          specialization: 'testing'
        };

        const persona = await academy.spawnPersona(config);
        expect(persona.identity.role).toBe('student');
      });

      it('should assign unique IDs to different personas', async () => {
        const config1: PersonaSpawnConfig = { name: 'Persona1', specialization: 'typescript' };
        const config2: PersonaSpawnConfig = { name: 'Persona2', specialization: 'testing' };

        const persona1 = await academy.spawnPersona(config1);
        const persona2 = await academy.spawnPersona(config2);

        expect(persona1.id).not.toBe(persona2.id);
        expect(persona1.identity.name).toBe('Persona1');
        expect(persona2.identity.name).toBe('Persona2');
      });

      it('should add persona to internal collection', async () => {
        const config: PersonaSpawnConfig = { name: 'TestPersona', specialization: 'typescript' };
        const persona = await academy.spawnPersona(config);

        const retrievedPersona = academy.getPersona(persona.id);
        expect(retrievedPersona).toEqual(persona);
      });

      it('should create persona with valid personality traits', async () => {
        const config: PersonaSpawnConfig = { name: 'TestPersona', specialization: 'typescript' };
        const persona = await academy.spawnPersona(config);

        const personality = persona.identity.personality;
        
        // All traits should be in valid range [0, 1]
        expect(personality.creativity).toBeGreaterThanOrEqual(0);
        expect(personality.creativity).toBeLessThanOrEqual(1);
        expect(personality.analytical).toBeGreaterThanOrEqual(0);
        expect(personality.analytical).toBeLessThanOrEqual(1);
        expect(personality.helpfulness).toBeGreaterThanOrEqual(0);
        expect(personality.helpfulness).toBeLessThanOrEqual(1);
        expect(personality.competitiveness).toBeGreaterThanOrEqual(0);
        expect(personality.competitiveness).toBeLessThanOrEqual(1);
        expect(personality.patience).toBeGreaterThanOrEqual(0);
        expect(personality.patience).toBeLessThanOrEqual(1);
        expect(personality.innovation).toBeGreaterThanOrEqual(0);
        expect(personality.innovation).toBeLessThanOrEqual(1);
      });

      it('should create persona with proper knowledge structure', async () => {
        const config: PersonaSpawnConfig = { name: 'TestPersona', specialization: 'architecture' };
        const persona = await academy.spawnPersona(config);

        const knowledge = persona.knowledge;
        
        expect(knowledge.domain).toBe('architecture');
        expect(knowledge.expertise).toContain('architecture');
        expect(knowledge.expertise).toContain('problem_solving');
        expect(knowledge.competencies).toHaveProperty('architecture');
        expect(knowledge.competencies).toHaveProperty('problem_solving');
        expect(knowledge.competencies).toHaveProperty('collaboration');
        expect(knowledge.experiencePoints).toBeGreaterThanOrEqual(0);
        expect(knowledge.experiencePoints).toBeLessThan(500);
      });
    });

    describe('getPersona', () => {
      it('should return persona by ID', async () => {
        const persona = await academy.spawnPersona({ name: 'Test', specialization: 'typescript' });
        const retrieved = academy.getPersona(persona.id);
        
        expect(retrieved).toEqual(persona);
      });

      it('should return null for non-existent persona', () => {
        const retrieved = academy.getPersona('non-existent-id');
        expect(retrieved).toBeNull();
      });

      it('should return null for empty string ID', () => {
        const retrieved = academy.getPersona('');
        expect(retrieved).toBeNull();
      });
    });

    describe('getAllPersonas', () => {
      it('should return all personas', async () => {
        const persona1 = await academy.spawnPersona({ name: 'Test1', specialization: 'typescript' });
        const persona2 = await academy.spawnPersona({ name: 'Test2', specialization: 'testing' });

        const allPersonas = academy.getAllPersonas();
        expect(allPersonas).toHaveLength(2);
        expect(allPersonas).toContain(persona1);
        expect(allPersonas).toContain(persona2);
      });

      it('should return empty array when no personas exist', () => {
        const allPersonas = academy.getAllPersonas();
        expect(allPersonas).toHaveLength(0);
      });

      it('should return fresh array on each call', async () => {
        await academy.spawnPersona({ name: 'Test', specialization: 'typescript' });
        
        const personas1 = academy.getAllPersonas();
        const personas2 = academy.getAllPersonas();
        
        expect(personas1).not.toBe(personas2); // Different array instances
        expect(personas1).toEqual(personas2); // Same content
      });
    });
  });

  describe('Evolution Configuration', () => {
    describe('validateEvolutionConfig', () => {
      it('should accept valid evolution configuration', () => {
        const config: EvolutionConfig = {
          generations: 5,
          populationSize: 10,
          evolutionaryPressure: {
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
            collaborationRequirement: 0.3
          }
        };

        expect(() => academy.testValidateEvolutionConfig(config)).not.toThrow();
      });

      it('should reject config with zero generations', () => {
        const config: EvolutionConfig = {
          generations: 0,
          populationSize: 10,
          evolutionaryPressure: {} as any
        };

        expect(() => academy.testValidateEvolutionConfig(config)).toThrow('Evolution must have at least 1 generation');
      });

      it('should reject config with negative generations', () => {
        const config: EvolutionConfig = {
          generations: -1,
          populationSize: 10,
          evolutionaryPressure: {} as any
        };

        expect(() => academy.testValidateEvolutionConfig(config)).toThrow('Evolution must have at least 1 generation');
      });

      it('should reject config with population size less than 2', () => {
        const config: EvolutionConfig = {
          generations: 5,
          populationSize: 1,
          evolutionaryPressure: {} as any
        };

        expect(() => academy.testValidateEvolutionConfig(config)).toThrow('Population size must be at least 2');
      });

      it('should reject config without evolutionary pressure', () => {
        const config: EvolutionConfig = {
          generations: 5,
          populationSize: 10,
          evolutionaryPressure: undefined as any
        };

        expect(() => academy.testValidateEvolutionConfig(config)).toThrow('Evolutionary pressure configuration is required');
      });

      it('should reject config with null evolutionary pressure', () => {
        const config: EvolutionConfig = {
          generations: 5,
          populationSize: 10,
          evolutionaryPressure: null as any
        };

        expect(() => academy.testValidateEvolutionConfig(config)).toThrow('Evolutionary pressure configuration is required');
      });
    });
  });

  describe('Persona Genome Creation', () => {
    it('should create valid persona genome structure', () => {
      const config: PersonaSpawnConfig = {
        name: 'TestPersona',
        specialization: 'typescript',
        role: 'student'
      };

      const genome = academy.testCreatePersonaGenome(config);

      // Verify all required fields exist
      expect(genome.identity).toBeDefined();
      expect(genome.knowledge).toBeDefined();
      expect(genome.behavior).toBeDefined();
      expect(genome.evolution).toBeDefined();
      expect(genome.substrate).toBeDefined();
      expect(genome.reproduction).toBeDefined();
      expect(genome.lineage).toBeDefined();
    });

    it('should create unique genome IDs', () => {
      const config: PersonaSpawnConfig = { name: 'Test', specialization: 'typescript' };
      
      const genome1 = academy.testCreatePersonaGenome(config);
      const genome2 = academy.testCreatePersonaGenome(config);
      
      expect(genome1.id).not.toBe(genome2.id);
      expect(genome1.id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    });

    it('should initialize genome with proper default values', () => {
      const config: PersonaSpawnConfig = { name: 'Test', specialization: 'debugging' };
      const genome = academy.testCreatePersonaGenome(config);

      expect(genome.identity.generation).toBe(0);
      expect(genome.evolution.generation).toBe(0);
      expect(genome.evolution.parentGenomes).toEqual([]);
      expect(genome.evolution.mutationHistory).toEqual([]);
      expect(genome.evolution.evolutionStage).toBe('spawning');
      expect(genome.lineage.ancestors).toEqual([]);
      expect(genome.lineage.descendants).toEqual([]);
      expect(genome.lineage.generation).toBe(0);
      expect(genome.reproduction.reproductionEligibility).toBe(true);
      expect(genome.reproduction.breedingSuccess).toBe(0);
      expect(genome.reproduction.offspringCount).toBe(0);
    });

    it('should set specialization-specific LoRA IDs', () => {
      const config: PersonaSpawnConfig = { name: 'Test', specialization: 'ui_design' };
      const genome = academy.testCreatePersonaGenome(config);

      expect(genome.substrate.loraIds).toContain('ui_design_lora');
      expect(genome.substrate.loraIds).toContain('base_reasoning_lora');
      expect(genome.substrate.loraIds.length).toBeGreaterThanOrEqual(2);
    });

    it('should create valid vector position', () => {
      const config: PersonaSpawnConfig = { name: 'Test', specialization: 'optimization' };
      const genome = academy.testCreatePersonaGenome(config);

      expect(genome.substrate.vectorPosition).toHaveLength(10);
      genome.substrate.vectorPosition.forEach(dimension => {
        expect(dimension).toBeGreaterThanOrEqual(0);
        expect(dimension).toBeLessThanOrEqual(1);
      });
    });
  });

  describe('Survivor Selection', () => {
    it('should select survivors based on fitness scores', () => {
      const personas: PersonaGenome[] = [
        academy.testCreatePersonaGenome({ name: 'High', specialization: 'typescript' }),
        academy.testCreatePersonaGenome({ name: 'Medium', specialization: 'testing' }),
        academy.testCreatePersonaGenome({ name: 'Low', specialization: 'architecture' })
      ];

      // Set fitness scores
      personas[0].evolution.fitnessScore = 0.9;
      personas[1].evolution.fitnessScore = 0.6;
      personas[2].evolution.fitnessScore = 0.3;

      const pressure: EvolutionaryPressure = {
        survivalRate: 0.67, // Should select top 67% (2 out of 3)
        selectionCriteria: {
          performance: 1.0,
          innovation: 0,
          adaptation: 0,
          collaboration: 0,
          teaching: 0
        },
        environmentalFactors: [],
        competitionLevel: 0.5,
        collaborationRequirement: 0.3
      };

      const survivors = academy.testSelectSurvivors(personas, pressure);

      expect(survivors).toHaveLength(2); // floor(3 * 0.67) = 2
      expect(survivors[0]).toBe(personas[0]); // Highest fitness
      expect(survivors[1]).toBe(personas[1]); // Second highest
    });

    it('should handle edge case with small populations', () => {
      const personas: PersonaGenome[] = [
        academy.testCreatePersonaGenome({ name: 'Only', specialization: 'typescript' })
      ];

      personas[0].evolution.fitnessScore = 0.5;

      const pressure: EvolutionaryPressure = {
        survivalRate: 0.5,
        selectionCriteria: {
          performance: 1.0,
          innovation: 0,
          adaptation: 0,
          collaboration: 0,
          teaching: 0
        },
        environmentalFactors: [],
        competitionLevel: 0.5,
        collaborationRequirement: 0.3
      };

      const survivors = academy.testSelectSurvivors(personas, pressure);
      expect(survivors).toHaveLength(0); // floor(1 * 0.5) = 0
    });

    it('should handle empty persona list', () => {
      const pressure: EvolutionaryPressure = {
        survivalRate: 0.6,
        selectionCriteria: {
          performance: 1.0,
          innovation: 0,
          adaptation: 0,
          collaboration: 0,
          teaching: 0
        },
        environmentalFactors: [],
        competitionLevel: 0.5,
        collaborationRequirement: 0.3
      };

      const survivors = academy.testSelectSurvivors([], pressure);
      expect(survivors).toHaveLength(0);
    });

    it('should preserve order of equally fit personas', () => {
      const personas: PersonaGenome[] = [
        academy.testCreatePersonaGenome({ name: 'First', specialization: 'typescript' }),
        academy.testCreatePersonaGenome({ name: 'Second', specialization: 'testing' }),
        academy.testCreatePersonaGenome({ name: 'Third', specialization: 'architecture' })
      ];

      // Set equal fitness scores
      personas[0].evolution.fitnessScore = 0.8;
      personas[1].evolution.fitnessScore = 0.8;
      personas[2].evolution.fitnessScore = 0.8;

      const pressure: EvolutionaryPressure = {
        survivalRate: 0.67,
        selectionCriteria: {
          performance: 1.0,
          innovation: 0,
          adaptation: 0,
          collaboration: 0,
          teaching: 0
        },
        environmentalFactors: [],
        competitionLevel: 0.5,
        collaborationRequirement: 0.3
      };

      const survivors = academy.testSelectSurvivors(personas, pressure);
      expect(survivors).toHaveLength(2);
      // Order should be preserved for equal fitness
      expect(survivors[0]).toBe(personas[0]);
      expect(survivors[1]).toBe(personas[1]);
    });
  });

  describe('Offspring Reproduction', () => {
    it('should create offspring from survivor population', async () => {
      const survivors: PersonaGenome[] = [
        academy.testCreatePersonaGenome({ name: 'Parent1', specialization: 'typescript' }),
        academy.testCreatePersonaGenome({ name: 'Parent2', specialization: 'testing' })
      ];

      const offspring = await academy.testReproduceOffspring(survivors);

      expect(offspring.length).toBe(3); // floor(2 * 1.5) = 3
      
      offspring.forEach(child => {
        expect(child.identity.name).toContain('_child');
        expect(child.evolution.generation).toBe(1);
        expect(child.evolution.parentGenomes).toHaveLength(2);
      });
    });

    it('should handle single survivor reproduction', async () => {
      const survivors: PersonaGenome[] = [
        academy.testCreatePersonaGenome({ name: 'OnlyParent', specialization: 'typescript' })
      ];

      const offspring = await academy.testReproduceOffspring(survivors);

      expect(offspring.length).toBe(1); // floor(1 * 1.5) = 1
      expect(offspring[0].identity.name).toContain('_child');
    });

    it('should handle empty survivor population', async () => {
      const offspring = await academy.testReproduceOffspring([]);
      expect(offspring).toHaveLength(0);
    });

    it('should create valid offspring genomes', async () => {
      const survivors: PersonaGenome[] = [
        academy.testCreatePersonaGenome({ name: 'Parent1', specialization: 'typescript' }),
        academy.testCreatePersonaGenome({ name: 'Parent2', specialization: 'testing' })
      ];

      const offspring = await academy.testReproduceOffspring(survivors);

      offspring.forEach(child => {
        expect(child.id).toBeDefined();
        expect(child.identity).toBeDefined();
        expect(child.knowledge).toBeDefined();
        expect(child.behavior).toBeDefined();
        expect(child.evolution).toBeDefined();
        expect(child.substrate).toBeDefined();
        expect(child.reproduction).toBeDefined();
        expect(child.lineage).toBeDefined();
      });
    });
  });

  describe('Simple Offspring Creation', () => {
    it('should create offspring with traits from both parents', () => {
      const parent1 = academy.testCreatePersonaGenome({ name: 'Parent1', specialization: 'typescript' });
      const parent2 = academy.testCreatePersonaGenome({ name: 'Parent2', specialization: 'testing' });

      parent1.evolution.generation = 1;
      parent2.evolution.generation = 2;

      const offspring = academy.testSimpleOffspring(parent1, parent2);

      expect(offspring.evolution.generation).toBe(3); // max(1, 2) + 1
      expect(offspring.evolution.parentGenomes).toEqual([parent1.id, parent2.id]);
      expect(offspring.lineage.ancestors).toContain(parent1.id);
      expect(offspring.lineage.ancestors).toContain(parent2.id);
      expect(offspring.identity.name).toContain('_child');
    });

    it('should inherit specialization from one parent', () => {
      const parent1 = academy.testCreatePersonaGenome({ name: 'Parent1', specialization: 'typescript' });
      const parent2 = academy.testCreatePersonaGenome({ name: 'Parent2', specialization: 'testing' });

      const offspring = academy.testSimpleOffspring(parent1, parent2);

      expect(['typescript', 'testing']).toContain(offspring.identity.specialization);
    });

    it('should start offspring as students', () => {
      const parent1 = academy.testCreatePersonaGenome({ name: 'Parent1', specialization: 'typescript' });
      const parent2 = academy.testCreatePersonaGenome({ name: 'Parent2', specialization: 'testing' });

      parent1.identity.role = 'teacher';
      parent2.identity.role = 'meta-teacher';

      const offspring = academy.testSimpleOffspring(parent1, parent2);

      expect(offspring.identity.role).toBe('student');
    });
  });

  describe('Ecosystem Management', () => {
    it('should update ecosystem with new persona population', () => {
      const personas = [
        academy.testCreatePersonaGenome({ name: 'Test1', specialization: 'typescript' }),
        academy.testCreatePersonaGenome({ name: 'Test2', specialization: 'testing' })
      ];

      academy.testUpdateEcosystem(personas);

      expect(academy.getAllPersonas()).toHaveLength(2);
      expect(academy.getPersona(personas[0].id)).toEqual(personas[0]);
      expect(academy.getPersona(personas[1].id)).toEqual(personas[1]);
    });

    it('should clear previous personas when updating', async () => {
      // Add initial persona
      await academy.spawnPersona({ name: 'Initial', specialization: 'typescript' });
      expect(academy.getAllPersonas()).toHaveLength(1);

      // Update with new personas
      const newPersonas = [
        academy.testCreatePersonaGenome({ name: 'New', specialization: 'testing' })
      ];
      
      academy.testUpdateEcosystem(newPersonas);

      expect(academy.getAllPersonas()).toHaveLength(1);
      expect(academy.getAllPersonas()[0].identity.name).toBe('New');
    });

    it('should handle empty ecosystem update', () => {
      academy.testUpdateEcosystem([]);
      expect(academy.getAllPersonas()).toHaveLength(0);
    });
  });

  describe('Ecosystem Health Calculation', () => {
    it('should calculate ecosystem health metrics', async () => {
      // Create diverse persona population
      await academy.spawnPersona({ name: 'TypeScript1', specialization: 'typescript' });
      await academy.spawnPersona({ name: 'Testing1', specialization: 'testing' });
      await academy.spawnPersona({ name: 'Architecture1', specialization: 'architecture' });

      const health = academy.testCalculateEcosystemHealth();

      expect(health.diversity).toBeGreaterThan(0);
      expect(health.innovation).toBeGreaterThanOrEqual(0);
      expect(health.collaboration).toBe(0.7); // Default value
      expect(health.sustainability).toBe(1); // 1 - 0 (no extinctions)
      expect(health.growth).toBeGreaterThanOrEqual(0);
    });

    it('should handle empty ecosystem health calculation', () => {
      const health = academy.testCalculateEcosystemHealth();

      expect(health.diversity).toBe(0);
      expect(health.innovation).toBe(0);
      expect(health.collaboration).toBe(0.7);
      expect(health.sustainability).toBe(1);
      expect(health.growth).toBe(0);
    });
  });

  describe('Session Management', () => {
    it('should create sandboxed session for persona', async () => {
      const persona = await academy.spawnPersona({ name: 'Test', specialization: 'typescript' });
      const sessionId = await academy.createSandboxedSession(persona);

      expect(sessionId).toBeDefined();
      expect(typeof sessionId).toBe('string');
      expect(sessionId.length).toBeGreaterThan(0);
      expect(academy.getTestSessions().has(sessionId)).toBe(true);
    });

    it('should store session data correctly', async () => {
      const persona = await academy.spawnPersona({ name: 'Test', specialization: 'typescript' });
      const sessionId = await academy.createSandboxedSession(persona);

      const session = academy.getTestSessions().get(sessionId);
      expect(session).toBeDefined();
      expect(session.persona).toEqual(persona);
      expect(session.created).toBeTypeOf('number');
      expect(session.challenges).toEqual([]);
      expect(session.results).toEqual([]);
    });

    it('should cleanup session resources', async () => {
      const persona = await academy.spawnPersona({ name: 'Test', specialization: 'typescript' });
      const sessionId = await academy.createSandboxedSession(persona);

      expect(academy.getTestSessions().has(sessionId)).toBe(true);

      await academy.cleanupSession(sessionId);
      expect(academy.getTestSessions().has(sessionId)).toBe(false);
    });

    it('should execute challenge in sandbox', async () => {
      const persona = await academy.spawnPersona({ name: 'Test', specialization: 'typescript' });
      const sessionId = await academy.createSandboxedSession(persona);

      const challenge = { 
        id: 'test-challenge', 
        difficulty: 0.5,
        domain: 'typescript',
        prompt: 'Test challenge prompt'
      };
      
      const result = await academy.executeChallengeInSandbox(sessionId, challenge);

      expect(result.sessionId).toBe(sessionId);
      expect(result.challengeId).toBe('test-challenge');
      expect(typeof result.success).toBe('boolean');
      expect(typeof result.accuracy).toBe('number');
      expect(typeof result.timeUsed).toBe('number');
      expect(result.accuracy).toBeGreaterThanOrEqual(0.2);
      expect(result.accuracy).toBeLessThanOrEqual(1.0);
      expect(result.timeUsed).toBeGreaterThanOrEqual(1000);
      expect(result.timeUsed).toBeLessThanOrEqual(11000);
    });

    it('should throw error for non-existent session', async () => {
      const challenge = { id: 'test-challenge', difficulty: 0.5 };
      
      await expect(academy.executeChallengeInSandbox('non-existent', challenge))
        .rejects.toThrow('Session not found: non-existent');
    });

    it('should track challenges and results in session', async () => {
      const persona = await academy.spawnPersona({ name: 'Test', specialization: 'typescript' });
      const sessionId = await academy.createSandboxedSession(persona);

      const challenge = { id: 'test-challenge', difficulty: 0.5 };
      await academy.executeChallengeInSandbox(sessionId, challenge);

      const session = academy.getTestSessions().get(sessionId);
      expect(session.challenges).toHaveLength(1);
      expect(session.results).toHaveLength(1);
      expect(session.challenges[0]).toEqual(challenge);
    });
  });

  describe('Message Handling', () => {
    it('should send and log messages', async () => {
      const message = { type: 'test', data: 'test message' };
      const response = await academy.sendMessage(message);

      expect(response.status).toBe('ok');
      expect(response.echo).toEqual(message);
      expect(response.timestamp).toBeTypeOf('number');
    });

    it('should log all sent messages', async () => {
      const message1 = { type: 'test1', data: 'message1' };
      const message2 = { type: 'test2', data: 'message2' };

      await academy.sendMessage(message1);
      await academy.sendMessage(message2);

      const log = academy.getMockMessageLog();
      expect(log).toHaveLength(2);
      expect(log[0]).toEqual(message1);
      expect(log[1]).toEqual(message2);
    });
  });

  describe('Lineage Tracking', () => {
    it('should track persona lineage correctly', async () => {
      const persona = await academy.spawnPersona({ name: 'Test', specialization: 'typescript' });
      const lineage = academy.getPersonaLineage(persona.id);

      expect(lineage).toBeDefined();
      expect(lineage!.personaId).toBe(persona.id);
      expect(lineage!.lineage).toBe(persona.lineage);
      expect(lineage!.evolution).toBe(persona.evolution);
      expect(lineage!.ancestors).toEqual([]);
      expect(lineage!.descendants).toEqual([]);
    });

    it('should return null for non-existent persona lineage', () => {
      const lineage = academy.getPersonaLineage('non-existent');
      expect(lineage).toBeNull();
    });

    it('should build lineage tree for generation 0 personas', async () => {
      const persona1 = await academy.spawnPersona({ name: 'Gen0-1', specialization: 'typescript' });
      const persona2 = await academy.spawnPersona({ name: 'Gen0-2', specialization: 'testing' });

      const tree = academy.getLineageTree();
      expect(tree).toHaveLength(2);
      expect(tree[0].generation).toBe(0);
      expect(tree[1].generation).toBe(0);
      expect(tree[0].descendants).toEqual([]);
      expect(tree[1].descendants).toEqual([]);
    });

    it('should build empty lineage tree when no personas exist', () => {
      const tree = academy.getLineageTree();
      expect(tree).toEqual([]);
    });
  });

  describe('Status and Metrics', () => {
    it('should return current academy status', () => {
      const status = academy.getAcademyStatus();

      expect(status.mode).toBe('idle');
      expect(status.isActive).toBe(false);
      expect(status.totalPersonas).toBe(0);
      expect(status.activeSessions).toBe(0);
      expect(status.uptime).toBeTypeOf('number');
      expect(status.ecosystemMetrics).toBeDefined();
    });

    it('should update status after spawning personas', async () => {
      const initialStatus = academy.getAcademyStatus();
      expect(initialStatus.totalPersonas).toBe(0);

      await academy.spawnPersona({ name: 'Test1', specialization: 'typescript' });
      await academy.spawnPersona({ name: 'Test2', specialization: 'testing' });

      const updatedStatus = academy.getAcademyStatus();
      expect(updatedStatus.totalPersonas).toBe(2);
    });

    it('should maintain consistent uptime', async () => {
      const status1 = academy.getAcademyStatus();
      const uptime1 = status1.uptime;

      // Small delay
      await new Promise(resolve => setTimeout(resolve, 10));

      const status2 = academy.getAcademyStatus();
      const uptime2 = status2.uptime;

      expect(uptime2).toBe(uptime1); // Should remain the same (creation time)
    });
  });

  describe('Evolution Process Integration', () => {
    it('should run complete evolution cycle', async () => {
      const config: EvolutionConfig = {
        generations: 2,
        populationSize: 4,
        evolutionaryPressure: {
          survivalRate: 0.5,
          selectionCriteria: {
            performance: 0.4,
            innovation: 0.2,
            adaptation: 0.2,
            collaboration: 0.15,
            teaching: 0.05
          },
          environmentalFactors: ['competition'],
          competitionLevel: 0.5,
          collaborationRequirement: 0.3
        }
      };

      const result = await academy.startEvolution(config);

      expect(result.success).toBe(true);
      expect(result.generationsCompleted).toBe(2);
      expect(result.finalPopulation).toBeGreaterThan(0);
      expect(result.ecosystemMetrics).toBeDefined();
      expect(result.ecosystemHealth).toBeDefined();
    });

    it('should handle evolution errors gracefully', async () => {
      const config: EvolutionConfig = {
        generations: 0, // Invalid
        populationSize: 5,
        evolutionaryPressure: {} as any
      };

      await expect(academy.startEvolution(config)).rejects.toThrow();
    });

    it('should maintain ecosystem during evolution', async () => {
      const config: EvolutionConfig = {
        generations: 1,
        populationSize: 3,
        evolutionaryPressure: {
          survivalRate: 0.7,
          selectionCriteria: {
            performance: 1.0,
            innovation: 0,
            adaptation: 0,
            collaboration: 0,
            teaching: 0
          },
          environmentalFactors: [],
          competitionLevel: 0.5,
          collaborationRequirement: 0.3
        }
      };

      const result = await academy.startEvolution(config);

      expect(result.success).toBe(true);
      expect(academy.getAllPersonas().length).toBeGreaterThan(0);
      
      const status = academy.getAcademyStatus();
      expect(status.mode).toBe('idle'); // Should return to idle after evolution
    });
  });

  describe('Error Handling', () => {
    it('should handle invalid persona spawn configuration', async () => {
      const invalidConfig = {
        name: '',
        specialization: 'invalid_specialization'
      } as PersonaSpawnConfig;

      // Should handle gracefully - the implementation should validate
      const persona = await academy.spawnPersona(invalidConfig);
      expect(persona).toBeDefined();
      expect(persona.identity.name).toBe(''); // Empty name should be preserved
    });

    it('should handle multiple session cleanup calls', async () => {
      const persona = await academy.spawnPersona({ name: 'Test', specialization: 'typescript' });
      const sessionId = await academy.createSandboxedSession(persona);

      // First cleanup
      await academy.cleanupSession(sessionId);
      expect(academy.getTestSessions().has(sessionId)).toBe(false);

      // Second cleanup should not throw
      await expect(academy.cleanupSession(sessionId)).resolves.not.toThrow();
    });

    it('should handle concurrent session creation', async () => {
      const persona = await academy.spawnPersona({ name: 'Test', specialization: 'typescript' });

      const sessionPromises = Array(5).fill(null).map(() => 
        academy.createSandboxedSession(persona)
      );

      const sessionIds = await Promise.all(sessionPromises);
      
      expect(sessionIds).toHaveLength(5);
      expect(new Set(sessionIds).size).toBe(5); // All unique
    });
  });
});

// Test utilities for other test files
export function createTestPersona(overrides: Partial<PersonaSpawnConfig> = {}): PersonaSpawnConfig {
  return {
    name: 'TestPersona',
    specialization: 'typescript',
    role: 'student',
    ...overrides
  };
}

export function createTestEvolutionConfig(overrides: Partial<EvolutionConfig> = {}): EvolutionConfig {
  return {
    generations: 3,
    populationSize: 5,
    evolutionaryPressure: {
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
      collaborationRequirement: 0.3
    },
    ...overrides
  };
}

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