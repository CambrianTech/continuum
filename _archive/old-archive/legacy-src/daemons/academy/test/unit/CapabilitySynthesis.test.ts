/**
 * Unit Tests for CapabilitySynthesis - Multi-Domain Intelligence Assembly Engine
 * 
 * Proves the Academy's capability synthesis system can compose working AIs from
 * multiple domains like "biophysics + quantum chemistry + geology". These tests
 * validate the core intelligence composition engine that enables the AI evolution ecosystem.
 */

import { 
  CapabilitySynthesis, 
  CapabilityRequest, 
  SynthesisResult, 
  PerformanceReq,
  ComponentPersona,
  LoRAComposition
} from '../../CapabilitySynthesis';
import { LoRADiscovery, LoRAMetadata } from '../../LoRADiscovery';
import { jest } from '@jest/globals';

// Mock dependencies
jest.mock('../../LoRADiscovery');
const MockLoRADiscovery = LoRADiscovery as jest.MockedClass<typeof LoRADiscovery>;

describe('CapabilitySynthesis - Multi-Domain Intelligence Assembly Engine', () => {
  let synthesis: CapabilitySynthesis;
  let mockSearchIndex: any;
  let mockLoRADiscovery: jest.Mocked<LoRADiscovery>;

  beforeEach(() => {
    jest.clearAllMocks();
    
    // Setup mock search index
    mockSearchIndex = {
      searchPersonas: jest.fn()
    };
    
    // Setup mock LoRA discovery
    mockLoRADiscovery = new MockLoRADiscovery() as jest.Mocked<LoRADiscovery>;
    
    synthesis = new CapabilitySynthesis(mockSearchIndex, mockLoRADiscovery);
  });

  describe('Multi-Domain Capability Synthesis', () => {
    it('synthesizes biophysics + quantum chemistry + geology capability', async () => {
      // Mock available components
      mockSearchIndex.searchPersonas.mockImplementation((query: any) => {
        if (query.domain_filter?.includes('biophysics')) {
          return Promise.resolve([{
            persona: {
              id: 'biophysics_expert',
              name: 'Biophysics Expert',
              domain: 'biophysics',
              skill_tags: ['protein_folding', 'molecular_dynamics', 'cellular_mechanics'],
              proficiency_scores: { biophysics: 0.85 },
              success_rate: 0.88
            },
            match_score: 0.85
          }]);
        }
        if (query.domain_filter?.includes('quantum_chemistry')) {
          return Promise.resolve([{
            persona: {
              id: 'quantum_chemistry_expert',
              name: 'Quantum Chemistry Expert',
              domain: 'quantum_chemistry',
              skill_tags: ['molecular_orbitals', 'electron_correlation', 'reaction_mechanisms'],
              proficiency_scores: { quantum_chemistry: 0.92 },
              success_rate: 0.91
            },
            match_score: 0.92
          }]);
        }
        if (query.domain_filter?.includes('geology')) {
          return Promise.resolve([{
            persona: {
              id: 'geology_expert',
              name: 'Geology Expert',
              domain: 'geology',
              skill_tags: ['rock_formation', 'mineral_analysis', 'seismic_modeling'],
              proficiency_scores: { geology: 0.80 },
              success_rate: 0.84
            },
            match_score: 0.80
          }]);
        }
        return Promise.resolve([]);
      });

      mockLoRADiscovery.discoverAdapters.mockResolvedValue([
        {
          id: 'biophysics_adapter',
          name: 'Biophysics LoRA',
          domain: 'biophysics',
          category: 'Science',
          rank: 32,
          alpha: 64,
          targetModules: ['q_proj', 'v_proj', 'k_proj'],
          dependencies: [],
          filePath: '.continuum/adapters/biophysics',
          version: '1.0.0',
          author: 'Academy',
          description: 'Biophysics expertise',
          isValid: true,
          warnings: [],
          errors: []
        },
        {
          id: 'quantum_chem_adapter',
          name: 'Quantum Chemistry LoRA',
          domain: 'quantum_chemistry',
          category: 'Science',
          rank: 28,
          alpha: 56,
          targetModules: ['q_proj', 'v_proj'],
          dependencies: [],
          filePath: '.continuum/adapters/quantum_chemistry',
          version: '1.0.0',
          author: 'Academy',
          description: 'Quantum chemistry expertise',
          isValid: true,
          warnings: [],
          errors: []
        },
        {
          id: 'geology_adapter',
          name: 'Geology LoRA',
          domain: 'geology',
          category: 'Earth Science',
          rank: 24,
          alpha: 48,
          targetModules: ['q_proj', 'v_proj'],
          dependencies: [],
          filePath: '.continuum/adapters/geology',
          version: '1.0.0',
          author: 'Academy',
          description: 'Geological expertise',
          isValid: true,
          warnings: [],
          errors: []
        }
      ]);

      const request: CapabilityRequest = {
        target_domains: ['biophysics', 'quantum_chemistry', 'geology'],
        task_description: 'Analyze quantum tunneling effects in underground mineral formations for biophysical modeling',
        performance_requirements: [
          {
            domain: 'biophysics',
            min_proficiency: 0.8,
            critical_skills: ['molecular_dynamics', 'cellular_mechanics'],
            context_understanding: 0.85
          },
          {
            domain: 'quantum_chemistry',
            min_proficiency: 0.85,
            critical_skills: ['molecular_orbitals', 'reaction_mechanisms'],
            context_understanding: 0.90
          },
          {
            domain: 'geology',
            min_proficiency: 0.75,
            critical_skills: ['mineral_analysis', 'rock_formation'],
            context_understanding: 0.80
          }
        ],
        integration_complexity: 'complex',
        time_constraints: 60000,
        quality_threshold: 0.8
      };

      const result = await synthesis.synthesizeCapability(request);

      // Verify synthesis result
      expect(result).toBeDefined();
      expect(result.synthesis_strategy).toMatch(/layer_composition|fine_tune_required|novel_creation/);
      expect(result.confidence).toBeGreaterThan(0.3);
      expect(result.lora_composition).toBeDefined();
      expect(result.estimated_performance).toBeDefined();
      expect(result.resource_requirements).toBeDefined();
      
      // Verify it found components for all domains
      expect(mockSearchIndex.searchPersonas).toHaveBeenCalledTimes(3);
      expect(mockLoRADiscovery.discoverAdapters).toHaveBeenCalled();
      
      // Verify performance requirements are considered
      expect(result.estimated_performance.overall_score).toBeDefined();
      expect(typeof result.estimated_performance.overall_score).toBe('number');
      
      // Verify resource requirements are realistic
      expect(result.resource_requirements.compute_hours).toBeGreaterThan(0);
      expect(result.resource_requirements.memory_gb).toBeGreaterThan(0);
      expect(result.creation_time_estimate).toBeGreaterThan(0);
    });

    it('identifies capability gaps correctly', async () => {
      // Mock scenario where some domains have no available components
      mockSearchIndex.searchPersonas.mockImplementation((query: any) => {
        if (query.domain_filter?.includes('exotic_physics')) {
          return Promise.resolve([]); // No components available
        }
        if (query.domain_filter?.includes('programming')) {
          return Promise.resolve([{
            persona: {
              id: 'programmer',
              name: 'Programmer',
              domain: 'programming',
              skill_tags: ['typescript', 'algorithms'],
              proficiency_scores: { programming: 0.95 },
              success_rate: 0.92
            },
            match_score: 0.95
          }]);
        }
        return Promise.resolve([]);
      });

      mockLoRADiscovery.discoverAdapters.mockResolvedValue([
        {
          id: 'programming_adapter',
          name: 'Programming LoRA',
          domain: 'programming',
          category: 'Technical',
          rank: 16,
          alpha: 32,
          targetModules: ['q_proj', 'v_proj'],
          dependencies: [],
          filePath: '.continuum/adapters/programming',
          version: '1.0.0',
          author: 'Academy',
          description: 'Programming expertise',
          isValid: true,
          warnings: [],
          errors: []
        }
      ]);

      const request: CapabilityRequest = {
        target_domains: ['programming', 'exotic_physics'],
        task_description: 'Build simulation software for theoretical exotic matter interactions',
        performance_requirements: [
          {
            domain: 'programming',
            min_proficiency: 0.9,
            critical_skills: ['algorithms', 'simulation'],
            context_understanding: 0.85
          },
          {
            domain: 'exotic_physics',
            min_proficiency: 0.7,
            critical_skills: ['theoretical_modeling', 'exotic_matter'],
            context_understanding: 0.8
          }
        ],
        integration_complexity: 'novel',
        time_constraints: 120000,
        quality_threshold: 0.7
      };

      const result = await synthesis.synthesizeCapability(request);

      // Should detect gap and suggest novel creation or fine-tuning
      expect(result.synthesis_strategy).toMatch(/fine_tune_required|novel_creation/);
      expect(result.confidence).toBeLessThan(0.8); // Lower confidence due to gaps
      
      if (result.fine_tuning_plan) {
        expect(result.fine_tuning_plan.target_gaps).toBeDefined();
        expect(result.fine_tuning_plan.training_strategy).toBeDefined();
      }
    });

    it('estimates resource requirements accurately', async () => {
      // Mock simple single-domain request for baseline
      mockSearchIndex.searchPersonas.mockResolvedValue([{
        persona: {
          id: 'simple_expert',
          name: 'Simple Expert',
          domain: 'mathematics',
          skill_tags: ['algebra', 'calculus'],
          proficiency_scores: { mathematics: 0.9 },
          success_rate: 0.95
        },
        match_score: 0.9
      }]);

      mockLoRADiscovery.discoverAdapters.mockResolvedValue([{
        id: 'math_adapter',
        name: 'Math LoRA',
        domain: 'mathematics',
        category: 'Academic',
        rank: 8,
        alpha: 16,
        targetModules: ['q_proj'],
        dependencies: [],
        filePath: '.continuum/adapters/math',
        version: '1.0.0',
        author: 'Academy',
        description: 'Math expertise',
        isValid: true,
        warnings: [],
        errors: []
      }]);

      const simpleRequest: CapabilityRequest = {
        target_domains: ['mathematics'],
        task_description: 'Solve basic calculus problems',
        performance_requirements: [{
          domain: 'mathematics',
          min_proficiency: 0.8,
          critical_skills: ['calculus'],
          context_understanding: 0.7
        }],
        integration_complexity: 'simple',
        time_constraints: 10000,
        quality_threshold: 0.8
      };

      const simpleResult = await synthesis.synthesizeCapability(simpleRequest);

      // Now test complex multi-domain request
      const complexRequest: CapabilityRequest = {
        target_domains: ['physics', 'chemistry', 'biology', 'computer_science'],
        task_description: 'Build comprehensive molecular simulation platform',
        performance_requirements: [
          { domain: 'physics', min_proficiency: 0.9, critical_skills: ['quantum_mechanics'], context_understanding: 0.9 },
          { domain: 'chemistry', min_proficiency: 0.85, critical_skills: ['molecular_modeling'], context_understanding: 0.85 },
          { domain: 'biology', min_proficiency: 0.8, critical_skills: ['biochemistry'], context_understanding: 0.8 },
          { domain: 'computer_science', min_proficiency: 0.95, critical_skills: ['algorithms', 'optimization'], context_understanding: 0.9 }
        ],
        integration_complexity: 'complex',
        time_constraints: 300000,
        quality_threshold: 0.85
      };

      const complexResult = await synthesis.synthesizeCapability(complexRequest);

      // Complex request should require more resources
      expect(complexResult.resource_requirements.compute_hours)
        .toBeGreaterThan(simpleResult.resource_requirements.compute_hours);
      expect(complexResult.resource_requirements.memory_gb)
        .toBeGreaterThan(simpleResult.resource_requirements.memory_gb);
      expect(complexResult.creation_time_estimate)
        .toBeGreaterThan(simpleResult.creation_time_estimate);
    });
  });

  describe('Synthesis Strategy Selection', () => {
    it('selects exact match for simple requests', async () => {
      // Mock perfect match scenario
      mockSearchIndex.searchPersonas.mockResolvedValue([{
        persona: {
          id: 'perfect_match',
          name: 'Perfect Match Expert',
          domain: 'typescript',
          skill_tags: ['typescript', 'programming', 'testing'],
          proficiency_scores: { typescript: 0.95, programming: 0.9 },
          success_rate: 0.96
        },
        match_score: 0.95
      }]);

      mockLoRADiscovery.discoverAdapters.mockResolvedValue([{
        id: 'typescript_adapter',
        name: 'TypeScript LoRA',
        domain: 'typescript',
        category: 'Programming',
        rank: 16,
        alpha: 32,
        targetModules: ['q_proj', 'v_proj'],
        dependencies: [],
        filePath: '.continuum/adapters/typescript',
        version: '1.0.0',
        author: 'Academy',
        description: 'TypeScript expertise',
        isValid: true,
        warnings: [],
        errors: []
      }]);

      const request: CapabilityRequest = {
        target_domains: ['typescript'],
        task_description: 'Write TypeScript code with proper type definitions',
        performance_requirements: [{
          domain: 'typescript',
          min_proficiency: 0.8,
          critical_skills: ['type_definitions', 'programming'],
          context_understanding: 0.7
        }],
        integration_complexity: 'simple',
        time_constraints: 5000,
        quality_threshold: 0.8
      };

      const result = await synthesis.synthesizeCapability(request);

      expect(result.synthesis_strategy).toBe('exact_match');
      expect(result.confidence).toBeGreaterThan(0.9);
      expect(result.creation_time_estimate).toBeLessThan(5000);
    });

    it('selects layer composition for multi-domain requests', async () => {
      // Mock multiple complementary components
      mockSearchIndex.searchPersonas.mockImplementation((query: any) => {
        if (query.domain_filter?.includes('frontend')) {
          return Promise.resolve([{
            persona: {
              id: 'frontend_expert',
              name: 'Frontend Expert',
              domain: 'frontend',
              skill_tags: ['react', 'typescript', 'css'],
              proficiency_scores: { frontend: 0.85 },
              success_rate: 0.87
            },
            match_score: 0.75
          }]);
        }
        if (query.domain_filter?.includes('backend')) {
          return Promise.resolve([{
            persona: {
              id: 'backend_expert',
              name: 'Backend Expert',
              domain: 'backend',
              skill_tags: ['nodejs', 'databases', 'apis'],
              proficiency_scores: { backend: 0.88 },
              success_rate: 0.84
            },
            match_score: 0.78
          }]);
        }
        return Promise.resolve([]);
      });

      const request: CapabilityRequest = {
        target_domains: ['frontend', 'backend'],
        task_description: 'Build full-stack web application with React frontend and Node.js backend',
        performance_requirements: [
          { domain: 'frontend', min_proficiency: 0.8, critical_skills: ['react'], context_understanding: 0.75 },
          { domain: 'backend', min_proficiency: 0.8, critical_skills: ['nodejs'], context_understanding: 0.75 }
        ],
        integration_complexity: 'moderate',
        time_constraints: 30000,
        quality_threshold: 0.75
      };

      const result = await synthesis.synthesizeCapability(request);

      expect(result.synthesis_strategy).toBe('layer_composition');
      expect(result.confidence).toBeGreaterThan(0.5);
      expect(result.lora_composition.primary_layers.length).toBeGreaterThan(0);
    });

    it('selects fine-tuning for partial matches', async () => {
      // Mock scenario with partial matches that need adaptation
      mockSearchIndex.searchPersonas.mockResolvedValue([{
        persona: {
          id: 'partial_match',
          name: 'Partial Match',
          domain: 'general_science',
          skill_tags: ['chemistry', 'research', 'analysis'],
          proficiency_scores: { chemistry: 0.6, research: 0.8 },
          success_rate: 0.75
        },
        match_score: 0.4 // Low match score
      }]);

      const request: CapabilityRequest = {
        target_domains: ['specialized_chemistry'],
        task_description: 'Perform advanced organometallic synthesis',
        performance_requirements: [{
          domain: 'specialized_chemistry',
          min_proficiency: 0.9,
          critical_skills: ['organometallic_synthesis', 'catalysis'],
          context_understanding: 0.95
        }],
        integration_complexity: 'complex',
        time_constraints: 120000,
        quality_threshold: 0.85
      };

      const result = await synthesis.synthesizeCapability(request);

      expect(result.synthesis_strategy).toBe('fine_tune_required');
      expect(result.fine_tuning_plan).toBeDefined();
      expect(result.confidence).toBeLessThan(0.8);
    });

    it('selects novel creation for completely new domains', async () => {
      // Mock scenario with no existing components
      mockSearchIndex.searchPersonas.mockResolvedValue([]);
      mockLoRADiscovery.discoverAdapters.mockResolvedValue([]);

      const request: CapabilityRequest = {
        target_domains: ['quantum_consciousness', 'digital_telepathy'],
        task_description: 'Create AI system for quantum consciousness modeling in digital environments',
        performance_requirements: [
          { domain: 'quantum_consciousness', min_proficiency: 0.7, critical_skills: ['consciousness_modeling'], context_understanding: 0.8 },
          { domain: 'digital_telepathy', min_proficiency: 0.7, critical_skills: ['mind_reading'], context_understanding: 0.8 }
        ],
        integration_complexity: 'novel',
        time_constraints: 600000,
        quality_threshold: 0.6
      };

      const result = await synthesis.synthesizeCapability(request);

      expect(result.synthesis_strategy).toBe('novel_creation');
      expect(result.confidence).toBeLessThan(0.5);
      expect(result.creation_time_estimate).toBeGreaterThan(100000);
    });
  });

  describe('LoRA Composition Design', () => {
    it('designs appropriate layer architecture for multi-domain requests', async () => {
      mockSearchIndex.searchPersonas.mockImplementation((query: any) => {
        return Promise.resolve([{
          persona: {
            id: `${query.domain_filter[0]}_expert`,
            name: `${query.domain_filter[0]} Expert`,
            domain: query.domain_filter[0],
            skill_tags: [`${query.domain_filter[0]}_skill`],
            proficiency_scores: { [query.domain_filter[0]]: 0.8 },
            success_rate: 0.8
          },
          match_score: 0.8
        }]);
      });

      mockLoRADiscovery.discoverAdapters.mockResolvedValue([
        {
          id: 'domain1_adapter',
          name: 'Domain 1 LoRA',
          domain: 'domain1',
          category: 'Primary',
          rank: 32,
          alpha: 64,
          targetModules: ['q_proj', 'v_proj', 'k_proj'],
          dependencies: [],
          filePath: '.continuum/adapters/domain1',
          version: '1.0.0',
          author: 'Academy',
          description: 'Domain 1 expertise',
          isValid: true,
          warnings: [],
          errors: []
        },
        {
          id: 'domain2_adapter',
          name: 'Domain 2 LoRA',
          domain: 'domain2',
          category: 'Secondary',
          rank: 16,
          alpha: 32,
          targetModules: ['q_proj', 'v_proj'],
          dependencies: [],
          filePath: '.continuum/adapters/domain2',
          version: '1.0.0',
          author: 'Academy',
          description: 'Domain 2 expertise',
          isValid: true,
          warnings: [],
          errors: []
        }
      ]);

      const request: CapabilityRequest = {
        target_domains: ['domain1', 'domain2'],
        task_description: 'Combine domain1 and domain2 expertise',
        performance_requirements: [
          { domain: 'domain1', min_proficiency: 0.8, critical_skills: ['skill1'], context_understanding: 0.8 },
          { domain: 'domain2', min_proficiency: 0.7, critical_skills: ['skill2'], context_understanding: 0.7 }
        ],
        integration_complexity: 'moderate',
        time_constraints: 30000,
        quality_threshold: 0.75
      };

      const result = await synthesis.synthesizeCapability(request);

      expect(result.lora_composition).toBeDefined();
      expect(result.lora_composition.composition_algorithm).toBeDefined();
      expect(result.lora_composition.total_rank).toBeGreaterThan(0);
      expect(result.lora_composition.compression_efficiency).toBeGreaterThan(0);
      expect(result.lora_composition.compression_efficiency).toBeLessThanOrEqual(1);
    });

    it('identifies bridge layers for domain integration', async () => {
      // Mock scenario with overlapping domains that need bridging
      mockSearchIndex.searchPersonas.mockImplementation((query: any) => {
        return Promise.resolve([{
          persona: {
            id: `${query.domain_filter[0]}_expert`,
            name: `${query.domain_filter[0]} Expert`,
            domain: query.domain_filter[0],
            skill_tags: [`${query.domain_filter[0]}_skill`, 'integration'],
            proficiency_scores: { [query.domain_filter[0]]: 0.75 },
            success_rate: 0.8
          },
          match_score: 0.6 // Medium relevance = bridge candidate
        }]);
      });

      const request: CapabilityRequest = {
        target_domains: ['biochemistry', 'computational_biology'],
        task_description: 'Model protein folding using computational methods',
        performance_requirements: [
          { domain: 'biochemistry', min_proficiency: 0.85, critical_skills: ['protein_structure'], context_understanding: 0.9 },
          { domain: 'computational_biology', min_proficiency: 0.8, critical_skills: ['simulation'], context_understanding: 0.85 }
        ],
        integration_complexity: 'complex',
        time_constraints: 60000,
        quality_threshold: 0.8
      };

      const result = await synthesis.synthesizeCapability(request);

      // Should identify need for bridge layers between related domains
      if (result.synthesis_strategy === 'layer_composition') {
        expect(result.lora_composition.bridge_layers).toBeDefined();
        // At least some components should be identified as bridges
        expect(result.lora_composition.total_rank).toBeGreaterThan(0);
      }
    });
  });

  describe('Performance Estimation', () => {
    it('provides realistic performance estimates', async () => {
      mockSearchIndex.searchPersonas.mockResolvedValue([{
        persona: {
          id: 'test_expert',
          name: 'Test Expert',
          domain: 'testing',
          skill_tags: ['unit_testing', 'integration_testing'],
          proficiency_scores: { testing: 0.9 },
          success_rate: 0.85
        },
        match_score: 0.85
      }]);

      const request: CapabilityRequest = {
        target_domains: ['testing'],
        task_description: 'Write comprehensive test suites',
        performance_requirements: [{
          domain: 'testing',
          min_proficiency: 0.8,
          critical_skills: ['unit_testing'],
          context_understanding: 0.8
        }],
        integration_complexity: 'simple',
        time_constraints: 15000,
        quality_threshold: 0.8
      };

      const result = await synthesis.synthesizeCapability(request);

      expect(result.estimated_performance.overall_score).toBeGreaterThan(0);
      expect(result.estimated_performance.overall_score).toBeLessThanOrEqual(1);
      expect(result.estimated_performance.confidence_interval).toHaveLength(2);
      expect(result.estimated_performance.confidence_interval[0])
        .toBeLessThanOrEqual(result.estimated_performance.confidence_interval[1]);
      expect(result.estimated_performance.domain_scores).toBeDefined();
    });

    it('adjusts confidence based on component quality', async () => {
      // Test with high-quality components
      mockSearchIndex.searchPersonas.mockResolvedValue([{
        persona: {
          id: 'high_quality_expert',
          name: 'High Quality Expert',
          domain: 'quality_domain',
          skill_tags: ['expertise'],
          proficiency_scores: { quality_domain: 0.95 },
          success_rate: 0.98
        },
        match_score: 0.95
      }]);

      const highQualityRequest: CapabilityRequest = {
        target_domains: ['quality_domain'],
        task_description: 'High quality task',
        performance_requirements: [{
          domain: 'quality_domain',
          min_proficiency: 0.8,
          critical_skills: ['expertise'],
          context_understanding: 0.8
        }],
        integration_complexity: 'simple',
        time_constraints: 10000,
        quality_threshold: 0.8
      };

      const highQualityResult = await synthesis.synthesizeCapability(highQualityRequest);

      // Test with low-quality components
      mockSearchIndex.searchPersonas.mockResolvedValue([{
        persona: {
          id: 'low_quality_expert',
          name: 'Low Quality Expert',
          domain: 'basic_domain',
          skill_tags: ['basic_skill'],
          proficiency_scores: { basic_domain: 0.4 },
          success_rate: 0.5
        },
        match_score: 0.3
      }]);

      const lowQualityRequest: CapabilityRequest = {
        target_domains: ['basic_domain'],
        task_description: 'Basic task',
        performance_requirements: [{
          domain: 'basic_domain',
          min_proficiency: 0.8,
          critical_skills: ['basic_skill'],
          context_understanding: 0.8
        }],
        integration_complexity: 'simple',
        time_constraints: 10000,
        quality_threshold: 0.8
      };

      const lowQualityResult = await synthesis.synthesizeCapability(lowQualityRequest);

      // High quality should have higher confidence
      expect(highQualityResult.confidence).toBeGreaterThan(lowQualityResult.confidence);
    });
  });

  describe('Caching System', () => {
    it('caches synthesis results for identical requests', async () => {
      mockSearchIndex.searchPersonas.mockResolvedValue([{
        persona: {
          id: 'cache_test_expert',
          name: 'Cache Test Expert',
          domain: 'caching',
          skill_tags: ['cache_management'],
          proficiency_scores: { caching: 0.8 },
          success_rate: 0.8
        },
        match_score: 0.8
      }]);

      const request: CapabilityRequest = {
        target_domains: ['caching'],
        task_description: 'Test caching functionality',
        performance_requirements: [{
          domain: 'caching',
          min_proficiency: 0.7,
          critical_skills: ['cache_management'],
          context_understanding: 0.7
        }],
        integration_complexity: 'simple',
        time_constraints: 10000,
        quality_threshold: 0.7
      };

      // First call
      const result1 = await synthesis.synthesizeCapability(request);
      expect(mockSearchIndex.searchPersonas).toHaveBeenCalled();

      // Reset mock call count
      mockSearchIndex.searchPersonas.mockClear();

      // Second identical call
      const result2 = await synthesis.synthesizeCapability(request);

      // Should return cached result without calling search again
      expect(mockSearchIndex.searchPersonas).not.toHaveBeenCalled();
      expect(result1).toEqual(result2);
    });

    it('generates different cache keys for different requests', async () => {
      const request1: CapabilityRequest = {
        target_domains: ['domain1'],
        task_description: 'Task 1',
        performance_requirements: [{
          domain: 'domain1',
          min_proficiency: 0.8,
          critical_skills: ['skill1'],
          context_understanding: 0.8
        }],
        integration_complexity: 'simple',
        time_constraints: 10000,
        quality_threshold: 0.8
      };

      const request2: CapabilityRequest = {
        target_domains: ['domain2'],
        task_description: 'Task 2',
        performance_requirements: [{
          domain: 'domain2',
          min_proficiency: 0.7,
          critical_skills: ['skill2'],
          context_understanding: 0.7
        }],
        integration_complexity: 'complex',
        time_constraints: 20000,
        quality_threshold: 0.7
      };

      // Generate cache keys using the private method
      const cacheKey1 = (synthesis as any).generateCacheKey(request1);
      const cacheKey2 = (synthesis as any).generateCacheKey(request2);

      expect(cacheKey1).not.toEqual(cacheKey2);
    });
  });

  describe('Error Handling', () => {
    it('handles search index failures gracefully', async () => {
      mockSearchIndex.searchPersonas.mockRejectedValue(new Error('Search index error'));
      mockLoRADiscovery.discoverAdapters.mockResolvedValue([]);

      const request: CapabilityRequest = {
        target_domains: ['error_domain'],
        task_description: 'Test error handling',
        performance_requirements: [{
          domain: 'error_domain',
          min_proficiency: 0.5,
          critical_skills: ['error_handling'],
          context_understanding: 0.5
        }],
        integration_complexity: 'simple',
        time_constraints: 10000,
        quality_threshold: 0.5
      };

      await expect(synthesis.synthesizeCapability(request)).rejects.toThrow('Search index error');
    });

    it('handles LoRA discovery failures gracefully', async () => {
      mockSearchIndex.searchPersonas.mockResolvedValue([]);
      mockLoRADiscovery.discoverAdapters.mockRejectedValue(new Error('LoRA discovery error'));

      const request: CapabilityRequest = {
        target_domains: ['lora_error_domain'],
        task_description: 'Test LoRA error handling',
        performance_requirements: [{
          domain: 'lora_error_domain',
          min_proficiency: 0.5,
          critical_skills: ['lora_handling'],
          context_understanding: 0.5
        }],
        integration_complexity: 'simple',
        time_constraints: 10000,
        quality_threshold: 0.5
      };

      await expect(synthesis.synthesizeCapability(request)).rejects.toThrow('LoRA discovery error');
    });

    it('handles empty component candidates gracefully', async () => {
      mockSearchIndex.searchPersonas.mockResolvedValue([]);
      mockLoRADiscovery.discoverAdapters.mockResolvedValue([]);

      const request: CapabilityRequest = {
        target_domains: ['nonexistent_domain'],
        task_description: 'Test with no available components',
        performance_requirements: [{
          domain: 'nonexistent_domain',
          min_proficiency: 0.9,
          critical_skills: ['impossible_skill'],
          context_understanding: 0.9
        }],
        integration_complexity: 'novel',
        time_constraints: 10000,
        quality_threshold: 0.9
      };

      const result = await synthesis.synthesizeCapability(request);

      // Should default to novel creation with low confidence
      expect(result.synthesis_strategy).toBe('novel_creation');
      expect(result.confidence).toBeLessThan(0.5);
    });
  });
});