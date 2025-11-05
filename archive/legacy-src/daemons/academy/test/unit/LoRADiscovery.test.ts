/**
 * Unit Tests for LoRADiscovery - Genome Discovery Engine
 * 
 * Proves the Academy's LoRA adapter discovery and validation system works
 * as designed. These tests validate the core genome discovery functionality
 * that enables the AI evolution ecosystem.
 */

import { LoRADiscovery, LoRAMetadata, LayerInfo } from '../../LoRADiscovery';
import * as fs from 'fs/promises';
import * as path from 'path';
import { jest } from '@jest/globals';

// Mock filesystem for controlled testing
jest.mock('fs/promises');
const mockFs = fs as jest.Mocked<typeof fs>;

describe('LoRADiscovery - Genome Discovery Engine', () => {
  let discovery: LoRADiscovery;
  let testAdaptersDir: string;
  let testPersonasDir: string;

  beforeEach(() => {
    jest.clearAllMocks();
    testAdaptersDir = '.continuum/test-adapters';
    testPersonasDir = '.continuum/test-personas';
    discovery = new LoRADiscovery(testAdaptersDir, testPersonasDir);
  });

  describe('Adapter Discovery', () => {
    it('discovers all valid LoRA adapters in system', async () => {
      // Mock directory structure with valid adapters
      mockFs.stat.mockImplementation((filePath: any) => {
        const pathStr = filePath.toString();
        if (pathStr.includes('test-adapters') || pathStr.includes('typescript_expert') || pathStr.includes('base_language')) {
          return Promise.resolve({ isDirectory: () => true } as any);
        }
        throw new Error('Path not found');
      });

      mockFs.readdir.mockImplementation((dirPath: any) => {
        const pathStr = dirPath.toString();
        if (pathStr.includes('test-adapters')) {
          return Promise.resolve([
            { name: 'typescript_expert', isDirectory: () => true },
            { name: 'base_language', isDirectory: () => true }
          ] as any);
        }
        return Promise.resolve([]);
      });

      mockFs.access.mockImplementation((filePath: any) => {
        const pathStr = filePath.toString();
        if (pathStr.includes('adapter.json')) {
          return Promise.resolve();
        }
        throw new Error('File not found');
      });

      mockFs.readFile.mockImplementation((filePath: any) => {
        const pathStr = filePath.toString();
        if (pathStr.includes('typescript_expert/adapter.json')) {
          return Promise.resolve(JSON.stringify({
            id: 'typescript_expert',
            name: 'TypeScript Expert',
            domain: 'programming',
            category: 'Language',
            rank: 16,
            alpha: 32,
            targetModules: ['q_proj', 'v_proj', 'k_proj', 'o_proj'],
            dependencies: ['base_language'],
            version: '1.2.0',
            author: 'Academy Team',
            description: 'Expert-level TypeScript programming capabilities'
          }));
        }
        if (pathStr.includes('base_language/adapter.json')) {
          return Promise.resolve(JSON.stringify({
            id: 'base_language',
            name: 'Base Language Model',
            domain: 'language',
            category: 'Foundation',
            rank: 8,
            alpha: 16,
            targetModules: ['q_proj', 'v_proj'],
            dependencies: [],
            version: '2.0.0',
            author: 'Academy Team',
            description: 'Foundation language understanding'
          }));
        }
        throw new Error('File not found');
      });

      const adapters = await discovery.discoverAdapters();

      expect(adapters.length).toBeGreaterThan(0);
      expect(adapters.every(a => a.isValid)).toBe(true);
      expect(adapters.every(a => a.rank > 0 && a.alpha > 0)).toBe(true);
      
      // Verify specific adapters were found
      const tsExpert = adapters.find(a => a.id === 'typescript_expert');
      expect(tsExpert).toBeDefined();
      expect(tsExpert?.domain).toBe('programming');
      expect(tsExpert?.dependencies).toContain('base_language');
    });

    it('validates adapter metadata completeness', async () => {
      // Mock a complete adapter
      mockFs.stat.mockImplementation(() => Promise.resolve({ isDirectory: () => true } as any));
      mockFs.readdir.mockImplementation(() => Promise.resolve([
        { name: 'complete_adapter', isDirectory: () => true }
      ] as any));
      mockFs.access.mockImplementation(() => Promise.resolve());
      mockFs.readFile.mockImplementation(() => Promise.resolve(JSON.stringify({
        id: 'complete_adapter',
        name: 'Complete Test Adapter',
        domain: 'testing',
        category: 'Validation',
        rank: 16,
        alpha: 32,
        targetModules: ['q_proj', 'v_proj', 'k_proj', 'o_proj'],
        dependencies: [],
        version: '1.0.0',
        author: 'Test Suite',
        description: 'Complete adapter for validation testing'
      })));

      const adapters = await discovery.discoverAdapters();

      expect(adapters.length).toBeGreaterThan(0);
      
      for (const adapter of adapters) {
        expect(adapter.domain).toBeDefined();
        expect(adapter.targetModules.length).toBeGreaterThan(0);
        expect(adapter.version).toMatch(/^\d+\.\d+\.\d+$/);
        expect(adapter.filePath).toMatch(/\.continuum\/(test-adapters|test-personas)/);
        expect(adapter.rank).toBeGreaterThan(0);
        expect(adapter.alpha).toBeGreaterThan(0);
        expect(adapter.id).toBeDefined();
        expect(adapter.name).toBeDefined();
      }
    });

    it('handles invalid adapters gracefully', async () => {
      // Mock directory with invalid adapter
      mockFs.stat.mockImplementation(() => Promise.resolve({ isDirectory: () => true } as any));
      mockFs.readdir.mockImplementation(() => Promise.resolve([
        { name: 'invalid_adapter', isDirectory: () => true }
      ] as any));
      mockFs.access.mockImplementation(() => Promise.resolve());
      mockFs.readFile.mockImplementation(() => Promise.resolve(JSON.stringify({
        id: 'invalid_adapter',
        // Missing required fields like rank, alpha, targetModules
        domain: 'invalid'
      })));

      const consoleSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});

      const adapters = await discovery.discoverAdapters();

      // Should still return adapters but with validation errors
      expect(adapters.length).toBeGreaterThan(0);
      const invalidAdapter = adapters.find(a => a.id === 'invalid_adapter');
      expect(invalidAdapter).toBeDefined();
      expect(invalidAdapter?.isValid).toBe(false);
      expect(invalidAdapter?.errors.length).toBeGreaterThan(0);

      consoleSpy.mockRestore();
    });

    it('discovers adapters from personas directory', async () => {
      // Mock personas directory structure
      mockFs.stat.mockImplementation((filePath: any) => {
        const pathStr = filePath.toString();
        if (pathStr.includes('test-personas') || 
            pathStr.includes('persona1') || 
            pathStr.includes('adapters') ||
            pathStr.includes('specialized_adapter')) {
          return Promise.resolve({ isDirectory: () => true } as any);
        }
        throw new Error('Path not found');
      });

      mockFs.readdir.mockImplementation((dirPath: any) => {
        const pathStr = dirPath.toString();
        if (pathStr.includes('test-personas')) {
          return Promise.resolve([{ name: 'persona1', isDirectory: () => true }] as any);
        }
        if (pathStr.includes('persona1/adapters')) {
          return Promise.resolve([{ name: 'specialized_adapter', isDirectory: () => true }] as any);
        }
        if (pathStr.includes('test-adapters')) {
          return Promise.resolve([]);
        }
        return Promise.resolve([]);
      });

      mockFs.access.mockImplementation((filePath: any) => {
        const pathStr = filePath.toString();
        if (pathStr.includes('specialized_adapter/adapter.json')) {
          return Promise.resolve();
        }
        throw new Error('File not found');
      });

      mockFs.readFile.mockImplementation(() => Promise.resolve(JSON.stringify({
        id: 'specialized_adapter',
        name: 'Specialized Persona Adapter',
        domain: 'specialized',
        rank: 24,
        alpha: 48,
        targetModules: ['q_proj', 'v_proj'],
        dependencies: [],
        version: '1.0.0'
      })));

      const adapters = await discovery.discoverAdapters();

      expect(adapters.length).toBeGreaterThan(0);
      const specialized = adapters.find(a => a.id === 'specialized_adapter');
      expect(specialized).toBeDefined();
      expect(specialized?.filePath).toMatch(/persona1\/adapters/);
    });
  });

  describe('Adapter Stack Loading', () => {
    beforeEach(() => {
      // Mock a set of adapters with dependencies
      mockFs.stat.mockImplementation(() => Promise.resolve({ isDirectory: () => true } as any));
      mockFs.readdir.mockImplementation(() => Promise.resolve([
        { name: 'base_language_model', isDirectory: () => true },
        { name: 'typescript_specialist', isDirectory: () => true },
        { name: 'testing_expert', isDirectory: () => true }
      ] as any));
      mockFs.access.mockImplementation(() => Promise.resolve());
    });

    it('loads adapter stacks with dependency resolution', async () => {
      mockFs.readFile.mockImplementation((filePath: any) => {
        const pathStr = filePath.toString();
        if (pathStr.includes('base_language_model/adapter.json')) {
          return Promise.resolve(JSON.stringify({
            id: 'base_language_model',
            name: 'Base Language Model',
            domain: 'language',
            rank: 8,
            alpha: 16,
            targetModules: ['q_proj', 'v_proj'],
            dependencies: [],
            version: '1.0.0'
          }));
        }
        if (pathStr.includes('typescript_specialist/adapter.json')) {
          return Promise.resolve(JSON.stringify({
            id: 'typescript_specialist',
            name: 'TypeScript Specialist',
            domain: 'programming',
            rank: 16,
            alpha: 32,
            targetModules: ['q_proj', 'v_proj', 'k_proj'],
            dependencies: ['base_language_model'],
            version: '1.0.0'
          }));
        }
        if (pathStr.includes('testing_expert/adapter.json')) {
          return Promise.resolve(JSON.stringify({
            id: 'testing_expert',
            name: 'Testing Expert',
            domain: 'testing',
            rank: 12,
            alpha: 24,
            targetModules: ['q_proj', 'v_proj'],
            dependencies: ['typescript_specialist'],
            version: '1.0.0'
          }));
        }
        throw new Error('File not found');
      });

      const stack = await discovery.loadAdapterStack([
        'testing_expert',
        'typescript_specialist'
      ]);

      expect(stack.length).toBe(3); // Should include dependencies
      
      // Verify dependency order: dependencies come first
      const stackIds = stack.map(a => a.id);
      expect(stackIds.indexOf('base_language_model')).toBeLessThan(stackIds.indexOf('typescript_specialist'));
      expect(stackIds.indexOf('typescript_specialist')).toBeLessThan(stackIds.indexOf('testing_expert'));
    });

    it('handles circular dependencies gracefully', async () => {
      mockFs.readFile.mockImplementation((filePath: any) => {
        const pathStr = filePath.toString();
        if (pathStr.includes('base_language_model/adapter.json')) {
          return Promise.resolve(JSON.stringify({
            id: 'base_language_model',
            name: 'Base Language Model',
            domain: 'language',
            rank: 8,
            alpha: 16,
            targetModules: ['q_proj', 'v_proj'],
            dependencies: ['typescript_specialist'], // Circular dependency
            version: '1.0.0'
          }));
        }
        if (pathStr.includes('typescript_specialist/adapter.json')) {
          return Promise.resolve(JSON.stringify({
            id: 'typescript_specialist',
            name: 'TypeScript Specialist',
            domain: 'programming',
            rank: 16,
            alpha: 32,
            targetModules: ['q_proj', 'v_proj', 'k_proj'],
            dependencies: ['base_language_model'], // Circular dependency
            version: '1.0.0'
          }));
        }
        throw new Error('File not found');
      });

      // Should not throw due to circular dependency protection
      await expect(discovery.loadAdapterStack([
        'typescript_specialist'
      ])).resolves.not.toThrow();

      const stack = await discovery.loadAdapterStack(['typescript_specialist']);
      
      // Should still load both adapters despite circular dependency
      expect(stack.length).toBe(2);
      expect(stack.find(a => a.id === 'typescript_specialist')).toBeDefined();
      expect(stack.find(a => a.id === 'base_language_model')).toBeDefined();
    });

    it('throws error for missing dependencies', async () => {
      mockFs.readFile.mockImplementation((filePath: any) => {
        const pathStr = filePath.toString();
        if (pathStr.includes('typescript_specialist/adapter.json')) {
          return Promise.resolve(JSON.stringify({
            id: 'typescript_specialist',
            name: 'TypeScript Specialist',
            domain: 'programming',
            rank: 16,
            alpha: 32,
            targetModules: ['q_proj', 'v_proj'],
            dependencies: ['nonexistent_adapter'], // Missing dependency
            version: '1.0.0'
          }));
        }
        throw new Error('File not found');
      });

      await expect(discovery.loadAdapterStack(['typescript_specialist']))
        .rejects.toThrow('Adapter not found: nonexistent_adapter');
    });
  });

  describe('Model Layer Discovery', () => {
    it('discovers model layers for LoRA adaptation', async () => {
      const layers = await discovery.discoverModelLayers('test_model');

      expect(layers.length).toBeGreaterThan(0);
      
      // Verify all returned layers are adaptable
      expect(layers.every(layer => layer.adaptable)).toBe(true);
      
      // Check for expected layer types
      const hasAttention = layers.some(layer => layer.type === 'attention');
      const hasMLP = layers.some(layer => layer.type === 'mlp');
      expect(hasAttention).toBe(true);
      expect(hasMLP).toBe(true);
      
      // Verify layer structure
      for (const layer of layers) {
        expect(layer.name).toBeDefined();
        expect(layer.type).toMatch(/^(attention|mlp|embedding|output)$/);
        expect(layer.modules.length).toBeGreaterThan(0);
        expect(layer.dimensions.input).toBeGreaterThan(0);
        expect(layer.dimensions.output).toBeGreaterThan(0);
      }
    });

    it('filters out non-adaptable layers', async () => {
      const allLayers = await discovery.discoverModelLayers();
      
      // The method should only return adaptable layers
      expect(allLayers.every(layer => layer.adaptable)).toBe(true);
      
      // Verify that embedding and output layers are excluded
      const hasEmbedding = allLayers.some(layer => layer.type === 'embedding');
      const hasOutput = allLayers.some(layer => layer.type === 'output');
      expect(hasEmbedding).toBe(false);
      expect(hasOutput).toBe(false);
    });
  });

  describe('Adapter Validation', () => {
    it('validates rank and alpha parameters', async () => {
      mockFs.stat.mockImplementation(() => Promise.resolve({ isDirectory: () => true } as any));
      mockFs.readdir.mockImplementation(() => Promise.resolve([
        { name: 'invalid_rank_adapter', isDirectory: () => true }
      ] as any));
      mockFs.access.mockImplementation(() => Promise.resolve());
      mockFs.readFile.mockImplementation(() => Promise.resolve(JSON.stringify({
        id: 'invalid_rank_adapter',
        name: 'Invalid Rank Adapter',
        domain: 'testing',
        rank: -1, // Invalid rank
        alpha: -5, // Invalid alpha
        targetModules: ['q_proj'],
        dependencies: [],
        version: '1.0.0'
      })));

      const adapters = await discovery.discoverAdapters();
      const invalidAdapter = adapters.find(a => a.id === 'invalid_rank_adapter');

      expect(invalidAdapter).toBeDefined();
      expect(invalidAdapter?.isValid).toBe(false);
      expect(invalidAdapter?.errors).toContain('Invalid rank value: -1 (must be 1-512)');
      expect(invalidAdapter?.errors).toContain('Invalid alpha value: -5 (must be > 0)');
    });

    it('validates target modules presence', async () => {
      mockFs.stat.mockImplementation(() => Promise.resolve({ isDirectory: () => true } as any));
      mockFs.readdir.mockImplementation(() => Promise.resolve([
        { name: 'no_modules_adapter', isDirectory: () => true }
      ] as any));
      mockFs.access.mockImplementation(() => Promise.resolve());
      mockFs.readFile.mockImplementation(() => Promise.resolve(JSON.stringify({
        id: 'no_modules_adapter',
        name: 'No Modules Adapter',
        domain: 'testing',
        rank: 16,
        alpha: 32,
        targetModules: [], // Empty target modules
        dependencies: [],
        version: '1.0.0'
      })));

      const adapters = await discovery.discoverAdapters();
      const invalidAdapter = adapters.find(a => a.id === 'no_modules_adapter');

      expect(invalidAdapter).toBeDefined();
      expect(invalidAdapter?.isValid).toBe(false);
      expect(invalidAdapter?.errors).toContain('No target modules specified');
    });

    it('warns about missing weights files', async () => {
      mockFs.stat.mockImplementation(() => Promise.resolve({ isDirectory: () => true } as any));
      mockFs.readdir.mockImplementation(() => Promise.resolve([
        { name: 'no_weights_adapter', isDirectory: () => true }
      ] as any));
      
      mockFs.access.mockImplementation((filePath: any) => {
        const pathStr = filePath.toString();
        if (pathStr.includes('adapter.json')) {
          return Promise.resolve();
        }
        if (pathStr.includes('weights.safetensors')) {
          throw new Error('Weights file not found');
        }
        throw new Error('File not found');
      });

      mockFs.readFile.mockImplementation(() => Promise.resolve(JSON.stringify({
        id: 'no_weights_adapter',
        name: 'No Weights Adapter',
        domain: 'testing',
        rank: 16,
        alpha: 32,
        targetModules: ['q_proj'],
        dependencies: [],
        version: '1.0.0'
      })));

      const adapters = await discovery.discoverAdapters();
      const adapter = adapters.find(a => a.id === 'no_weights_adapter');

      expect(adapter).toBeDefined();
      expect(adapter?.isValid).toBe(true); // Still valid, just warning
      expect(adapter?.warnings).toContain('No weights file found - adapter may be incomplete');
    });
  });

  describe('Error Handling', () => {
    it('handles filesystem errors gracefully', async () => {
      // Mock filesystem errors
      mockFs.stat.mockRejectedValue(new Error('Filesystem error'));
      mockFs.readdir.mockRejectedValue(new Error('Readdir error'));

      const consoleSpy = jest.spyOn(console, 'error').mockImplementation(() => {});

      const adapters = await discovery.discoverAdapters();

      expect(adapters).toEqual([]);
      expect(consoleSpy).toHaveBeenCalledWith('LoRA discovery failed:', expect.any(Error));

      consoleSpy.mockRestore();
    });

    it('handles invalid JSON gracefully', async () => {
      mockFs.stat.mockImplementation(() => Promise.resolve({ isDirectory: () => true } as any));
      mockFs.readdir.mockImplementation(() => Promise.resolve([
        { name: 'invalid_json_adapter', isDirectory: () => true }
      ] as any));
      mockFs.access.mockImplementation(() => Promise.resolve());
      mockFs.readFile.mockImplementation(() => Promise.resolve('invalid json {'));

      const consoleSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});

      const adapters = await discovery.discoverAdapters();

      expect(adapters).toEqual([]);
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringContaining('Invalid adapter config'),
        expect.any(Error)
      );

      consoleSpy.mockRestore();
    });
  });
});