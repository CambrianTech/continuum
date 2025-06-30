/**
 * LoRA Discovery System - Dynamic layer discovery for local Academy training
 * 
 * Adapts WidgetDiscovery patterns for LoRA adapter management
 */

import * as fs from 'fs/promises';
import * as path from 'path';

export interface LoRAMetadata {
  id: string;
  name: string;
  domain: string;
  category: string;
  rank: number;
  alpha: number;
  targetModules: string[];
  dependencies: string[];
  filePath: string;
  version: string;
  author: string;
  description: string;
  isValid: boolean;
  warnings: string[];
  errors: string[];
}

export interface LayerInfo {
  name: string;
  type: 'attention' | 'mlp' | 'embedding' | 'output';
  modules: string[];
  dimensions: {
    input: number;
    output: number;
    hidden?: number;
  };
  adaptable: boolean;
}

export class LoRADiscovery {
  private adaptersDir: string;
  private personasDir: string;

  constructor(
    adaptersDir: string = '.continuum/adapters',
    personasDir: string = '.continuum/personas'
  ) {
    this.adaptersDir = adaptersDir;
    this.personasDir = personasDir;
  }

  /**
   * Discover all available LoRA adapters (adapts WidgetDiscovery.discoverWidgets pattern)
   */
  async discoverAdapters(): Promise<LoRAMetadata[]> {
    const adapters: LoRAMetadata[] = [];
    
    try {
      // Check both adapter directories and existing personas
      const adapterPaths = await this.getAdapterPaths();
      
      for (const adapterPath of adapterPaths) {
        const packageJsonPath = path.join(adapterPath, 'adapter.json');
        
        if (await this.fileExists(packageJsonPath)) {
          try {
            const adapterConfig = JSON.parse(await fs.readFile(packageJsonPath, 'utf-8'));
            const metadata = await this.analyzeAdapter(path.basename(adapterPath), adapterPath, adapterConfig);
            adapters.push(metadata);
          } catch (error) {
            // Skip invalid adapter configurations
            console.warn(`Invalid adapter config at ${packageJsonPath}:`, error);
          }
        }
      }

      console.log(`🧬 Discovered ${adapters.length} LoRA adapters`);
      return adapters;
    } catch (error) {
      console.error('LoRA discovery failed:', error);
      return [];
    }
  }

  /**
   * Analyze individual adapter (adapts WidgetDiscovery.analyzeWidget pattern)
   */
  private async analyzeAdapter(name: string, adapterPath: string, adapterConfig: any): Promise<LoRAMetadata> {
    const metadata: LoRAMetadata = {
      id: adapterConfig.id || name,
      name: adapterConfig.name || name,
      domain: adapterConfig.domain || 'unknown',
      category: adapterConfig.category || 'General',
      rank: adapterConfig.rank || 16,
      alpha: adapterConfig.alpha || 32,
      targetModules: adapterConfig.targetModules || ['q_proj', 'v_proj', 'k_proj', 'o_proj'],
      dependencies: adapterConfig.dependencies || [],
      filePath: adapterPath,
      version: adapterConfig.version || '1.0.0',
      author: adapterConfig.author || 'unknown',
      description: adapterConfig.description || '',
      isValid: true,
      warnings: [],
      errors: []
    };

    // Validation (adapts WidgetDiscovery validation patterns)
    await this.validateAdapter(metadata, adapterPath);

    return metadata;
  }

  /**
   * Validate adapter structure and dependencies
   */
  private async validateAdapter(metadata: LoRAMetadata, adapterPath: string): Promise<void> {
    // Check for required files
    const requiredFiles = ['adapter.json'];
    const optionalFiles = ['weights.safetensors', 'config.json', 'training_log.jsonl'];
    
    for (const file of requiredFiles) {
      const filePath = path.join(adapterPath, file);
      if (!(await this.fileExists(filePath))) {
        metadata.errors.push(`Missing required file: ${file}`);
        metadata.isValid = false;
      }
    }

    // Check for weights file
    const weightsPath = path.join(adapterPath, 'weights.safetensors');
    if (!(await this.fileExists(weightsPath))) {
      metadata.warnings.push('No weights file found - adapter may be incomplete');
    }

    // Validate rank and alpha values
    if (metadata.rank <= 0 || metadata.rank > 512) {
      metadata.errors.push(`Invalid rank value: ${metadata.rank} (must be 1-512)`);
      metadata.isValid = false;
    }

    if (metadata.alpha <= 0) {
      metadata.errors.push(`Invalid alpha value: ${metadata.alpha} (must be > 0)`);
      metadata.isValid = false;
    }

    // Check target modules
    if (!metadata.targetModules || metadata.targetModules.length === 0) {
      metadata.errors.push('No target modules specified');
      metadata.isValid = false;
    }
  }

  /**
   * Get all potential adapter paths
   */
  private async getAdapterPaths(): Promise<string[]> {
    const paths: string[] = [];
    
    // Check dedicated adapters directory
    if (await this.directoryExists(this.adaptersDir)) {
      const adapterDirs = await this.getDirectories(this.adaptersDir);
      paths.push(...adapterDirs);
    }

    // Check personas directory for existing adapters
    if (await this.directoryExists(this.personasDir)) {
      const personaDirs = await this.getDirectories(this.personasDir);
      for (const personaDir of personaDirs) {
        const adaptersSubdir = path.join(personaDir, 'adapters');
        if (await this.directoryExists(adaptersSubdir)) {
          const personaAdapters = await this.getDirectories(adaptersSubdir);
          paths.push(...personaAdapters);
        }
      }
    }

    return paths;
  }

  /**
   * Discover model layers for LoRA adaptation
   */
  async discoverModelLayers(modelName: string = 'base_model'): Promise<LayerInfo[]> {
    // Simulate model introspection - in real implementation this would
    // connect to actual model architecture discovery
    const layers: LayerInfo[] = [
      {
        name: 'attention',
        type: 'attention',
        modules: ['q_proj', 'k_proj', 'v_proj', 'o_proj'],
        dimensions: { input: 4096, output: 4096 },
        adaptable: true
      },
      {
        name: 'mlp',
        type: 'mlp', 
        modules: ['gate_proj', 'up_proj', 'down_proj'],
        dimensions: { input: 4096, output: 11008, hidden: 4096 },
        adaptable: true
      },
      {
        name: 'embed_tokens',
        type: 'embedding',
        modules: ['embed_tokens'],
        dimensions: { input: 32000, output: 4096 },
        adaptable: false // Usually not adapted
      },
      {
        name: 'lm_head',
        type: 'output',
        modules: ['lm_head'],
        dimensions: { input: 4096, output: 32000 },
        adaptable: false // Usually not adapted
      }
    ];

    console.log(`🔍 Discovered ${layers.length} model layers for ${modelName}`);
    return layers.filter(layer => layer.adaptable);
  }

  /**
   * Load adapter stack with dependency resolution
   */
  async loadAdapterStack(adapterIds: string[]): Promise<LoRAMetadata[]> {
    const allAdapters = await this.discoverAdapters();
    const adapterMap = new Map<string, LoRAMetadata>();
    
    // Create adapter lookup map
    allAdapters.forEach(adapter => {
      adapterMap.set(adapter.id, adapter);
    });

    // Resolve dependencies and build stack
    const resolvedStack: LoRAMetadata[] = [];
    const visited = new Set<string>();

    for (const adapterId of adapterIds) {
      await this.resolveDependencies(adapterId, adapterMap, resolvedStack, visited);
    }

    console.log(`📚 Loaded adapter stack with ${resolvedStack.length} adapters`);
    return resolvedStack;
  }

  /**
   * Recursive dependency resolution
   */
  private async resolveDependencies(
    adapterId: string,
    adapterMap: Map<string, LoRAMetadata>,
    stack: LoRAMetadata[],
    visited: Set<string>
  ): Promise<void> {
    if (visited.has(adapterId)) return;
    
    const adapter = adapterMap.get(adapterId);
    if (!adapter) {
      throw new Error(`Adapter not found: ${adapterId}`);
    }

    visited.add(adapterId);

    // Resolve dependencies first
    for (const depId of adapter.dependencies) {
      await this.resolveDependencies(depId, adapterMap, stack, visited);
    }

    // Add this adapter to stack
    if (!stack.find(a => a.id === adapterId)) {
      stack.push(adapter);
    }
  }

  // Utility methods (adapted from WidgetDiscovery)
  private async getDirectories(parentDir: string): Promise<string[]> {
    try {
      const items = await fs.readdir(parentDir, { withFileTypes: true });
      return items
        .filter(item => item.isDirectory())
        .map(item => path.join(parentDir, item.name));
    } catch {
      return [];
    }
  }

  private async fileExists(filePath: string): Promise<boolean> {
    try {
      await fs.access(filePath);
      return true;
    } catch {
      return false;
    }
  }

  private async directoryExists(dirPath: string): Promise<boolean> {
    try {
      const stat = await fs.stat(dirPath);
      return stat.isDirectory();
    } catch {
      return false;
    }
  }
}