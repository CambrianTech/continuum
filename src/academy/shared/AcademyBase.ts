// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * AcademyBase - Abstract base class for all Academy implementations
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: AcademyBase abstract class methods
 * - Integration tests: Academy + persona management
 * - Evolution tests: Persona evolution and orchestration
 * - Ecosystem tests: Academy ecosystem monitoring
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Implements middle-out modular pattern with centralized shared logic
 * - Handles persona management, evolution orchestration, ecosystem monitoring
 * - Separation of burden: 80-90% shared complexity, 5-10% specific overrides
 * - Abstract base for client/server/integration implementations
 * 
 * This implements the middle-out modular pattern where shared logic is centralized
 * and specific implementations (client/server/integrations) extend this base.
 * 
 * Handles persona management, evolution orchestration, and ecosystem monitoring
 * with 80-90% of complexity in shared base, 5-10% in sparse overrides.
 */

import { PersonaGenome, EcosystemMetrics, EvolutionaryPressure, TrainingSession } from './AcademyTypes';

/**
 * Abstract base class for Academy implementations
 * Contains core Academy logic shared between all environments
 */
export abstract class AcademyBase<TInput, TOutput> {
  protected academyStatus: AcademyStatus;
  protected personas: Map<string, PersonaGenome> = new Map();
  protected activeSessions: Map<string, TrainingSession> = new Map();
  protected ecosystemMetrics: EcosystemMetrics;

  constructor() {
    this.academyStatus = this.initializeAcademyStatus();
    this.ecosystemMetrics = this.initializeEcosystemMetrics();
  }

  // ==================== ABSTRACT METHODS ====================
  // These must be implemented by client/server/integration classes

  /**
   * Create sandboxed session for persona evolution
   * Different environments handle sandboxing differently
   */
  abstract createSandboxedSession(persona: PersonaGenome): Promise<string>;

  /**
   * Clean up session resources
   * Environment-specific cleanup logic
   */
  abstract cleanupSession(sessionId: string): Promise<void>;

  /**
   * Execute challenge in sandbox
   * Platform-specific execution logic
   */
  abstract executeChallengeInSandbox(sessionId: string, challenge: any): Promise<any>;

  /**
   * Send message to external systems
   * Transport-specific messaging
   */
  abstract sendMessage(message: TInput): Promise<TOutput>;

  // ==================== SHARED CORE LOGIC ====================
  // 80-90% of Academy complexity lives here

  /**
   * Start evolution process with evolutionary pressure
   */
  async startEvolution(config: EvolutionConfig): Promise<EvolutionResult> {
    this.validateEvolutionConfig(config);
    
    this.academyStatus.mode = 'evolving';
    
    try {
      // Create initial population if needed
      if (this.personas.size === 0) {
        await this.createInitialPopulation(config.populationSize);
      }

      // Run evolution generations
      let currentPersonas = Array.from(this.personas.values());
      
      for (let generation = 1; generation <= config.generations; generation++) {
        this.logMessage(`⚡ Running Evolution Generation ${generation}/${config.generations}`);
        
        // Create sessions for each persona
        const sessionIds = await this.createEvolutionSessions(currentPersonas);
        
        // Run generation through evolution engine
        currentPersonas = await this.runGeneration(currentPersonas, config.evolutionaryPressure);
        
        // Update ecosystem
        this.updateEcosystem(currentPersonas);
        
        // Clean up sessions
        await this.cleanupEvolutionSessions(sessionIds);
        
        this.logMessage(`✅ Generation ${generation} complete: ${currentPersonas.length} personas`);
      }
      
      this.academyStatus.mode = 'idle';
      
      return {
        success: true,
        finalPopulation: currentPersonas.length,
        generationsCompleted: config.generations,
        ecosystemMetrics: this.ecosystemMetrics,
        ecosystemHealth: this.calculateEcosystemHealth()
      };
      
    } catch (error) {
      this.academyStatus.mode = 'idle';
      throw error;
    }
  }

  /**
   * Get comprehensive Academy status
   */
  getAcademyStatus(): AcademyStatus {
    return {
      ...this.academyStatus,
      totalPersonas: this.personas.size,
      activeSessions: this.activeSessions.size,
      ecosystemMetrics: this.ecosystemMetrics
    };
  }

  /**
   * Get persona by ID
   */
  getPersona(id: string): PersonaGenome | null {
    return this.personas.get(id) || null;
  }

  /**
   * Get all personas
   */
  getAllPersonas(): PersonaGenome[] {
    return Array.from(this.personas.values());
  }

  /**
   * Get persona lineage information
   */
  getPersonaLineage(personaId: string): PersonaLineageInfo | null {
    const persona = this.personas.get(personaId);
    if (!persona) return null;

    return {
      personaId,
      lineage: persona.lineage,
      evolution: persona.evolution,
      ancestors: this.getAncestorDetails(persona),
      descendants: this.getDescendantDetails(persona)
    };
  }

  /**
   * Get complete lineage tree for ecosystem
   */
  getLineageTree(): LineageTree[] {
    const personas = Array.from(this.personas.values());
    const tree: LineageTree[] = [];
    
    // Build family trees starting from generation 0
    const founders = personas.filter(p => p.evolution.generation === 0);
    
    for (const founder of founders) {
      tree.push({
        id: founder.id,
        name: (founder.identity as any).name || 'Unknown',
        generation: founder.evolution.generation,
        specialization: founder.identity.specialization,
        descendants: this.buildDescendantTree(founder, personas)
      });
    }
    
    return tree;
  }

  /**
   * Spawn new persona with specified configuration
   */
  async spawnPersona(config: PersonaSpawnConfig): Promise<PersonaGenome> {
    const persona = this.createPersonaGenome(config);
    this.personas.set(persona.id, persona);
    
    this.logMessage(`🧬 Spawned persona: ${(persona.identity as any).name || persona.id} (${persona.identity.specialization})`);
    
    return persona;
  }

  // ==================== PROTECTED HELPER METHODS ====================

  /**
   * Create initial population for evolution
   */
  protected async createInitialPopulation(size: number): Promise<void> {
    this.logMessage(`🌱 Creating initial population of ${size} personas`);
    
    const specializations = ['typescript', 'testing', 'architecture', 'ui_design', 'debugging', 'optimization'];
    const names = ['Alpha', 'Beta', 'Gamma', 'Delta', 'Epsilon', 'Zeta', 'Eta', 'Theta', 'Iota', 'Kappa'];
    
    for (let i = 0; i < size; i++) {
      const specialization = specializations[i % specializations.length];
      const name = `${specialization}${names[i % names.length]}`;
      
      await this.spawnPersona({
        name,
        specialization,
        role: 'student'
      });
    }
    
    this.logMessage(`✅ Initial population created: ${this.personas.size} personas`);
  }

  /**
   * Create evolution sessions for all personas
   */
  protected async createEvolutionSessions(personas: PersonaGenome[]): Promise<string[]> {
    const sessionIds: string[] = [];
    
    for (const persona of personas) {
      const sessionId = await this.createSandboxedSession(persona);
      sessionIds.push(sessionId);
    }
    
    return sessionIds;
  }

  /**
   * Clean up evolution sessions
   */
  protected async cleanupEvolutionSessions(sessionIds: string[]): Promise<void> {
    for (const sessionId of sessionIds) {
      await this.cleanupSession(sessionId);
    }
  }

  /**
   * Run one generation of evolution
   */
  protected async runGeneration(personas: PersonaGenome[], pressure: EvolutionaryPressure): Promise<PersonaGenome[]> {
    // TODO: Integrate with EvolutionEngine
    // For now, simulate evolution
    const survivors = this.selectSurvivors(personas, pressure);
    const offspring = await this.reproduceOffspring(survivors);
    
    return [...survivors, ...offspring];
  }

  /**
   * Update ecosystem with new personas
   */
  protected updateEcosystem(personas: PersonaGenome[]): void {
    this.personas.clear();
    
    for (const persona of personas) {
      this.personas.set(persona.id, persona);
    }
    
    this.updateEcosystemMetrics();
  }

  /**
   * Create a new PersonaGenome with default settings
   */
  protected createPersonaGenome(config: PersonaSpawnConfig): PersonaGenome {
    return {
      // ChatParticipant properties
      id: this.generateUUID(),
      name: config.name,
      type: 'persona',
      created: Date.now(),
      // canCommunicate: true, // Removed - not part of PersonaGenome interface
      
      // PersonaBase properties
      prompt: (config as any).prompt || `You are ${config.name}, a specialized ${config.specialization} persona.`,
      description: (config as any).description || `A ${config.specialization} specialist persona.`,
      
      // PersonaGenome properties
      identity: {
        role: config.role || 'student',
        generation: 0,
        specialization: config.specialization,
        personality: {
          creativity: 0.3 + Math.random() * 0.4,
          analytical: 0.3 + Math.random() * 0.4,
          helpfulness: 0.3 + Math.random() * 0.4,
          competitiveness: 0.3 + Math.random() * 0.4,
          patience: 0.3 + Math.random() * 0.4,
          innovation: 0.3 + Math.random() * 0.4
        },
        goals: [`master_${config.specialization}`, 'collaborate_effectively', 'teach_others']
      },
      knowledge: {
        domain: config.specialization,
        expertise: [config.specialization, 'problem_solving'],
        competencies: {
          [config.specialization]: 0.4 + Math.random() * 0.3,
          'problem_solving': 0.3 + Math.random() * 0.3,
          'collaboration': 0.2 + Math.random() * 0.3
        },
        experiencePoints: Math.floor(Math.random() * 500)
      },
      behavior: {
        learningStyle: 'analytical',
        adaptationRate: 0.3 + Math.random() * 0.4,
        communicationStyle: 'direct',
        decisionMakingStyle: 'analytical',
        riskTolerance: 0.3 + Math.random() * 0.4,
        collaborationPreference: 0.3 + Math.random() * 0.4
      },
      evolution: {
        generation: 0,
        parentGenomes: [],
        mutationHistory: [],
        evolutionStage: 'spawning',
        fitnessScore: 0.3 + Math.random() * 0.4,
        adaptationSuccess: 0,
        survivalRounds: 0,
        evolutionPressure: []
      },
      substrate: {
        loraIds: [`${config.specialization}_lora`, 'base_reasoning_lora'],
        memoryPatterns: ['short_term', 'working_memory'],
        processingStyle: 'sequential',
        adaptationMechanisms: ['reinforcement_learning', 'gradient_descent'],
        vectorPosition: Array.from({ length: 10 }, () => Math.random())
      },
      reproduction: {
        mutationRate: 0.1 + Math.random() * 0.1,
        reproductionEligibility: true,
        breedingSuccess: 0,
        offspringCount: 0
      },
      lineage: {
        ancestors: [],
        descendants: [],
        siblings: [],
        generation: 0,
        lineageStrength: 0.5,
        emergentTraits: []
      }
    };
  }

  /**
   * Validate evolution configuration
   */
  protected validateEvolutionConfig(config: EvolutionConfig): void {
    if (!config.generations || config.generations < 1) {
      throw new Error('Evolution must have at least 1 generation');
    }
    
    if (!config.populationSize || config.populationSize < 2) {
      throw new Error('Population size must be at least 2');
    }
    
    if (!config.evolutionaryPressure) {
      throw new Error('Evolutionary pressure configuration is required');
    }
  }

  /**
   * Select survivors based on evolutionary pressure
   */
  protected selectSurvivors(personas: PersonaGenome[], pressure: EvolutionaryPressure): PersonaGenome[] {
    const sortedPersonas = personas.sort((a, b) => b.evolution.fitnessScore - a.evolution.fitnessScore);
    const survivorCount = Math.floor(personas.length * pressure.survivalRate);
    
    return sortedPersonas.slice(0, survivorCount);
  }

  /**
   * Reproduce offspring from survivors
   */
  protected async reproduceOffspring(survivors: PersonaGenome[]): Promise<PersonaGenome[]> {
    const offspring: PersonaGenome[] = [];
    const targetOffspringCount = Math.floor(survivors.length * 1.5);
    
    for (let i = 0; i < targetOffspringCount; i++) {
      const parent1 = survivors[Math.floor(Math.random() * survivors.length)];
      const parent2 = survivors[Math.floor(Math.random() * survivors.length)];
      
      // TODO: Implement proper crossover from GenomeProcessor
      const child = this.simpleOffspring(parent1, parent2);
      offspring.push(child);
    }
    
    return offspring;
  }

  /**
   * Simple offspring creation (placeholder for GenomeProcessor)
   */
  protected simpleOffspring(parent1: PersonaGenome, parent2: PersonaGenome): PersonaGenome {
    const childConfig: PersonaSpawnConfig = {
      name: `${(parent1.identity as any).name || parent1.id}_${(parent2.identity as any).name || parent2.id}_child`,
      specialization: Math.random() < 0.5 ? parent1.identity.specialization : parent2.identity.specialization,
      role: 'student'
    };
    
    const child = this.createPersonaGenome(childConfig);
    child.evolution.generation = Math.max(parent1.evolution.generation, parent2.evolution.generation) + 1;
    child.evolution.parentGenomes = [parent1.id, parent2.id];
    child.lineage.ancestors = [...new Set([...parent1.lineage.ancestors, ...parent2.lineage.ancestors, parent1.id, parent2.id])];
    
    return child;
  }

  /**
   * Initialize Academy status
   */
  protected initializeAcademyStatus(): AcademyStatus {
    return {
      mode: 'idle',
      isActive: false,
      totalPersonas: 0,
      activeSessions: 0,
      uptime: Date.now(),
      ecosystemMetrics: this.initializeEcosystemMetrics()
    };
  }

  /**
   * Initialize ecosystem metrics
   */
  protected initializeEcosystemMetrics(): EcosystemMetrics {
    return {
      totalPersonas: 0,
      activePersonas: 0,
      averageFitness: 0,
      generationNumber: 0,
      diversityIndex: 0,
      innovationRate: 0,
      graduationRate: 0,
      extinctionRate: 0,
      emergentCapabilities: [],
      ecosystemAge: 0
    };
  }

  /**
   * Update ecosystem metrics
   */
  protected updateEcosystemMetrics(): void {
    const personas = Array.from(this.personas.values());
    
    this.ecosystemMetrics.totalPersonas = personas.length;
    this.ecosystemMetrics.activePersonas = personas.filter(p => p.evolution.evolutionStage !== 'extinct').length;
    
    if (personas.length > 0) {
      this.ecosystemMetrics.averageFitness = personas.reduce((sum, p) => sum + p.evolution.fitnessScore, 0) / personas.length;
      
      const specializations = new Set(personas.map(p => p.identity.specialization));
      this.ecosystemMetrics.diversityIndex = specializations.size / personas.length;
      
      const graduations = personas.filter(p => p.identity.role === 'teacher').length;
      this.ecosystemMetrics.graduationRate = graduations / personas.length;
    }
  }

  /**
   * Calculate ecosystem health
   */
  protected calculateEcosystemHealth(): EcosystemHealth {
    return {
      diversity: this.ecosystemMetrics.diversityIndex,
      innovation: this.ecosystemMetrics.innovationRate,
      collaboration: 0.7, // TODO: Calculate from actual collaboration metrics
      sustainability: 1 - this.ecosystemMetrics.extinctionRate,
      growth: this.ecosystemMetrics.averageFitness
    };
  }

  /**
   * Get ancestor details for a persona
   */
  protected getAncestorDetails(persona: PersonaGenome): PersonaLineageNode[] {
    return persona.lineage.ancestors.map(ancestorId => {
      const ancestor = this.personas.get(ancestorId);
      return ancestor ? {
        id: ancestorId,
        name: (ancestor.identity as any).name || ancestor.id,
        generation: ancestor.evolution.generation,
        specialization: ancestor.identity.specialization
      } : { id: ancestorId, name: 'Unknown', generation: -1, specialization: 'unknown' };
    });
  }

  /**
   * Get descendant details for a persona
   */
  protected getDescendantDetails(persona: PersonaGenome): PersonaLineageNode[] {
    return persona.lineage.descendants.map(descendantId => {
      const descendant = this.personas.get(descendantId);
      return descendant ? {
        id: descendantId,
        name: (descendant.identity as any).name || descendant.id,
        generation: descendant.evolution.generation,
        specialization: descendant.identity.specialization
      } : { id: descendantId, name: 'Unknown', generation: -1, specialization: 'unknown' };
    });
  }

  /**
   * Build descendant tree for lineage visualization
   */
  protected buildDescendantTree(ancestor: PersonaGenome, allPersonas: PersonaGenome[]): LineageTree[] {
    const descendants = allPersonas.filter(p => 
      p.lineage.ancestors.includes(ancestor.id)
    );
    
    return descendants.map(descendant => ({
      id: descendant.id,
      name: (descendant.identity as any).name || descendant.id,
      generation: descendant.evolution.generation,
      specialization: descendant.identity.specialization,
      descendants: this.buildDescendantTree(descendant, allPersonas)
    }));
  }

  /**
   * Generate UUID
   */
  protected generateUUID(): string {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
      const r = Math.random() * 16 | 0;
      const v = c === 'x' ? r : (r & 0x3 | 0x8);
      return v.toString(16);
    });
  }

  /**
   * Log message (to be overridden by implementations)
   */
  protected logMessage(message: string): void {
    console.log(`[Academy] ${message}`);
  }
}

// ==================== SUPPORTING INTERFACES ====================

export interface AcademyStatus {
  mode: 'idle' | 'training' | 'evaluating' | 'evolving';
  isActive: boolean;
  totalPersonas: number;
  activeSessions: number;
  uptime: number;
  ecosystemMetrics: EcosystemMetrics;
}

export interface EvolutionConfig {
  generations: number;
  populationSize: number;
  evolutionaryPressure: EvolutionaryPressure;
}

export interface EvolutionResult {
  success: boolean;
  finalPopulation: number;
  generationsCompleted: number;
  ecosystemMetrics: EcosystemMetrics;
  ecosystemHealth: EcosystemHealth;
}

export interface PersonaSpawnConfig {
  name: string;
  specialization: string;
  role?: 'student' | 'teacher' | 'meta-teacher';
}

export interface PersonaLineageInfo {
  personaId: string;
  lineage: any;
  evolution: any;
  ancestors: PersonaLineageNode[];
  descendants: PersonaLineageNode[];
}

export interface PersonaLineageNode {
  id: string;
  name: string;
  generation: number;
  specialization: string;
}

export interface LineageTree {
  id: string;
  name: string;
  generation: number;
  specialization: string;
  descendants: LineageTree[];
}

export interface EcosystemHealth {
  diversity: number;
  innovation: number;
  collaboration: number;
  sustainability: number;
  growth: number;
}