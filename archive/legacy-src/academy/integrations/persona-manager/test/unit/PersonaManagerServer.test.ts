/**
 * Unit tests for PersonaManagerServer
 * 
 * These tests validate the complete persona management functionality:
 * - CRUD operations for persona genomes
 * - Validation and error handling
 * - Crossover and mutation operations
 * - File system persistence
 * - Statistics and analytics
 * 
 * Test Coverage:
 * - Persona creation and validation
 * - Search and filtering operations
 * - Update and mutation tracking
 * - Crossover breeding system
 * - Export/import functionality
 * - Statistics generation
 * - Error handling and edge cases
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { PersonaManagerServer } from '../PersonaManagerServer';
import { 
  PersonaGenome,
  PersonaSpawnConfig,
  generateUUID,
  SPECIALIZATIONS
} from '../../../shared/AcademyTypes';
import {
  CreatePersonaRequest,
  PersonaSearchRequest,
  PersonaUpdateRequest,
  PersonaCrossoverRequest,
  PersonaValidationRequest,
  PersonaStatisticsRequest,
  PersonaExportRequest,
  PersonaImportRequest,
  PersonaFilters,
  PersonaSortField,
  PERSONA_MANAGER_CONSTANTS
} from '../shared/PersonaManagerTypes';
import {
  createTestPersonaGenome,
  createTestPersonaPopulation,
  createTestPersonaConfig,
  createMockFileSystem,
  assertValidPersonaGenome,
  setupTestEnvironment,
  cleanupTestResources
} from '../../../test/shared/TestUtilities';
import * as fs from 'fs/promises';
import * as path from 'path';

describe('PersonaManagerServer', () => {
  let personaManager: PersonaManagerServer;
  let mockFs: any;
  let tempDir: string;

  beforeEach(async () => {
    setupTestEnvironment();
    
    // Setup mock file system
    mockFs = createMockFileSystem();
    
    // Mock fs operations
    vi.spyOn(fs, 'writeFile').mockImplementation(mockFs.writeFile);
    vi.spyOn(fs, 'readFile').mockImplementation(mockFs.readFile);
    vi.spyOn(fs, 'mkdir').mockImplementation(mockFs.mkdir);
    vi.spyOn(fs, 'rm').mockImplementation(mockFs.rm);
    vi.spyOn(fs, 'access').mockImplementation(mockFs.access);
    vi.spyOn(fs, 'readdir').mockImplementation(mockFs.readdir);
    
    tempDir = '/tmp/persona-manager-test';
    personaManager = new PersonaManagerServer(tempDir);
    
    // Initialize persona manager
    await personaManager.initialize();
  });

  afterEach(async () => {
    cleanupTestResources();
    mockFs.clearFiles();
  });

  describe('Persona Creation', () => {
    it('should create persona with valid configuration', async () => {
      const request: CreatePersonaRequest = {
        config: createTestPersonaConfig({
          name: 'TestCreationPersona',
          specialization: 'typescript',
          role: 'student'
        }),
        parentGenomes: [],
        mutationRate: 0.1
      };

      const result = await personaManager.createPersona(request);

      expect(result.success).toBe(true);
      expect(result.persona).toBeDefined();
      
      if (result.persona) {
        assertValidPersonaGenome(result.persona);
        expect(result.persona.identity.name).toBe('TestCreationPersona');
        expect(result.persona.identity.specialization).toBe('typescript');
        expect(result.persona.identity.role).toBe('student');
      }
    });

    it('should create persona with parent genomes (crossover)', async () => {
      const parent1 = createTestPersonaGenome({
        identity: { 
          name: 'Parent1',
          specialization: 'typescript',
          role: 'teacher',
          generation: 1,
          personality: {
            creativity: 0.8,
            analytical: 0.6,
            helpfulness: 0.7,
            competitiveness: 0.5,
            patience: 0.9,
            innovation: 0.4
          },
          goals: ['master_typescript', 'teach_effectively']
        }
      });

      const parent2 = createTestPersonaGenome({
        identity: { 
          name: 'Parent2',
          specialization: 'react',
          role: 'student',
          generation: 1,
          personality: {
            creativity: 0.6,
            analytical: 0.8,
            helpfulness: 0.5,
            competitiveness: 0.7,
            patience: 0.4,
            innovation: 0.9
          },
          goals: ['master_react', 'collaborate_effectively']
        }
      });

      const request: CreatePersonaRequest = {
        config: createTestPersonaConfig({
          name: 'CrossoverChild',
          specialization: 'fullstack',
          role: 'student'
        }),
        parentGenomes: [parent1, parent2],
        mutationRate: 0.05
      };

      const result = await personaManager.createPersona(request);

      expect(result.success).toBe(true);
      expect(result.persona).toBeDefined();
      
      if (result.persona) {
        assertValidPersonaGenome(result.persona);
        expect(result.persona.identity.name).toBe('CrossoverChild');
        expect(result.persona.evolution.parentGenomes).toHaveLength(2);
        expect(result.persona.evolution.parentGenomes).toContain(parent1.id);
        expect(result.persona.evolution.parentGenomes).toContain(parent2.id);
        expect(result.persona.evolution.generation).toBe(2);
        
        // Verify personality traits are blended
        const personality = result.persona.identity.personality;
        expect(personality.creativity).toBeGreaterThan(0);
        expect(personality.creativity).toBeLessThan(1);
        expect(personality.analytical).toBeGreaterThan(0);
        expect(personality.analytical).toBeLessThan(1);
      }
    });

    it('should handle invalid persona configuration', async () => {
      const invalidRequest: CreatePersonaRequest = {
        config: {
          name: '', // Invalid empty name
          specialization: 'invalid_specialization' as any,
          role: 'invalid_role' as any
        },
        parentGenomes: [],
        mutationRate: 2.0 // Invalid mutation rate > 1
      };

      const result = await personaManager.createPersona(invalidRequest);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      expect(result.error).toContain('Invalid');
    });

    it('should persist created persona to file system', async () => {
      const request: CreatePersonaRequest = {
        config: createTestPersonaConfig({
          name: 'PersistenceTest',
          specialization: 'python'
        }),
        parentGenomes: [],
        mutationRate: 0.1
      };

      const result = await personaManager.createPersona(request);

      expect(result.success).toBe(true);
      expect(result.persona).toBeDefined();
      
      if (result.persona) {
        const filePath = path.join(tempDir, 'personas', `${result.persona.id}.json`);
        expect(mockFs.files.has(filePath)).toBe(true);
        
        const savedData = JSON.parse(mockFs.files.get(filePath)!);
        expect(savedData.id).toBe(result.persona.id);
        expect(savedData.identity.name).toBe('PersistenceTest');
      }
    });
  });

  describe('Persona Search and Retrieval', () => {
    beforeEach(async () => {
      // Create test population
      const testPopulation = createTestPersonaPopulation(10);
      for (const persona of testPopulation) {
        await personaManager.createPersona({
          config: {
            name: persona.identity.name,
            specialization: persona.identity.specialization,
            role: persona.identity.role
          },
          parentGenomes: [],
          mutationRate: 0.1
        });
      }
    });

    it('should search personas by specialization', async () => {
      const request: PersonaSearchRequest = {
        filters: {
          specialization: 'typescript'
        },
        sortBy: 'fitness',
        sortOrder: 'desc',
        limit: 5
      };

      const result = await personaManager.searchPersonas(request);

      expect(result.success).toBe(true);
      expect(result.personas).toBeDefined();
      expect(Array.isArray(result.personas)).toBe(true);
      
      if (result.personas) {
        result.personas.forEach(persona => {
          expect(persona.identity.specialization).toBe('typescript');
        });
      }
    });

    it('should search personas by role', async () => {
      const request: PersonaSearchRequest = {
        filters: {
          role: 'teacher'
        },
        sortBy: 'generation',
        sortOrder: 'asc',
        limit: 3
      };

      const result = await personaManager.searchPersonas(request);

      expect(result.success).toBe(true);
      expect(result.personas).toBeDefined();
      
      if (result.personas) {
        result.personas.forEach(persona => {
          expect(persona.identity.role).toBe('teacher');
        });
      }
    });

    it('should search personas by fitness range', async () => {
      const request: PersonaSearchRequest = {
        filters: {
          fitnessRange: {
            min: 0.5,
            max: 0.8
          }
        },
        sortBy: 'fitness',
        sortOrder: 'desc',
        limit: 10
      };

      const result = await personaManager.searchPersonas(request);

      expect(result.success).toBe(true);
      expect(result.personas).toBeDefined();
      
      if (result.personas) {
        result.personas.forEach(persona => {
          expect(persona.evolution.fitnessScore).toBeGreaterThanOrEqual(0.5);
          expect(persona.evolution.fitnessScore).toBeLessThanOrEqual(0.8);
        });
      }
    });

    it('should search personas by generation', async () => {
      const request: PersonaSearchRequest = {
        filters: {
          generation: 0
        },
        sortBy: 'name',
        sortOrder: 'asc',
        limit: 20
      };

      const result = await personaManager.searchPersonas(request);

      expect(result.success).toBe(true);
      expect(result.personas).toBeDefined();
      
      if (result.personas) {
        result.personas.forEach(persona => {
          expect(persona.evolution.generation).toBe(0);
        });
      }
    });

    it('should handle empty search results', async () => {
      const request: PersonaSearchRequest = {
        filters: {
          specialization: 'nonexistent_specialization' as any
        },
        sortBy: 'fitness',
        sortOrder: 'desc',
        limit: 10
      };

      const result = await personaManager.searchPersonas(request);

      expect(result.success).toBe(true);
      expect(result.personas).toBeDefined();
      expect(result.personas).toHaveLength(0);
    });
  });

  describe('Persona Updates', () => {
    let testPersona: PersonaGenome;

    beforeEach(async () => {
      const createResult = await personaManager.createPersona({
        config: createTestPersonaConfig({
          name: 'UpdateTestPersona',
          specialization: 'typescript'
        }),
        parentGenomes: [],
        mutationRate: 0.1
      });

      if (createResult.success && createResult.persona) {
        testPersona = createResult.persona;
      }
    });

    it('should update persona fitness score', async () => {
      const request: PersonaUpdateRequest = {
        personaId: testPersona.id,
        updates: {
          fitnessScore: 0.85
        }
      };

      const result = await personaManager.updatePersona(request);

      expect(result.success).toBe(true);
      expect(result.persona).toBeDefined();
      
      if (result.persona) {
        expect(result.persona.evolution.fitnessScore).toBe(0.85);
      }
    });

    it('should update persona competencies', async () => {
      const request: PersonaUpdateRequest = {
        personaId: testPersona.id,
        updates: {
          competencies: {
            'typescript': 0.9,
            'react': 0.7,
            'node': 0.8
          }
        }
      };

      const result = await personaManager.updatePersona(request);

      expect(result.success).toBe(true);
      expect(result.persona).toBeDefined();
      
      if (result.persona) {
        expect(result.persona.knowledge.competencies['typescript']).toBe(0.9);
        expect(result.persona.knowledge.competencies['react']).toBe(0.7);
        expect(result.persona.knowledge.competencies['node']).toBe(0.8);
      }
    });

    it('should update persona personality traits', async () => {
      const request: PersonaUpdateRequest = {
        personaId: testPersona.id,
        updates: {
          personality: {
            creativity: 0.8,
            analytical: 0.9
          }
        }
      };

      const result = await personaManager.updatePersona(request);

      expect(result.success).toBe(true);
      expect(result.persona).toBeDefined();
      
      if (result.persona) {
        expect(result.persona.identity.personality.creativity).toBe(0.8);
        expect(result.persona.identity.personality.analytical).toBe(0.9);
      }
    });

    it('should handle update of nonexistent persona', async () => {
      const request: PersonaUpdateRequest = {
        personaId: 'nonexistent-id',
        updates: {
          fitnessScore: 0.5
        }
      };

      const result = await personaManager.updatePersona(request);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      expect(result.error).toContain('not found');
    });

    it('should persist updates to file system', async () => {
      const request: PersonaUpdateRequest = {
        personaId: testPersona.id,
        updates: {
          fitnessScore: 0.95
        }
      };

      const result = await personaManager.updatePersona(request);

      expect(result.success).toBe(true);
      
      const filePath = path.join(tempDir, 'personas', `${testPersona.id}.json`);
      expect(mockFs.files.has(filePath)).toBe(true);
      
      const savedData = JSON.parse(mockFs.files.get(filePath)!);
      expect(savedData.evolution.fitnessScore).toBe(0.95);
    });
  });

  describe('Persona Crossover', () => {
    let parent1: PersonaGenome;
    let parent2: PersonaGenome;

    beforeEach(async () => {
      const result1 = await personaManager.createPersona({
        config: createTestPersonaConfig({
          name: 'CrossoverParent1',
          specialization: 'typescript',
          role: 'teacher'
        }),
        parentGenomes: [],
        mutationRate: 0.1
      });

      const result2 = await personaManager.createPersona({
        config: createTestPersonaConfig({
          name: 'CrossoverParent2',
          specialization: 'react',
          role: 'student'
        }),
        parentGenomes: [],
        mutationRate: 0.1
      });

      if (result1.success && result1.persona) parent1 = result1.persona;
      if (result2.success && result2.persona) parent2 = result2.persona;
    });

    it('should perform crossover between two personas', async () => {
      const request: PersonaCrossoverRequest = {
        parentIds: [parent1.id, parent2.id],
        crossoverWeights: {
          identity: 0.5,
          knowledge: 0.6,
          behavior: 0.4,
          substrate: 0.5
        },
        mutationRate: 0.05,
        childName: 'CrossoverChild'
      };

      const result = await personaManager.performCrossover(request);

      expect(result.success).toBe(true);
      expect(result.child).toBeDefined();
      expect(result.report).toBeDefined();
      
      if (result.child) {
        assertValidPersonaGenome(result.child);
        expect(result.child.identity.name).toBe('CrossoverChild');
        expect(result.child.evolution.parentGenomes).toHaveLength(2);
        expect(result.child.evolution.parentGenomes).toContain(parent1.id);
        expect(result.child.evolution.parentGenomes).toContain(parent2.id);
        expect(result.child.evolution.generation).toBe(1);
      }

      if (result.report) {
        expect(result.report.parentIds).toEqual([parent1.id, parent2.id]);
        expect(result.report.childId).toBe(result.child?.id);
        expect(result.report.traitsInherited).toBeDefined();
        expect(result.report.mutationsApplied).toBeDefined();
      }
    });

    it('should handle crossover with custom weights', async () => {
      const request: PersonaCrossoverRequest = {
        parentIds: [parent1.id, parent2.id],
        crossoverWeights: {
          identity: 0.8, // Favor parent1 identity
          knowledge: 0.2, // Favor parent2 knowledge
          behavior: 0.6,
          substrate: 0.3
        },
        mutationRate: 0.1,
        childName: 'WeightedCrossoverChild'
      };

      const result = await personaManager.performCrossover(request);

      expect(result.success).toBe(true);
      expect(result.child).toBeDefined();
      
      if (result.child) {
        // Should inherit more traits from parent1 for identity (0.8 weight)
        // Should inherit more traits from parent2 for knowledge (0.2 weight)
        assertValidPersonaGenome(result.child);
      }
    });

    it('should handle invalid crossover requests', async () => {
      const request: PersonaCrossoverRequest = {
        parentIds: ['invalid-id-1', 'invalid-id-2'],
        crossoverWeights: {
          identity: 0.5,
          knowledge: 0.5,
          behavior: 0.5,
          substrate: 0.5
        },
        mutationRate: 0.1,
        childName: 'InvalidCrossover'
      };

      const result = await personaManager.performCrossover(request);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });
  });

  describe('Persona Validation', () => {
    it('should validate valid persona genome', async () => {
      const validPersona = createTestPersonaGenome();
      
      const request: PersonaValidationRequest = {
        persona: validPersona,
        strict: true
      };

      const result = await personaManager.validatePersona(request);

      expect(result.success).toBe(true);
      expect(result.isValid).toBe(true);
      expect(result.errors).toHaveLength(0);
      expect(result.warnings).toHaveLength(0);
    });

    it('should detect persona validation errors', async () => {
      const invalidPersona = createTestPersonaGenome({
        identity: {
          name: '', // Invalid empty name
          role: 'invalid_role' as any,
          generation: -1, // Invalid negative generation
          specialization: 'typescript',
          personality: {
            creativity: 1.5, // Invalid > 1
            analytical: -0.1, // Invalid < 0
            helpfulness: 0.5,
            competitiveness: 0.5,
            patience: 0.5,
            innovation: 0.5
          },
          goals: []
        }
      });

      const request: PersonaValidationRequest = {
        persona: invalidPersona,
        strict: true
      };

      const result = await personaManager.validatePersona(request);

      expect(result.success).toBe(true);
      expect(result.isValid).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
      
      // Check for specific validation errors
      const errorMessages = result.errors.map(e => e.message);
      expect(errorMessages.some(msg => msg.includes('name'))).toBe(true);
      expect(errorMessages.some(msg => msg.includes('role'))).toBe(true);
      expect(errorMessages.some(msg => msg.includes('generation'))).toBe(true);
      expect(errorMessages.some(msg => msg.includes('creativity'))).toBe(true);
      expect(errorMessages.some(msg => msg.includes('analytical'))).toBe(true);
    });

    it('should detect persona validation warnings', async () => {
      const warningPersona = createTestPersonaGenome({
        evolution: {
          generation: 0,
          parentGenomes: [],
          mutationHistory: [],
          evolutionStage: 'spawning',
          fitnessScore: 0.1, // Very low fitness (warning)
          adaptationSuccess: 0,
          survivalRounds: 0,
          evolutionPressure: []
        },
        knowledge: {
          domain: 'typescript',
          expertise: ['typescript'],
          competencies: {
            'typescript': 0.05 // Very low competency (warning)
          },
          experiencePoints: 0
        }
      });

      const request: PersonaValidationRequest = {
        persona: warningPersona,
        strict: false
      };

      const result = await personaManager.validatePersona(request);

      expect(result.success).toBe(true);
      expect(result.isValid).toBe(true);
      expect(result.warnings.length).toBeGreaterThan(0);
      
      // Check for specific warnings
      const warningMessages = result.warnings.map(w => w.message);
      expect(warningMessages.some(msg => msg.includes('fitness'))).toBe(true);
      expect(warningMessages.some(msg => msg.includes('competency'))).toBe(true);
    });
  });

  describe('Statistics and Analytics', () => {
    beforeEach(async () => {
      // Create diverse test population
      const testPopulation = createTestPersonaPopulation(20);
      for (const persona of testPopulation) {
        await personaManager.createPersona({
          config: {
            name: persona.identity.name,
            specialization: persona.identity.specialization,
            role: persona.identity.role
          },
          parentGenomes: [],
          mutationRate: 0.1
        });
      }
    });

    it('should generate population statistics', async () => {
      const request: PersonaStatisticsRequest = {
        includePopulationStats: true,
        includeFitnessDistribution: true,
        includeSpecializationBreakdown: true,
        includeEvolutionTrends: true,
        generationRange: { min: 0, max: 5 }
      };

      const result = await personaManager.getStatistics(request);

      expect(result.success).toBe(true);
      expect(result.statistics).toBeDefined();
      
      if (result.statistics) {
        expect(result.statistics.totalPersonas).toBeGreaterThan(0);
        expect(result.statistics.averageFitness).toBeGreaterThan(0);
        expect(result.statistics.averageFitness).toBeLessThan(1);
        expect(result.statistics.generationDistribution).toBeDefined();
        expect(result.statistics.specializationBreakdown).toBeDefined();
        expect(result.statistics.fitnessDistribution).toBeDefined();
        expect(result.statistics.evolutionTrends).toBeDefined();
      }
    });

    it('should generate fitness distribution', async () => {
      const request: PersonaStatisticsRequest = {
        includePopulationStats: true,
        includeFitnessDistribution: true,
        includeSpecializationBreakdown: false,
        includeEvolutionTrends: false
      };

      const result = await personaManager.getStatistics(request);

      expect(result.success).toBe(true);
      expect(result.statistics?.fitnessDistribution).toBeDefined();
      
      if (result.statistics?.fitnessDistribution) {
        const distribution = result.statistics.fitnessDistribution;
        expect(distribution.bins).toBeDefined();
        expect(distribution.bins.length).toBeGreaterThan(0);
        expect(distribution.average).toBeGreaterThan(0);
        expect(distribution.median).toBeGreaterThan(0);
        expect(distribution.standardDeviation).toBeGreaterThanOrEqual(0);
      }
    });

    it('should generate specialization breakdown', async () => {
      const request: PersonaStatisticsRequest = {
        includePopulationStats: false,
        includeFitnessDistribution: false,
        includeSpecializationBreakdown: true,
        includeEvolutionTrends: false
      };

      const result = await personaManager.getStatistics(request);

      expect(result.success).toBe(true);
      expect(result.statistics?.specializationBreakdown).toBeDefined();
      
      if (result.statistics?.specializationBreakdown) {
        const breakdown = result.statistics.specializationBreakdown;
        expect(typeof breakdown).toBe('object');
        
        Object.keys(breakdown).forEach(specialization => {
          expect(SPECIALIZATIONS.includes(specialization)).toBe(true);
          expect(breakdown[specialization]).toBeGreaterThan(0);
        });
      }
    });
  });

  describe('Export and Import', () => {
    let testPersonas: PersonaGenome[];

    beforeEach(async () => {
      testPersonas = [];
      for (let i = 0; i < 5; i++) {
        const result = await personaManager.createPersona({
          config: createTestPersonaConfig({
            name: `ExportTestPersona${i + 1}`,
            specialization: SPECIALIZATIONS[i % SPECIALIZATIONS.length]
          }),
          parentGenomes: [],
          mutationRate: 0.1
        });
        
        if (result.success && result.persona) {
          testPersonas.push(result.persona);
        }
      }
    });

    it('should export personas to JSON', async () => {
      const request: PersonaExportRequest = {
        personaIds: testPersonas.map(p => p.id),
        format: 'json',
        includeMetadata: true
      };

      const result = await personaManager.exportPersonas(request);

      expect(result.success).toBe(true);
      expect(result.data).toBeDefined();
      
      if (result.data) {
        const exportedData = JSON.parse(result.data);
        expect(exportedData.personas).toBeDefined();
        expect(exportedData.personas).toHaveLength(testPersonas.length);
        expect(exportedData.metadata).toBeDefined();
        expect(exportedData.metadata.exportedAt).toBeDefined();
        expect(exportedData.metadata.version).toBeDefined();
      }
    });

    it('should import personas from JSON', async () => {
      // First export
      const exportRequest: PersonaExportRequest = {
        personaIds: testPersonas.slice(0, 2).map(p => p.id),
        format: 'json',
        includeMetadata: true
      };

      const exportResult = await personaManager.exportPersonas(exportRequest);
      expect(exportResult.success).toBe(true);

      // Clear existing personas
      mockFs.clearFiles();
      await personaManager.initialize();

      // Import
      const importRequest: PersonaImportRequest = {
        data: exportResult.data!,
        format: 'json',
        overwriteExisting: true
      };

      const importResult = await personaManager.importPersonas(importRequest);

      expect(importResult.success).toBe(true);
      expect(importResult.importedCount).toBe(2);
      expect(importResult.skippedCount).toBe(0);
      expect(importResult.errorCount).toBe(0);
    });

    it('should handle import conflicts', async () => {
      const exportRequest: PersonaExportRequest = {
        personaIds: testPersonas.slice(0, 1).map(p => p.id),
        format: 'json',
        includeMetadata: true
      };

      const exportResult = await personaManager.exportPersonas(exportRequest);
      expect(exportResult.success).toBe(true);

      // Import same data (should detect conflict)
      const importRequest: PersonaImportRequest = {
        data: exportResult.data!,
        format: 'json',
        overwriteExisting: false
      };

      const importResult = await personaManager.importPersonas(importRequest);

      expect(importResult.success).toBe(true);
      expect(importResult.skippedCount).toBe(1);
      expect(importResult.importedCount).toBe(0);
    });
  });

  describe('Error Handling and Edge Cases', () => {
    it('should handle file system errors gracefully', async () => {
      // Mock file system error
      vi.spyOn(fs, 'writeFile').mockRejectedValue(new Error('Disk full'));

      const request: CreatePersonaRequest = {
        config: createTestPersonaConfig(),
        parentGenomes: [],
        mutationRate: 0.1
      };

      const result = await personaManager.createPersona(request);

      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      expect(result.error).toContain('Disk full');
    });

    it('should handle corrupted persona data', async () => {
      // Simulate corrupted data
      const corruptedPath = path.join(tempDir, 'personas', 'corrupted.json');
      mockFs.files.set(corruptedPath, '{ invalid json }');

      // Should handle gracefully during initialization
      const newManager = new PersonaManagerServer(tempDir);
      await expect(newManager.initialize()).resolves.not.toThrow();
    });

    it('should handle concurrent operations', async () => {
      const concurrentRequests = Array.from({ length: 10 }, (_, i) => 
        personaManager.createPersona({
          config: createTestPersonaConfig({
            name: `ConcurrentPersona${i + 1}`
          }),
          parentGenomes: [],
          mutationRate: 0.1
        })
      );

      const results = await Promise.all(concurrentRequests);

      // All should succeed
      results.forEach(result => {
        expect(result.success).toBe(true);
      });

      // All should have unique IDs
      const ids = results.map(r => r.persona?.id).filter(Boolean);
      const uniqueIds = new Set(ids);
      expect(uniqueIds.size).toBe(ids.length);
    });
  });
});