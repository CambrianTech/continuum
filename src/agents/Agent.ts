/**
 * Proper Agent Interface and Base Class
 */

import { ModelDiscoveryService, AvailableModels } from '../services/ModelDiscoveryService.js';

export interface AgentConfig {
  name: string;
  role: string;
  provider?: 'anthropic' | 'openai';
  model?: string;
  temperature?: number;
  maxTokens?: number;
}

export interface AgentResponse {
  result: string;
  cost: number;
  model: string;
  tokens?: {
    input: number;
    output: number;
  };
}

export abstract class Agent {
  protected config: AgentConfig;
  protected modelService: ModelDiscoveryService;

  constructor(config: AgentConfig, modelService: ModelDiscoveryService) {
    this.config = config;
    this.modelService = modelService;
  }

  getName(): string {
    return this.config.name;
  }

  getRole(): string {
    return this.config.role;
  }

  getAvailableModels(): AvailableModels {
    return this.modelService.getAvailableModels();
  }

  getBestModel(provider?: 'anthropic' | 'openai'): string {
    return this.modelService.getBestModel(provider || this.config.provider);
  }

  protected getModelToUse(): string {
    if (this.config.model && this.modelService.hasModel(this.config.model)) {
      return this.config.model;
    }
    return this.getBestModel();
  }

  abstract execute(task: string, context?: any): Promise<AgentResponse>;
}

export class GeneralAI extends Agent {
  constructor(modelService: ModelDiscoveryService) {
    super({
      name: 'GeneralAI',
      role: 'General assistant for analysis and coordination',
      provider: 'anthropic',
      temperature: 0.7,
      maxTokens: 1000
    }, modelService);
  }

  async execute(task: string, context?: any): Promise<AgentResponse> {
    const model = this.getModelToUse();
    
    // Implementation would call the actual AI API
    return {
      result: `GeneralAI (${model}) processing: ${task}`,
      cost: 0.01,
      model,
      tokens: { input: 100, output: 50 }
    };
  }
}

export class CodeAI extends Agent {
  constructor(modelService: ModelDiscoveryService) {
    super({
      name: 'CodeAI', 
      role: 'Code analysis and implementation specialist',
      provider: 'anthropic',
      temperature: 0.3,
      maxTokens: 2000
    }, modelService);
  }

  async execute(task: string, context?: any): Promise<AgentResponse> {
    const model = this.getModelToUse();
    
    return {
      result: `CodeAI (${model}) processing: ${task}`,
      cost: 0.02,
      model,
      tokens: { input: 150, output: 200 }
    };
  }
}