/**
 * Agent Factory - Creates and manages typed agents
 */

import { Agent, GeneralAI, CodeAI } from './Agent.js';
import { ModelDiscoveryService } from '../services/ModelDiscoveryService.js';

export type AgentType = 'GeneralAI' | 'CodeAI' | 'PlannerAI';

export class AgentFactory {
  private modelService: ModelDiscoveryService;
  private agents = new Map<string, Agent>();

  constructor(modelService: ModelDiscoveryService) {
    this.modelService = modelService;
  }

  createAgent(type: AgentType): Agent {
    const existingAgent = this.agents.get(type);
    if (existingAgent) {
      return existingAgent;
    }

    let agent: Agent;
    
    switch (type) {
      case 'GeneralAI':
        agent = new GeneralAI(this.modelService);
        break;
      case 'CodeAI':
        agent = new CodeAI(this.modelService);
        break;
      case 'PlannerAI':
        // Could extend with PlannerAI class
        agent = new GeneralAI(this.modelService);
        break;
      default:
        throw new Error(`Unknown agent type: ${type}`);
    }

    this.agents.set(type, agent);
    console.log(`🚀 Created ${type} agent using model: ${agent.getBestModel()}`);
    
    return agent;
  }

  getAgent(type: AgentType): Agent | undefined {
    return this.agents.get(type);
  }

  getAllAgents(): Map<string, Agent> {
    return new Map(this.agents);
  }

  getAvailableModels() {
    return this.modelService.getAvailableModels();
  }
}