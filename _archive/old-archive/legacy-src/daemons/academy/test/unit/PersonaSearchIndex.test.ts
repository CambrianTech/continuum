/**
 * Unit Tests for PersonaSearchIndex - Vector Space Intelligence Network
 * 
 * Proves the Academy's 512-dimensional capability vector search and persona
 * matching system works as designed. These tests validate the core vector
 * space intelligence that enables distributed AI collaboration.
 */

import { PersonaSearchIndex, PersonaCapability, PersonaSearchQuery, PersonaSearchResult } from '../../PersonaSearchIndex';
import { LoRADiscovery, LoRAMetadata } from '../../LoRADiscovery';
import { jest } from '@jest/globals';

// Mock LoRADiscovery for controlled testing
jest.mock('../../LoRADiscovery');
const MockLoRADiscovery = LoRADiscovery as jest.MockedClass<typeof LoRADiscovery>;

describe('PersonaSearchIndex - Vector Space Intelligence Network', () => {
  let searchIndex: PersonaSearchIndex;
  let mockLoRADiscovery: jest.Mocked<LoRADiscovery>;

  beforeEach(() => {
    jest.clearAllMocks();
    
    // Setup mock LoRADiscovery
    mockLoRADiscovery = new MockLoRADiscovery() as jest.Mocked<LoRADiscovery>;
    
    searchIndex = new PersonaSearchIndex();
    // Replace the internal loraDiscovery with our mock
    (searchIndex as any).loraDiscovery = mockLoRADiscovery;
  });

  describe('Index Initialization', () => {
    it('generates 512-dimensional capability vectors', async () => {
      // Mock adapter discovery
      mockLoRADiscovery.discoverAdapters.mockResolvedValue([
        {
          id: 'typescript_expert',
          name: 'TypeScript Expert',
          domain: 'programming',
          category: 'Language',
          rank: 16,
          alpha: 32,
          targetModules: ['q_proj', 'v_proj'],
          dependencies: [],
          filePath: '.continuum/personas/ts_expert/adapters/typescript_expert',
          version: '1.0.0',
          author: 'Academy',
          description: 'TypeScript expertise',
          isValid: true,
          warnings: [],
          errors: []
        },
        {
          id: 'testing_specialist',
          name: 'Testing Specialist',
          domain: 'testing',
          category: 'Quality',
          rank: 12,
          alpha: 24,
          targetModules: ['q_proj', 'v_proj'],
          dependencies: [],
          filePath: '.continuum/personas/test_expert/adapters/testing_specialist',
          version: '1.0.0',
          author: 'Academy',
          description: 'Testing expertise',
          isValid: true,
          warnings: [],
          errors: []
        }
      ]);

      await searchIndex.initializeIndex();

      const stats = searchIndex.getIndexStats();
      expect(stats.total_personas).toBeGreaterThan(0);

      // Verify all personas have 512-dimensional vectors
      const personas = Array.from((searchIndex as any).personas.values());
      for (const persona of personas) {
        expect(persona.capability_vector).toHaveLength(512);
        expect(persona.capability_vector.every((v: number) => typeof v === 'number')).toBe(true);
        expect(persona.capability_vector.some((v: number) => v > 0)).toBe(true);
      }
    });

    it('indexes personas by skills and domains', async () => {
      mockLoRADiscovery.discoverAdapters.mockResolvedValue([
        {
          id: 'python_expert',
          name: 'Python Expert',
          domain: 'python',
          category: 'Programming',
          rank: 20,
          alpha: 40,
          targetModules: ['q_proj', 'v_proj', 'k_proj'],
          dependencies: [],
          filePath: '.continuum/personas/python_expert/adapters/python_expert',
          version: '2.0.0',
          author: 'Academy',
          description: 'Python programming expertise',
          isValid: true,
          warnings: [],
          errors: []
        },
        {
          id: 'ml_specialist',
          name: 'Machine Learning Specialist',
          domain: 'machine_learning',
          category: 'AI',
          rank: 24,
          alpha: 48,
          targetModules: ['q_proj', 'v_proj', 'k_proj', 'o_proj'],
          dependencies: [],
          filePath: '.continuum/personas/ml_expert/adapters/ml_specialist',
          version: '1.5.0',
          author: 'Academy',
          description: 'ML and AI expertise',
          isValid: true,
          warnings: [],
          errors: []
        }
      ]);

      await searchIndex.initializeIndex();

      const stats = searchIndex.getIndexStats();
      
      // Verify indexing structure
      expect(stats.total_personas).toBe(2);
      expect(stats.domains_covered).toContain('python');
      expect(stats.domains_covered).toContain('machine_learning');
      
      // Verify skill coverage
      expect(stats.skill_coverage['python']).toBeGreaterThan(0);
      expect(stats.skill_coverage['machine_learning']).toBeGreaterThan(0);
      expect(stats.skill_coverage['programming']).toBeGreaterThan(0);
    });

    it('builds multi-domain personas from multiple adapters', async () => {
      mockLoRADiscovery.discoverAdapters.mockResolvedValue([
        {
          id: 'fullstack_base',
          name: 'Full Stack Base',
          domain: 'frontend',
          category: 'Web',
          rank: 16,
          alpha: 32,
          targetModules: ['q_proj', 'v_proj'],
          dependencies: [],
          filePath: '.continuum/personas/fullstack_dev/adapters/frontend',
          version: '1.0.0',
          author: 'Academy',
          description: 'Frontend development',
          isValid: true,
          warnings: [],
          errors: []
        },
        {
          id: 'fullstack_backend',
          name: 'Full Stack Backend',
          domain: 'backend',
          category: 'Web',
          rank: 16,
          alpha: 32,
          targetModules: ['q_proj', 'v_proj'],
          dependencies: [],
          filePath: '.continuum/personas/fullstack_dev/adapters/backend',
          version: '1.0.0',
          author: 'Academy',
          description: 'Backend development',
          isValid: true,
          warnings: [],
          errors: []
        },
        {
          id: 'fullstack_database',
          name: 'Full Stack Database',
          domain: 'database',
          category: 'Data',
          rank: 12,
          alpha: 24,
          targetModules: ['q_proj', 'v_proj'],
          dependencies: [],
          filePath: '.continuum/personas/fullstack_dev/adapters/database',
          version: '1.0.0',
          author: 'Academy',
          description: 'Database management',
          isValid: true,
          warnings: [],
          errors: []
        }
      ]);

      await searchIndex.initializeIndex();

      const personas = Array.from((searchIndex as any).personas.values());
      const fullstackPersona = personas.find((p: PersonaCapability) => 
        p.skill_tags.includes('frontend') && 
        p.skill_tags.includes('backend') && 
        p.skill_tags.includes('database')
      );

      expect(fullstackPersona).toBeDefined();
      expect(fullstackPersona?.domain).toBe('multi_domain');
      expect(fullstackPersona?.skill_tags).toContain('frontend');
      expect(fullstackPersona?.skill_tags).toContain('backend');
      expect(fullstackPersona?.skill_tags).toContain('database');
      expect(fullstackPersona?.experience_points).toBe(300); // 3 adapters * 100
    });
  });

  describe('Vector Space Search', () => {
    beforeEach(async () => {
      // Setup test personas with known capability vectors
      const testPersona1: PersonaCapability = {
        id: 'test_persona_1',
        name: 'TypeScript Expert',
        domain: 'programming',
        capability_vector: new Array(512).fill(0).map((_, i) => i < 10 ? 0.8 : 0),
        skill_tags: ['typescript', 'programming', 'testing'],
        proficiency_scores: { typescript: 0.9, programming: 0.8, testing: 0.7 },
        specializations: ['typescript_specialization'],
        experience_points: 500,
        success_rate: 0.85,
        last_training: new Date(),
        adaptation_speed: 0.7,
        reliability_score: 0.9
      };

      const testPersona2: PersonaCapability = {
        id: 'test_persona_2',
        name: 'Python Expert',
        domain: 'programming',
        capability_vector: new Array(512).fill(0).map((_, i) => i >= 10 && i < 20 ? 0.9 : 0),
        skill_tags: ['python', 'programming', 'data_science'],
        proficiency_scores: { python: 0.95, programming: 0.8, data_science: 0.8 },
        specializations: ['python_specialization'],
        experience_points: 600,
        success_rate: 0.9,
        last_training: new Date(),
        adaptation_speed: 0.8,
        reliability_score: 0.85
      };

      searchIndex.addPersonaToIndex(testPersona1);
      searchIndex.addPersonaToIndex(testPersona2);
    });

    it('finds personas by vector similarity', async () => {
      // Query vector similar to TypeScript expert (first 10 dimensions active)
      const queryVector = new Array(512).fill(0).map((_, i) => i < 10 ? 0.7 : 0);

      const query: PersonaSearchQuery = {
        capability_vector: queryVector,
        similarity_threshold: 0.5,
        max_results: 5
      };

      const results = await searchIndex.searchPersonas(query);

      expect(results.length).toBeGreaterThan(0);
      
      // TypeScript expert should have higher similarity
      const tsResult = results.find(r => r.persona.id === 'test_persona_1');
      const pyResult = results.find(r => r.persona.id === 'test_persona_2');

      expect(tsResult).toBeDefined();
      expect(tsResult!.vector_similarity).toBeGreaterThan(0.5);
      
      if (pyResult) {
        expect(tsResult!.vector_similarity).toBeGreaterThan(pyResult.vector_similarity);
      }
    });

    it('filters by required skills accurately', async () => {
      const query: PersonaSearchQuery = {
        required_skills: ['typescript', 'programming'],
        max_results: 10
      };

      const results = await searchIndex.searchPersonas(query);

      expect(results.length).toBeGreaterThan(0);
      
      // All results should have both required skills
      for (const result of results) {
        expect(result.persona.skill_tags).toContain('typescript');
        expect(result.persona.skill_tags).toContain('programming');
        expect(result.skill_matches).toContain('typescript');
        expect(result.skill_matches).toContain('programming');
      }
    });

    it('scores preferred skills appropriately', async () => {
      const query: PersonaSearchQuery = {
        preferred_skills: ['python', 'data_science'],
        max_results: 10
      };

      const results = await searchIndex.searchPersonas(query);

      expect(results.length).toBeGreaterThan(0);
      
      // Python expert should score higher due to preferred skills
      const sortedResults = results.sort((a, b) => b.match_score - a.match_score);
      expect(sortedResults[0].persona.id).toBe('test_persona_2');
      expect(sortedResults[0].skill_matches).toContain('python');
    });

    it('respects minimum proficiency thresholds', async () => {
      const query: PersonaSearchQuery = {
        min_proficiency: 0.85,
        max_results: 10
      };

      const results = await searchIndex.searchPersonas(query);

      // Verify all results meet minimum proficiency
      for (const result of results) {
        const avgProficiency = Object.values(result.persona.proficiency_scores)
          .reduce((a, b) => a + b, 0) / Object.values(result.persona.proficiency_scores).length;
        expect(avgProficiency).toBeGreaterThanOrEqual(0.85);
      }
    });

    it('filters by success rate threshold', async () => {
      const query: PersonaSearchQuery = {
        min_success_rate: 0.88,
        max_results: 10
      };

      const results = await searchIndex.searchPersonas(query);

      // Only Python expert (0.9 success rate) should match
      expect(results.length).toBe(1);
      expect(results[0].persona.id).toBe('test_persona_2');
      expect(results[0].persona.success_rate).toBeGreaterThanOrEqual(0.88);
    });

    it('limits results correctly', async () => {
      const query: PersonaSearchQuery = {
        max_results: 1
      };

      const results = await searchIndex.searchPersonas(query);

      expect(results.length).toBeLessThanOrEqual(1);
    });

    it('combines multiple criteria effectively', async () => {
      const query: PersonaSearchQuery = {
        required_skills: ['programming'],
        preferred_skills: ['typescript'],
        min_success_rate: 0.8,
        similarity_threshold: 0.3,
        max_results: 5
      };

      const results = await searchIndex.searchPersonas(query);

      expect(results.length).toBeGreaterThan(0);
      
      // All should have programming skill and meet success rate
      for (const result of results) {
        expect(result.persona.skill_tags).toContain('programming');
        expect(result.persona.success_rate).toBeGreaterThanOrEqual(0.8);
        expect(result.match_score).toBeGreaterThanOrEqual(0.3);
      }
    });
  });

  describe('Persona Management', () => {
    it('updates persona capabilities correctly', async () => {
      const testPersona: PersonaCapability = {
        id: 'update_test',
        name: 'Update Test Persona',
        domain: 'testing',
        capability_vector: new Array(512).fill(0.5),
        skill_tags: ['testing'],
        proficiency_scores: { testing: 0.6 },
        specializations: [],
        experience_points: 100,
        success_rate: 0.7,
        last_training: new Date(),
        adaptation_speed: 0.5,
        reliability_score: 0.8
      };

      searchIndex.addPersonaToIndex(testPersona);

      // Update capabilities
      await searchIndex.updatePersonaCapabilities('update_test', {
        skill_tags: ['testing', 'automation', 'qa'],
        proficiency_scores: { testing: 0.8, automation: 0.7, qa: 0.6 },
        success_rate: 0.85,
        experience_points: 200
      });

      // Search for updated persona
      const results = await searchIndex.searchPersonas({
        required_skills: ['automation'],
        max_results: 5
      });

      expect(results.length).toBeGreaterThan(0);
      const updatedPersona = results.find(r => r.persona.id === 'update_test');
      
      expect(updatedPersona).toBeDefined();
      expect(updatedPersona!.persona.skill_tags).toContain('automation');
      expect(updatedPersona!.persona.proficiency_scores.automation).toBe(0.7);
      expect(updatedPersona!.persona.success_rate).toBe(0.85);
    });

    it('handles persona not found error', async () => {
      await expect(searchIndex.updatePersonaCapabilities('nonexistent', {
        skill_tags: ['new_skill']
      })).rejects.toThrow('Persona not found in index: nonexistent');
    });

    it('rebuilds indices when skills change', async () => {
      const testPersona: PersonaCapability = {
        id: 'skill_change_test',
        name: 'Skill Change Test',
        domain: 'development',
        capability_vector: new Array(512).fill(0.1),
        skill_tags: ['old_skill'],
        proficiency_scores: { old_skill: 0.7 },
        specializations: [],
        experience_points: 150,
        success_rate: 0.75,
        last_training: new Date(),
        adaptation_speed: 0.6,
        reliability_score: 0.8
      };

      searchIndex.addPersonaToIndex(testPersona);

      // Search with old skill should find it
      let results = await searchIndex.searchPersonas({
        required_skills: ['old_skill']
      });
      expect(results.length).toBe(1);

      // Update to new skill
      await searchIndex.updatePersonaCapabilities('skill_change_test', {
        skill_tags: ['new_skill'],
        proficiency_scores: { new_skill: 0.8 }
      });

      // Search with old skill should find nothing
      results = await searchIndex.searchPersonas({
        required_skills: ['old_skill']
      });
      expect(results.length).toBe(0);

      // Search with new skill should find it
      results = await searchIndex.searchPersonas({
        required_skills: ['new_skill']
      });
      expect(results.length).toBe(1);
    });
  });

  describe('P2P Query Preparation', () => {
    it('prepares well-formed P2P queries', async () => {
      const query: PersonaSearchQuery = {
        required_skills: ['distributed_systems', 'networking'],
        preferred_skills: ['p2p', 'consensus'],
        domain_filter: ['systems', 'networking'],
        min_proficiency: 0.8,
        min_success_rate: 0.9,
        capability_vector: new Array(512).fill(0).map((_, i) => i < 5 ? 0.8 : 0),
        similarity_threshold: 0.7,
        max_results: 3
      };

      const p2pQuery = searchIndex.prepareP2PQuery(query);

      expect(p2pQuery.query_type).toBe('persona_capability_search');
      expect(p2pQuery.query_vector).toEqual(query.capability_vector);
      expect(p2pQuery.required_skills).toEqual(['distributed_systems', 'networking']);
      expect(p2pQuery.preferred_skills).toEqual(['p2p', 'consensus']);
      expect(p2pQuery.domain_filter).toEqual(['systems', 'networking']);
      
      expect(p2pQuery.constraints.min_proficiency).toBe(0.8);
      expect(p2pQuery.constraints.min_success_rate).toBe(0.9);
      expect(p2pQuery.constraints.similarity_threshold).toBe(0.7);
      
      expect(p2pQuery.query_metadata.source_node).toBe('local');
      expect(p2pQuery.query_metadata.max_results).toBe(3);
      expect(p2pQuery.query_metadata.timeout_ms).toBe(5000);
      expect(p2pQuery.query_metadata.timestamp).toBeDefined();
    });

    it('handles minimal queries for P2P', async () => {
      const minimalQuery: PersonaSearchQuery = {};

      const p2pQuery = searchIndex.prepareP2PQuery(minimalQuery);

      expect(p2pQuery.query_type).toBe('persona_capability_search');
      expect(p2pQuery.required_skills).toEqual([]);
      expect(p2pQuery.preferred_skills).toEqual([]);
      expect(p2pQuery.domain_filter).toEqual([]);
      expect(p2pQuery.constraints.min_proficiency).toBe(0);
      expect(p2pQuery.constraints.similarity_threshold).toBe(0.5);
      expect(p2pQuery.query_metadata.max_results).toBe(10);
    });
  });

  describe('Search Index Statistics', () => {
    beforeEach(async () => {
      // Add multiple test personas
      const personas = [
        {
          id: 'stats_test_1',
          name: 'Stats Test 1',
          domain: 'programming',
          capability_vector: new Array(512).fill(0.1),
          skill_tags: ['typescript', 'react', 'testing'],
          proficiency_scores: { typescript: 0.8 },
          specializations: [],
          experience_points: 100,
          success_rate: 0.8,
          last_training: new Date(),
          adaptation_speed: 0.6,
          reliability_score: 0.8
        },
        {
          id: 'stats_test_2',
          name: 'Stats Test 2',
          domain: 'data_science',
          capability_vector: new Array(512).fill(0.2),
          skill_tags: ['python', 'pandas', 'machine_learning'],
          proficiency_scores: { python: 0.9 },
          specializations: [],
          experience_points: 200,
          success_rate: 0.9,
          last_training: new Date(),
          adaptation_speed: 0.7,
          reliability_score: 0.9
        }
      ];

      for (const persona of personas) {
        searchIndex.addPersonaToIndex(persona);
      }
    });

    it('provides accurate statistics', async () => {
      const stats = searchIndex.getIndexStats();

      expect(stats.total_personas).toBe(2);
      expect(stats.indexed_capabilities).toBe(6); // 3 + 3 skills
      expect(stats.domains_covered).toContain('programming');
      expect(stats.domains_covered).toContain('data_science');
      
      expect(stats.skill_coverage['typescript']).toBe(1);
      expect(stats.skill_coverage['python']).toBe(1);
      expect(stats.skill_coverage['react']).toBe(1);
      
      expect(stats.vector_space_coverage.active_regions).toBeGreaterThan(0);
      expect(stats.last_index_update).toBeInstanceOf(Date);
    });

    it('updates statistics when personas change', async () => {
      const initialStats = searchIndex.getIndexStats();
      
      // Add another persona
      searchIndex.addPersonaToIndex({
        id: 'stats_test_3',
        name: 'Stats Test 3',
        domain: 'devops',
        capability_vector: new Array(512).fill(0.15),
        skill_tags: ['kubernetes', 'docker', 'monitoring'],
        proficiency_scores: { kubernetes: 0.7 },
        specializations: [],
        experience_points: 150,
        success_rate: 0.85,
        last_training: new Date(),
        adaptation_speed: 0.65,
        reliability_score: 0.85
      });

      const updatedStats = searchIndex.getIndexStats();
      
      expect(updatedStats.total_personas).toBe(initialStats.total_personas + 1);
      expect(updatedStats.indexed_capabilities).toBe(initialStats.indexed_capabilities + 3);
      expect(updatedStats.domains_covered).toContain('devops');
      expect(updatedStats.skill_coverage['kubernetes']).toBe(1);
    });
  });

  describe('Error Handling', () => {
    it('handles LoRA discovery failures gracefully', async () => {
      mockLoRADiscovery.discoverAdapters.mockRejectedValue(new Error('Discovery failed'));

      await expect(searchIndex.initializeIndex()).rejects.toThrow('Discovery failed');
    });

    it('handles invalid capability vectors', async () => {
      const query: PersonaSearchQuery = {
        capability_vector: [1, 2, 3], // Wrong length
        max_results: 5
      };

      const results = await searchIndex.searchPersonas(query);
      
      // Should still work but with 0 vector similarity
      expect(results.length).toBeGreaterThanOrEqual(0);
      if (results.length > 0) {
        expect(results[0].vector_similarity).toBe(0);
      }
    });

    it('handles empty search index gracefully', async () => {
      const emptyIndex = new PersonaSearchIndex();
      
      const results = await emptyIndex.searchPersonas({
        required_skills: ['nonexistent']
      });

      expect(results).toEqual([]);
    });
  });
});