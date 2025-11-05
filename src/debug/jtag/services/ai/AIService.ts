/**
 * AI Service - Business Logic for AI System Integration
 * 
 * Integrates the Academy competitive training system, genomic LoRA layers,
 * cosine similarity search, and persona management using clean API types
 * and existing router/transport system.
 * 
 * Provides high-level AI operations for:
 * - Persona creation and management (LoRA-adapted models)
 * - Academy competitive training sessions
 * - Genomic search for optimal LoRA combinations  
 * - Real-time AI conversations and tool integration
 */

import { ServiceBase } from '../shared/ServiceBase';
import type { IServiceTransport } from '../shared/ServiceBase';
import type { JTAGContext } from '../../system/core/types/JTAGTypes';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { PersonaUser, AgentUser, PersonaConfig, AgentConfig } from '../../api/types/User';

// Import Academy types for competitive training
export interface AcademySession {
  sessionId: UUID;
  modality: 'speed-round' | 'marathon-session' | 'battle-royale' | 'team-challenge';
  participants: UUID[];
  currentScore: Record<UUID, number>;
  status: 'preparing' | 'active' | 'completed';
}

// Import Genomic types for LoRA layer management
export interface GenomicSearchQuery {
  requirements: string[];
  specialization: string;
  proficiencyThreshold: number;
  maxLayers: number;
}

export interface GenomicLoRALayer {
  layerId: UUID;
  name: string;
  specialization: string;
  proficiencyLevel: number;
  performanceMetrics: Record<string, number>;
  embedding: Float32Array; // 512-dimensional vector
}

export interface IAIService {
  // Persona Management
  createPersona(config: PersonaConfig): Promise<PersonaUser>;
  createAgent(config: AgentConfig): Promise<AgentUser>;
  listPersonas(): Promise<PersonaUser[]>;
  getPersonaById(personaId: UUID): Promise<PersonaUser | null>;
  
  // Academy Competitive Training
  startAcademySession(modality: AcademySession['modality'], participants: UUID[]): Promise<AcademySession>;
  joinAcademySession(sessionId: UUID, personaId: UUID): Promise<boolean>;
  getActiveAcademySessions(): Promise<AcademySession[]>;
  getSessionLeaderboard(sessionId: UUID): Promise<Record<UUID, number>>;
  
  // Genomic LoRA Search & Assembly
  searchGenomicLayers(query: GenomicSearchQuery): Promise<GenomicLoRALayer[]>;
  assembleOptimalGenome(requirements: string[]): Promise<GenomicLoRALayer[]>;
  evaluateGenomicCompatibility(layers: GenomicLoRALayer[]): Promise<number>;
  
  // AI Conversation & Tool Integration
  sendPersonaMessage(personaId: UUID, message: string, context?: Record<string, any>): Promise<string>;
  executeAgentCommand(agentId: UUID, command: string, params: Record<string, any>): Promise<any>;
  getAICapabilities(aiUserId: UUID): Promise<string[]>;
  
  // Performance Monitoring
  getAIPerformanceMetrics(aiUserId: UUID): Promise<Record<string, number>>;
  updateAIProfile(aiUserId: UUID, performance: Record<string, number>): Promise<boolean>;
}

export class AIService extends ServiceBase implements IAIService {
  private personaCache = new Map<UUID, PersonaUser>();
  private agentCache = new Map<UUID, AgentUser>();
  private genomicCache = new Map<string, GenomicLoRALayer[]>();
  
  constructor(transport: IServiceTransport, context: JTAGContext) {
    super('AIService', transport, context);
  }

  async createPersona(config: PersonaConfig): Promise<PersonaUser> {
    // Use API factory (PersonaUser constructor from API types)
    const persona = new PersonaUser(config);
    
    // Persist through transport system
    const result = await this.executeCommand('ai/create-persona', { persona: config });
    
    if (!result.success) {
      throw new Error(`Failed to create persona: ${result.error}`);
    }
    
    this.personaCache.set(persona.id, persona);
    return persona;
  }

  async createAgent(config: AgentConfig): Promise<AgentUser> {
    // Use API factory (AgentUser constructor from API types)
    const agent = new AgentUser(config);
    
    // Persist through transport system with JTAG integration
    const result = await this.executeCommand('ai/create-agent', { 
      agent: config,
      jtagEnabled: config.integration.jtagEnabled
    });
    
    if (!result.success) {
      throw new Error(`Failed to create agent: ${result.error}`);
    }
    
    this.agentCache.set(agent.id, agent);
    return agent;
  }

  async listPersonas(): Promise<PersonaUser[]> {
    const result = await this.executeCommand('ai/list-personas', {});
    
    if (result.success && result.personas) {
      return result.personas.map((config: PersonaConfig) => new PersonaUser(config));
    }
    
    return [];
  }

  async getPersonaById(personaId: UUID): Promise<PersonaUser | null> {
    // Check cache first
    if (this.personaCache.has(personaId)) {
      return this.personaCache.get(personaId)!;
    }

    const result = await this.executeCommand('ai/get-persona', { personaId });
    
    if (result.success && result.persona) {
      const persona = new PersonaUser(result.persona);
      this.personaCache.set(personaId, persona);
      return persona;
    }
    
    return null;
  }

  // Academy Competitive Training
  async startAcademySession(modality: AcademySession['modality'], participants: UUID[]): Promise<AcademySession> {
    const result = await this.executeCommand('academy/start-session', { modality, participants });
    
    if (!result.success) {
      throw new Error(`Failed to start Academy session: ${result.error}`);
    }
    
    return result.session;
  }

  async joinAcademySession(sessionId: UUID, personaId: UUID): Promise<boolean> {
    const result = await this.executeCommand('academy/join-session', { sessionId, personaId });
    return result.success;
  }

  async getActiveAcademySessions(): Promise<AcademySession[]> {
    const result = await this.executeCommand('academy/list-active-sessions', {});
    return result.success ? result.sessions : [];
  }

  async getSessionLeaderboard(sessionId: UUID): Promise<Record<UUID, number>> {
    const result = await this.executeCommand('academy/get-leaderboard', { sessionId });
    return result.success ? result.leaderboard : {};
  }

  // Genomic LoRA Search & Assembly
  async searchGenomicLayers(query: GenomicSearchQuery): Promise<GenomicLoRALayer[]> {
    // Check cache first
    const cacheKey = JSON.stringify(query);
    if (this.genomicCache.has(cacheKey)) {
      return this.genomicCache.get(cacheKey)!;
    }

    const result = await this.executeCommand('genomic/search-layers', query);
    
    if (result.success && result.layers) {
      this.genomicCache.set(cacheKey, result.layers);
      return result.layers;
    }
    
    return [];
  }

  async assembleOptimalGenome(requirements: string[]): Promise<GenomicLoRALayer[]> {
    const result = await this.executeCommand('genomic/assemble-optimal', { requirements });
    return result.success ? result.assembly : [];
  }

  async evaluateGenomicCompatibility(layers: GenomicLoRALayer[]): Promise<number> {
    const result = await this.executeCommand('genomic/evaluate-compatibility', { layers });
    return result.success ? result.compatibilityScore : 0;
  }

  // AI Conversation & Tool Integration
  async sendPersonaMessage(personaId: UUID, message: string, context?: Record<string, any>): Promise<string> {
    const result = await this.executeCommand('ai/persona-chat', { personaId, message, context });
    return result.success ? result.response : '';
  }

  async executeAgentCommand(agentId: UUID, command: string, params: Record<string, any>): Promise<any> {
    const result = await this.executeCommand('ai/agent-execute', { agentId, command, params });
    return result.success ? result.result : null;
  }

  async getAICapabilities(aiUserId: UUID): Promise<string[]> {
    const result = await this.executeCommand('ai/get-capabilities', { aiUserId });
    return result.success ? result.capabilities : [];
  }

  // Performance Monitoring
  async getAIPerformanceMetrics(aiUserId: UUID): Promise<Record<string, number>> {
    const result = await this.executeCommand('ai/get-performance', { aiUserId });
    return result.success ? result.metrics : {};
  }

  async updateAIProfile(aiUserId: UUID, performance: Record<string, number>): Promise<boolean> {
    const result = await this.executeCommand('ai/update-performance', { aiUserId, performance });
    return result.success;
  }
}