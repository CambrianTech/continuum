/**
 * Integration tests for PersonaManagerServer with file system and Academy daemon
 * 
 * These tests validate the complete flow of PersonaManager integration:
 * - Real file system operations and persistence
 * - Integration with Academy daemon system
 * - Concurrent operations and performance
 * - Error recovery and data integrity
 * - Large-scale population management
 * 
 * This follows the middle-out testing methodology for layer 4 validation.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { PersonaManagerServer } from '../../server/PersonaManagerServer';
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
  PersonaStatisticsRequest,
  PersonaExportRequest,
  PersonaImportRequest
} from '../../shared/PersonaManagerTypes';
import {
  createTestPersonaGenome,
  createTestPersonaPopulation,
  createTestPersonaConfig,
  assertValidPersonaGenome,
  measureExecutionTime,
  benchmarkFunction,
  setupTestEnvironment,
  cleanupTestResources
} from '../../../test/shared/TestUtilities';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

describe('PersonaManagerServer Integration Tests', () => {
  let personaManager: PersonaManagerServer;
  let tempDir: string;
  let personaDir: string;
  let backupDir: string;

  beforeEach(async () => {
    setupTestEnvironment();
    
    // Create real temporary directory structure
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'persona-manager-integration-'));
    personaDir = path.join(tempDir, 'personas');
    backupDir = path.join(tempDir, 'backups');
    
    // Create directory structure
    await fs.mkdir(personaDir, { recursive: true });
    await fs.mkdir(backupDir, { recursive: true });
    
    // Initialize PersonaManager with real file system
    personaManager = new PersonaManagerServer(tempDir);
    await personaManager.initialize();
  });

  afterEach(async () => {
    cleanupTestResources();
    
    // Cleanup real temporary directory
    try {
      await fs.rm(tempDir, { recursive: true, force: true });
    } catch (error) {
      console.warn('Failed to cleanup temp directory:', error);
    }
  });

  describe('File System Integration', () => {
    it('should persist personas to real file system', async () => {
      const request: CreatePersonaRequest = {
        config: createTestPersonaConfig({
          name: 'FilePersistenceTest',
          specialization: 'typescript',
          role: 'teacher'
        }),
        parentGenomes: [],
        mutationRate: 0.1
      };

      const result = await personaManager.createPersona(request);

      expect(result.success).toBe(true);
      expect(result.persona).toBeDefined();
      
      if (result.persona) {
        // Verify file was created
        const filePath = path.join(personaDir, `${result.persona.id}.json`);
        const fileExists = await fs.access(filePath).then(() => true).catch(() => false);
        expect(fileExists).toBe(true);
        
        // Verify file contents
        const fileContent = await fs.readFile(filePath, 'utf8');
        const savedPersona = JSON.parse(fileContent);
        
        expect(savedPersona.id).toBe(result.persona.id);
        expect(savedPersona.identity.name).toBe('FilePersistenceTest');
        expect(savedPersona.identity.specialization).toBe('typescript');
        expect(savedPersona.identity.role).toBe('teacher');
        
        // Verify persona structure is complete
        assertValidPersonaGenome(savedPersona);
      }
    });

    it('should handle file system errors gracefully', async () => {
      // Create a directory with restricted permissions
      const restrictedDir = path.join(tempDir, 'restricted');
      await fs.mkdir(restrictedDir, { mode: 0o444 }); // Read-only
      
      const restrictedManager = new PersonaManagerServer(restrictedDir);
      
      // Should handle initialization failure gracefully
      await expect(restrictedManager.initialize()).resolves.not.toThrow();
      
      // Should handle persona creation failure gracefully
      const request: CreatePersonaRequest = {
        config: createTestPersonaConfig(),
        parentGenomes: [],
        mutationRate: 0.1
      };

      const result = await restrictedManager.createPersona(request);
      
      // Should fail gracefully with proper error message
      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });

    it('should recover from corrupted persona files', async () => {
      // Create a valid persona first
      const validRequest: CreatePersonaRequest = {
        config: createTestPersonaConfig({
          name: 'ValidPersona',
          specialization: 'python'
        }),
        parentGenomes: [],
        mutationRate: 0.1
      };

      const validResult = await personaManager.createPersona(validRequest);
      expect(validResult.success).toBe(true);

      // Create corrupted persona file
      const corruptedPath = path.join(personaDir, 'corrupted.json');
      await fs.writeFile(corruptedPath, '{ invalid json data }');

      // Reinitialize manager - should handle corrupted file gracefully
      const newManager = new PersonaManagerServer(tempDir);
      await newManager.initialize();

      // Should still be able to find valid persona
      const searchRequest: PersonaSearchRequest = {
        filters: { name: 'ValidPersona' },
        sortBy: 'name',
        sortOrder: 'asc',
        limit: 10
      };

      const searchResult = await newManager.searchPersonas(searchRequest);
      expect(searchResult.success).toBe(true);
      expect(searchResult.personas).toHaveLength(1);
    });

    it('should handle concurrent file operations', async () => {
      const concurrentCount = 20;
      const requests = Array.from({ length: concurrentCount }, (_, i) => 
        personaManager.createPersona({
          config: createTestPersonaConfig({
            name: `ConcurrentPersona${i + 1}`,
            specialization: SPECIALIZATIONS[i % SPECIALIZATIONS.length]
          }),
          parentGenomes: [],
          mutationRate: 0.1
        })
      );

      const results = await Promise.all(requests);

      // All should succeed
      results.forEach(result => {
        expect(result.success).toBe(true);
        expect(result.persona).toBeDefined();
      });

      // Verify all files were created
      const files = await fs.readdir(personaDir);
      expect(files).toHaveLength(concurrentCount);

      // Verify all personas are unique
      const ids = results.map(r => r.persona?.id).filter(Boolean);
      const uniqueIds = new Set(ids);
      expect(uniqueIds.size).toBe(concurrentCount);
    });
  });

  describe('Large-Scale Population Management', () => {
    it('should handle large persona populations efficiently', async () => {
      const populationSize = 100;
      const personas: PersonaGenome[] = [];

      // Create large population
      const { result: creationResults, averageTimeMs } = await benchmarkFunction(
        async () => {
          const batch = [];
          for (let i = 0; i < 10; i++) {
            batch.push(personaManager.createPersona({
              config: createTestPersonaConfig({
                name: `LargePopPersona${personas.length + i + 1}`,
                specialization: SPECIALIZATIONS[i % SPECIALIZATIONS.length]
              }),
              parentGenomes: [],
              mutationRate: 0.1
            }));
          }
          return await Promise.all(batch);
        },
        10 // 10 batches of 10 personas each
      );

      // Flatten results
      const allResults = creationResults.flat();
      allResults.forEach(result => {
        expect(result.success).toBe(true);
        if (result.persona) {
          personas.push(result.persona);
        }
      });

      expect(personas).toHaveLength(populationSize);
      expect(averageTimeMs).toBeLessThan(1000); // Should be fast

      // Test search performance on large population
      const { result: searchResults, timeMs: searchTime } = await measureExecutionTime(
        async () => {
          return await personaManager.searchPersonas({
            filters: { specialization: 'typescript' },
            sortBy: 'fitness',
            sortOrder: 'desc',
            limit: 20
          });
        }
      );

      expect(searchResults.success).toBe(true);
      expect(searchTime).toBeLessThan(100); // Should be very fast
    });

    it('should handle complex evolution lineages', async () => {
      // Create founder population
      const founders = [];
      for (let i = 0; i < 5; i++) {
        const result = await personaManager.createPersona({
          config: createTestPersonaConfig({
            name: `Founder${i + 1}`,
            specialization: SPECIALIZATIONS[i % SPECIALIZATIONS.length]
          }),
          parentGenomes: [],
          mutationRate: 0.1
        });
        
        if (result.success && result.persona) {
          founders.push(result.persona);
        }
      }

      // Create multiple generations through crossover
      let currentGeneration = founders;
      const allPersonas = [...founders];

      for (let gen = 1; gen <= 3; gen++) {
        const nextGeneration = [];
        
        // Create children from current generation
        for (let i = 0; i < currentGeneration.length; i++) {
          const parent1 = currentGeneration[i];
          const parent2 = currentGeneration[(i + 1) % currentGeneration.length];
          
          const crossoverResult = await personaManager.performCrossover({
            parentIds: [parent1.id, parent2.id],
            crossoverWeights: {
              identity: 0.5,
              knowledge: 0.6,
              behavior: 0.4,
              substrate: 0.5
            },
            mutationRate: 0.1,
            childName: `Gen${gen}Child${i + 1}`
          });

          if (crossoverResult.success && crossoverResult.child) {
            nextGeneration.push(crossoverResult.child);
            allPersonas.push(crossoverResult.child);
          }
        }

        currentGeneration = nextGeneration;
      }

      // Verify lineage structure
      expect(allPersonas.length).toBeGreaterThan(founders.length);
      
      // Verify generation progression
      const generationCounts = new Map<number, number>();
      allPersonas.forEach(persona => {
        const gen = persona.evolution.generation;
        generationCounts.set(gen, (generationCounts.get(gen) || 0) + 1);
      });

      expect(generationCounts.get(0)).toBe(founders.length);
      expect(generationCounts.get(1)).toBeGreaterThan(0);
      expect(generationCounts.get(2)).toBeGreaterThan(0);
      expect(generationCounts.get(3)).toBeGreaterThan(0);
    });

    it('should generate comprehensive statistics for large populations', async () => {
      // Create diverse population
      const populationSize = 50;
      for (let i = 0; i < populationSize; i++) {
        const generation = Math.floor(i / 10); // 10 personas per generation
        const fitnessScore = Math.random() * 0.8 + 0.2;
        
        const result = await personaManager.createPersona({
          config: createTestPersonaConfig({
            name: `StatsPersona${i + 1}`,
            specialization: SPECIALIZATIONS[i % SPECIALIZATIONS.length]
          }),
          parentGenomes: [],
          mutationRate: 0.1
        });

        if (result.success && result.persona) {
          // Update fitness to create distribution
          await personaManager.updatePersona({
            personaId: result.persona.id,
            updates: {
              fitnessScore,
              generation
            }
          });
        }
      }

      // Generate comprehensive statistics
      const { result: statsResult, timeMs } = await measureExecutionTime(
        async () => {
          return await personaManager.getStatistics({
            includePopulationStats: true,
            includeFitnessDistribution: true,
            includeSpecializationBreakdown: true,
            includeEvolutionTrends: true,
            generationRange: { min: 0, max: 10 }
          });
        }
      );

      expect(statsResult.success).toBe(true);
      expect(timeMs).toBeLessThan(200); // Should be fast even for large populations
      
      if (statsResult.statistics) {
        const stats = statsResult.statistics;
        
        expect(stats.totalPersonas).toBe(populationSize);
        expect(stats.averageFitness).toBeGreaterThan(0.2);
        expect(stats.averageFitness).toBeLessThan(1.0);
        
        expect(stats.generationDistribution).toBeDefined();
        expect(Object.keys(stats.generationDistribution)).toHaveLength(5); // 5 generations
        
        expect(stats.specializationBreakdown).toBeDefined();
        expect(Object.keys(stats.specializationBreakdown).length).toBeGreaterThan(1);
        
        expect(stats.fitnessDistribution).toBeDefined();
        expect(stats.fitnessDistribution.bins.length).toBeGreaterThan(0);
        
        expect(stats.evolutionTrends).toBeDefined();
        expect(stats.evolutionTrends.length).toBeGreaterThan(0);
      }
    });
  });

  describe('Data Integrity and Recovery', () => {
    it('should maintain data integrity during concurrent operations', async () => {
      // Create initial persona
      const initialResult = await personaManager.createPersona({
        config: createTestPersonaConfig({
          name: 'ConcurrencyTest',
          specialization: 'typescript'
        }),
        parentGenomes: [],
        mutationRate: 0.1
      });

      expect(initialResult.success).toBe(true);
      const personaId = initialResult.persona!.id;

      // Perform concurrent updates
      const updatePromises = Array.from({ length: 10 }, (_, i) => 
        personaManager.updatePersona({
          personaId,
          updates: {
            fitnessScore: (i + 1) * 0.1,
            experiencePoints: (i + 1) * 100
          }
        })
      );

      const updateResults = await Promise.all(updatePromises);
      
      // All updates should succeed
      updateResults.forEach(result => {
        expect(result.success).toBe(true);
      });

      // Verify final state is consistent
      const searchResult = await personaManager.searchPersonas({
        filters: { name: 'ConcurrencyTest' },
        sortBy: 'name',
        sortOrder: 'asc',
        limit: 1
      });

      expect(searchResult.success).toBe(true);
      expect(searchResult.personas).toHaveLength(1);
      
      const finalPersona = searchResult.personas![0];
      expect(finalPersona.evolution.fitnessScore).toBeGreaterThan(0);
      expect(finalPersona.knowledge.experiencePoints).toBeGreaterThan(0);
    });

    it('should handle export/import data integrity', async () => {
      // Create test population
      const testPersonas = [];
      for (let i = 0; i < 5; i++) {
        const result = await personaManager.createPersona({
          config: createTestPersonaConfig({
            name: `ExportImportTest${i + 1}`,
            specialization: SPECIALIZATIONS[i % SPECIALIZATIONS.length]
          }),
          parentGenomes: [],
          mutationRate: 0.1
        });
        
        if (result.success && result.persona) {
          testPersonas.push(result.persona);
        }
      }

      // Export personas
      const exportResult = await personaManager.exportPersonas({
        personaIds: testPersonas.map(p => p.id),
        format: 'json',
        includeMetadata: true
      });

      expect(exportResult.success).toBe(true);
      expect(exportResult.data).toBeDefined();

      // Create new PersonaManager instance (simulating restart)
      const newTempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'persona-import-test-'));
      const newPersonaManager = new PersonaManagerServer(newTempDir);
      await newPersonaManager.initialize();

      // Import personas
      const importResult = await newPersonaManager.importPersonas({
        data: exportResult.data!,
        format: 'json',
        overwriteExisting: true
      });

      expect(importResult.success).toBe(true);
      expect(importResult.importedCount).toBe(testPersonas.length);
      expect(importResult.errorCount).toBe(0);

      // Verify imported personas are identical
      for (const originalPersona of testPersonas) {
        const searchResult = await newPersonaManager.searchPersonas({
          filters: { name: originalPersona.identity.name },
          sortBy: 'name',
          sortOrder: 'asc',
          limit: 1
        });

        expect(searchResult.success).toBe(true);
        expect(searchResult.personas).toHaveLength(1);
        
        const importedPersona = searchResult.personas![0];
        expect(importedPersona.id).toBe(originalPersona.id);
        expect(importedPersona.identity.name).toBe(originalPersona.identity.name);
        expect(importedPersona.identity.specialization).toBe(originalPersona.identity.specialization);
        
        // Verify structure integrity
        assertValidPersonaGenome(importedPersona);
      }

      // Cleanup
      await fs.rm(newTempDir, { recursive: true, force: true });
    });

    it('should handle partial file corruption gracefully', async () => {
      // Create several personas
      const personas = [];
      for (let i = 0; i < 5; i++) {
        const result = await personaManager.createPersona({
          config: createTestPersonaConfig({
            name: `CorruptionTest${i + 1}`,
            specialization: SPECIALIZATIONS[i % SPECIALIZATIONS.length]
          }),
          parentGenomes: [],
          mutationRate: 0.1
        });
        
        if (result.success && result.persona) {
          personas.push(result.persona);
        }
      }

      // Corrupt one persona file
      const corruptedPersonaPath = path.join(personaDir, `${personas[2].id}.json`);
      await fs.writeFile(corruptedPersonaPath, '{ corrupted data }');

      // Reinitialize PersonaManager
      const newManager = new PersonaManagerServer(tempDir);
      await newManager.initialize();

      // Should still find the uncorrupted personas
      const searchResult = await newManager.searchPersonas({
        filters: {},
        sortBy: 'name',
        sortOrder: 'asc',
        limit: 10
      });

      expect(searchResult.success).toBe(true);
      expect(searchResult.personas).toHaveLength(4); // 5 - 1 corrupted

      // Verify the remaining personas are valid
      searchResult.personas!.forEach(persona => {
        assertValidPersonaGenome(persona);
        expect(persona.identity.name).toMatch(/^CorruptionTest[1-5]$/);
      });
    });
  });

  describe('Performance Benchmarks', () => {
    it('should meet performance requirements for persona operations', async () => {
      // Benchmark persona creation
      const { averageTimeMs: createTime } = await benchmarkFunction(
        async () => {
          return await personaManager.createPersona({
            config: createTestPersonaConfig({
              name: `BenchmarkPersona${Date.now()}`,
              specialization: 'typescript'
            }),
            parentGenomes: [],
            mutationRate: 0.1
          });
        },
        5
      );

      expect(createTime).toBeLessThan(50); // Should be very fast

      // Create test population for search benchmarks
      const testPopulation = [];
      for (let i = 0; i < 100; i++) {
        const result = await personaManager.createPersona({
          config: createTestPersonaConfig({
            name: `SearchBenchmark${i + 1}`,
            specialization: SPECIALIZATIONS[i % SPECIALIZATIONS.length]
          }),
          parentGenomes: [],
          mutationRate: 0.1
        });
        
        if (result.success && result.persona) {
          testPopulation.push(result.persona);
        }
      }

      // Benchmark search operations
      const { averageTimeMs: searchTime } = await benchmarkFunction(
        async () => {
          return await personaManager.searchPersonas({
            filters: { specialization: 'typescript' },
            sortBy: 'fitness',
            sortOrder: 'desc',
            limit: 10
          });
        },
        10
      );

      expect(searchTime).toBeLessThan(20); // Should be very fast

      // Benchmark crossover operations
      const { averageTimeMs: crossoverTime } = await benchmarkFunction(
        async () => {
          const parent1 = testPopulation[Math.floor(Math.random() * testPopulation.length)];
          const parent2 = testPopulation[Math.floor(Math.random() * testPopulation.length)];
          
          return await personaManager.performCrossover({
            parentIds: [parent1.id, parent2.id],
            crossoverWeights: {
              identity: 0.5,
              knowledge: 0.5,
              behavior: 0.5,
              substrate: 0.5
            },
            mutationRate: 0.1,
            childName: `CrossoverBenchmark${Date.now()}`
          });
        },
        5
      );

      expect(crossoverTime).toBeLessThan(100); // Should be reasonably fast
    });
  });

  describe('Middle-Out Layer 4 Integration Validation', () => {
    it('should validate PersonaManager architecture integration', async () => {
      // Verify PersonaManager follows expected interface
      expect(typeof personaManager.createPersona).toBe('function');
      expect(typeof personaManager.searchPersonas).toBe('function');
      expect(typeof personaManager.updatePersona).toBe('function');
      expect(typeof personaManager.performCrossover).toBe('function');
      expect(typeof personaManager.validatePersona).toBe('function');
      expect(typeof personaManager.getStatistics).toBe('function');
      expect(typeof personaManager.exportPersonas).toBe('function');
      expect(typeof personaManager.importPersonas).toBe('function');
      
      // Verify integration with file system
      expect(await fs.access(personaDir).then(() => true).catch(() => false)).toBe(true);
      
      // Verify integration with Academy types
      const testPersona = await personaManager.createPersona({
        config: createTestPersonaConfig(),
        parentGenomes: [],
        mutationRate: 0.1
      });
      
      expect(testPersona.success).toBe(true);
      if (testPersona.persona) {
        assertValidPersonaGenome(testPersona.persona);
      }
    });

    it('should validate end-to-end persona lifecycle', async () => {
      // Create persona
      const createResult = await personaManager.createPersona({
        config: createTestPersonaConfig({
          name: 'LifecycleTest',
          specialization: 'typescript'
        }),
        parentGenomes: [],
        mutationRate: 0.1
      });

      expect(createResult.success).toBe(true);
      const persona = createResult.persona!;

      // Search for persona
      const searchResult = await personaManager.searchPersonas({
        filters: { name: 'LifecycleTest' },
        sortBy: 'name',
        sortOrder: 'asc',
        limit: 1
      });

      expect(searchResult.success).toBe(true);
      expect(searchResult.personas).toHaveLength(1);
      expect(searchResult.personas![0].id).toBe(persona.id);

      // Update persona
      const updateResult = await personaManager.updatePersona({
        personaId: persona.id,
        updates: {
          fitnessScore: 0.8,
          experiencePoints: 500
        }
      });

      expect(updateResult.success).toBe(true);
      expect(updateResult.persona!.evolution.fitnessScore).toBe(0.8);
      expect(updateResult.persona!.knowledge.experiencePoints).toBe(500);

      // Validate persona
      const validateResult = await personaManager.validatePersona({
        persona: updateResult.persona!,
        strict: true
      });

      expect(validateResult.success).toBe(true);
      expect(validateResult.isValid).toBe(true);

      // Export persona
      const exportResult = await personaManager.exportPersonas({
        personaIds: [persona.id],
        format: 'json',
        includeMetadata: true
      });

      expect(exportResult.success).toBe(true);
      expect(exportResult.data).toBeDefined();

      // Verify complete lifecycle
      const exportedData = JSON.parse(exportResult.data!);
      expect(exportedData.personas).toHaveLength(1);
      expect(exportedData.personas[0].id).toBe(persona.id);
      expect(exportedData.personas[0].evolution.fitnessScore).toBe(0.8);
      expect(exportedData.personas[0].knowledge.experiencePoints).toBe(500);
    });
  });
});