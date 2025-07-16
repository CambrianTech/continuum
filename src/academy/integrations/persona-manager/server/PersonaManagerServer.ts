/**
 * PersonaManagerServer - Server-side persona management implementation
 * 
 * Extends ServerAcademy with persona-specific operations
 * Handles file system storage, process management, and daemon communication
 */

import { ServerAcademy } from '../../../server/ServerAcademy';
import { PersonaGenome, generateUUID, validatePersonaGenome } from '../../../shared/AcademyTypes';
import { 
  CreatePersonaRequest, 
  CreatePersonaResponse,
  PersonaSearchRequest,
  PersonaSearchResponse,
  PersonaUpdateRequest,
  PersonaUpdateResponse,
  PersonaCrossoverRequest,
  PersonaCrossoverResponse,
  PersonaStatisticsRequest,
  PersonaStatisticsResponse,
  PersonaManagerOperation,
  PersonaManagerStatus,
  PersonaValidationRequest,
  PersonaValidationResponse
} from '../shared/PersonaManagerTypes';
import { promises as fs } from 'fs';
import { join } from 'path';

/**
 * Server-side persona manager implementation
 */
export class PersonaManagerServer extends ServerAcademy<PersonaManagerRequest, PersonaManagerResponse> {
  private personaStoragePath: string;
  private operationHistory: PersonaManagerOperation[] = [];
  private responseTimesMs: number[] = [];

  constructor(daemonClient: any, storagePath: string = '.continuum/personas') {
    super(daemonClient);
    this.personaStoragePath = storagePath;
    this.ensureStorageDirectory();
  }

  // ==================== PERSONA MANAGEMENT OPERATIONS ====================

  /**
   * Create new persona with specified configuration
   */
  async createPersona(request: CreatePersonaRequest): Promise<CreatePersonaResponse> {
    const startTime = Date.now();
    const operationId = generateUUID();

    try {
      // Validate request
      if (!request.name || !request.specialization) {
        throw new Error('Name and specialization are required');
      }

      // Check for duplicate names
      const existingPersonas = await this.loadAllPersonas();
      if (existingPersonas.some(p => p.identity.name === request.name)) {
        throw new Error(`Persona with name '${request.name}' already exists`);
      }

      // Create persona genome
      const persona = await this.spawnPersona({
        name: request.name,
        specialization: request.specialization,
        role: request.role
      });

      // Apply custom traits if provided
      if (request.customTraits) {
        this.applyCustomTraits(persona, request.customTraits);
      }

      // Set parent relationships if provided
      if (request.parentIds && request.parentIds.length > 0) {
        persona.evolution.parentGenomes = request.parentIds;
        persona.lineage.ancestors = request.parentIds;
        persona.evolution.generation = await this.calculateChildGeneration(request.parentIds);
      }

      // Save persona to storage
      await this.savePersona(persona);

      // Record operation
      this.recordOperation({
        type: 'create',
        timestamp: new Date(),
        personaId: persona.id,
        operationId,
        success: true,
        metadata: { name: request.name, specialization: request.specialization }
      });

      this.recordResponseTime(Date.now() - startTime);

      return {
        success: true,
        persona
      };

    } catch (error) {
      this.recordOperation({
        type: 'create',
        timestamp: new Date(),
        operationId,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });

      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Search for personas based on filters
   */
  async searchPersonas(request: PersonaSearchRequest): Promise<PersonaSearchResponse> {
    const startTime = Date.now();
    const operationId = generateUUID();

    try {
      const allPersonas = await this.loadAllPersonas();
      let filteredPersonas = allPersonas;

      // Apply filters
      if (request.filters) {
        filteredPersonas = this.applyFilters(filteredPersonas, request.filters);
      }

      // Apply sorting
      if (request.sortBy) {
        filteredPersonas = this.sortPersonas(filteredPersonas, request.sortBy, request.sortOrder);
      }

      // Apply pagination
      const offset = request.offset || 0;
      const limit = request.limit || 50;
      const total = filteredPersonas.length;
      const personas = filteredPersonas.slice(offset, offset + limit);
      const hasMore = (offset + limit) < total;

      this.recordOperation({
        type: 'search',
        timestamp: new Date(),
        operationId,
        success: true,
        metadata: { 
          totalResults: total, 
          returned: personas.length,
          filters: request.filters 
        }
      });

      this.recordResponseTime(Date.now() - startTime);

      return {
        success: true,
        personas,
        total,
        hasMore
      };

    } catch (error) {
      this.recordOperation({
        type: 'search',
        timestamp: new Date(),
        operationId,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });

      return {
        success: false,
        personas: [],
        total: 0,
        hasMore: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Update existing persona
   */
  async updatePersona(request: PersonaUpdateRequest): Promise<PersonaUpdateResponse> {
    const startTime = Date.now();
    const operationId = generateUUID();

    try {
      const persona = await this.loadPersona(request.personaId);
      if (!persona) {
        throw new Error(`Persona not found: ${request.personaId}`);
      }

      // Apply updates
      if (request.updates.fitness !== undefined) {
        persona.evolution.fitnessScore = request.updates.fitness;
      }
      
      if (request.updates.role) {
        persona.identity.role = request.updates.role;
      }
      
      if (request.updates.experiencePoints !== undefined) {
        persona.knowledge.experiencePoints = request.updates.experiencePoints;
      }
      
      if (request.updates.newTraits) {
        persona.lineage.emergentTraits.push(...request.updates.newTraits);
      }

      if (request.updates.mutationEvent) {
        persona.evolution.mutationHistory.push({
          timestamp: new Date(),
          type: request.updates.mutationEvent.type,
          changes: request.updates.mutationEvent.changes,
          trigger: request.updates.mutationEvent.trigger,
          outcome: 'beneficial' // Default, could be calculated
        });
      }

      // Save updated persona
      await this.savePersona(persona);

      this.recordOperation({
        type: 'update',
        timestamp: new Date(),
        personaId: request.personaId,
        operationId,
        success: true,
        metadata: { updates: Object.keys(request.updates) }
      });

      this.recordResponseTime(Date.now() - startTime);

      return {
        success: true,
        updatedPersona: persona
      };

    } catch (error) {
      this.recordOperation({
        type: 'update',
        timestamp: new Date(),
        personaId: request.personaId,
        operationId,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });

      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Create offspring from two parent personas
   */
  async crossoverPersonas(request: PersonaCrossoverRequest): Promise<PersonaCrossoverResponse> {
    const startTime = Date.now();
    const operationId = generateUUID();

    try {
      const parent1 = await this.loadPersona(request.parent1Id);
      const parent2 = await this.loadPersona(request.parent2Id);

      if (!parent1 || !parent2) {
        throw new Error('One or both parent personas not found');
      }

      // Create offspring using GenomeProcessor logic
      const offspring = await this.createOffspringFromParents(parent1, parent2, request);

      // Save offspring
      await this.savePersona(offspring);

      // Update parent lineages
      parent1.lineage.descendants.push(offspring.id);
      parent2.lineage.descendants.push(offspring.id);
      await this.savePersona(parent1);
      await this.savePersona(parent2);

      this.recordOperation({
        type: 'crossover',
        timestamp: new Date(),
        operationId,
        success: true,
        metadata: { 
          parent1: request.parent1Id,
          parent2: request.parent2Id,
          offspring: offspring.id 
        }
      });

      this.recordResponseTime(Date.now() - startTime);

      return {
        success: true,
        offspring,
        crossoverReport: {
          inheritedTraits: this.analyzeInheritedTraits(parent1, parent2, offspring),
          newTraits: [],
          mutationEvents: [],
          compatibilityScore: this.calculateCompatibilityScore(parent1, parent2)
        }
      };

    } catch (error) {
      this.recordOperation({
        type: 'crossover',
        timestamp: new Date(),
        operationId,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });

      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Validate persona genome
   */
  async validatePersona(request: PersonaValidationRequest): Promise<PersonaValidationResponse> {
    const startTime = Date.now();
    const operationId = generateUUID();

    try {
      const errors = [];
      const warnings = [];

      // Basic validation
      if (!validatePersonaGenome(request.persona)) {
        errors.push({
          field: 'structure',
          message: 'Invalid persona genome structure',
          severity: 'error' as const,
          code: 'INVALID_STRUCTURE'
        });
      }

      // Comprehensive validation
      if (request.validationLevel === 'comprehensive' || request.validationLevel === 'strict') {
        this.validatePersonaFields(request.persona, errors, warnings);
      }

      // Strict validation
      if (request.validationLevel === 'strict') {
        this.validatePersonaConstraints(request.persona, errors, warnings);
      }

      const isValid = errors.length === 0;

      this.recordOperation({
        type: 'validate',
        timestamp: new Date(),
        personaId: request.persona.id,
        operationId,
        success: true,
        metadata: { 
          validationLevel: request.validationLevel,
          isValid,
          errorCount: errors.length,
          warningCount: warnings.length 
        }
      });

      this.recordResponseTime(Date.now() - startTime);

      return {
        success: true,
        isValid,
        errors,
        warnings
      };

    } catch (error) {
      this.recordOperation({
        type: 'validate',
        timestamp: new Date(),
        operationId,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });

      return {
        success: false,
        isValid: false,
        errors: [{
          field: 'system',
          message: error instanceof Error ? error.message : String(error),
          severity: 'error' as const,
          code: 'VALIDATION_ERROR'
        }],
        warnings: []
      };
    }
  }

  /**
   * Get persona statistics
   */
  async getPersonaStatistics(request: PersonaStatisticsRequest): Promise<PersonaStatisticsResponse> {
    const startTime = Date.now();

    try {
      const personas = await this.loadAllPersonas();
      
      // Calculate statistics
      const statistics = {
        totalPersonas: personas.length,
        roleDistribution: this.calculateRoleDistribution(),
        specializationDistribution: this.calculateSpecializationDistribution(),
        generationDistribution: this.calculateGenerationDistribution(personas),
        averageFitness: this.calculateAverageFitness(personas),
        topPerformers: request.includeTopPerformers ? this.getTopPerformers(personas) : [],
        evolutionTrends: request.includeEvolutionTrends ? this.calculateEvolutionTrends(personas) : []
      };

      this.recordResponseTime(Date.now() - startTime);

      return {
        success: true,
        statistics
      };

    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Get persona manager status
   */
  getPersonaManagerStatus(): PersonaManagerStatus {
    const totalOps = this.operationHistory.length;
    const successfulOps = this.operationHistory.filter(op => op.success).length;
    const failedOps = totalOps - successfulOps;

    return {
      totalOperations: totalOps,
      successfulOperations: successfulOps,
      failedOperations: failedOps,
      recentOperations: this.operationHistory.slice(-10),
      averageResponseTime: this.calculateAverageResponseTime(),
      systemHealth: this.calculateSystemHealth()
    };
  }

  // ==================== PRIVATE HELPER METHODS ====================

  /**
   * Ensure storage directory exists
   */
  private async ensureStorageDirectory(): Promise<void> {
    try {
      await fs.mkdir(this.personaStoragePath, { recursive: true });
    } catch (error) {
      this.logMessage(`Failed to create storage directory: ${error}`);
    }
  }

  /**
   * Save persona to file system
   */
  private async savePersona(persona: PersonaGenome): Promise<void> {
    const filePath = join(this.personaStoragePath, `${persona.id}.json`);
    await fs.writeFile(filePath, JSON.stringify(persona, null, 2));
    this.updatePersona(persona);
  }

  /**
   * Load persona from file system
   */
  private async loadPersona(id: string): Promise<PersonaGenome | null> {
    try {
      const filePath = join(this.personaStoragePath, `${id}.json`);
      const content = await fs.readFile(filePath, 'utf-8');
      return JSON.parse(content);
    } catch (error) {
      return null;
    }
  }

  /**
   * Load all personas from storage
   */
  private async loadAllPersonas(): Promise<PersonaGenome[]> {
    try {
      const files = await fs.readdir(this.personaStoragePath);
      const personas: PersonaGenome[] = [];

      for (const file of files) {
        if (file.endsWith('.json')) {
          const id = file.replace('.json', '');
          const persona = await this.loadPersona(id);
          if (persona) {
            personas.push(persona);
          }
        }
      }

      return personas;
    } catch (error) {
      this.logMessage(`Failed to load personas: ${error}`);
      return [];
    }
  }

  /**
   * Apply custom traits to persona
   */
  private applyCustomTraits(persona: PersonaGenome, traits: any): void {
    if (traits.personality) {
      Object.assign(persona.identity.personality, traits.personality);
    }
    
    if (traits.knowledge) {
      // Apply knowledge traits
      if (traits.knowledge.domainExpertise) {
        persona.knowledge.competencies[persona.knowledge.domain] = traits.knowledge.domainExpertise;
      }
    }
    
    if (traits.behavior) {
      // Apply behavior traits
      if (traits.behavior.adaptability) {
        persona.behavior.adaptationRate = traits.behavior.adaptability;
      }
      if (traits.behavior.collaboration) {
        persona.behavior.collaborationPreference = traits.behavior.collaboration;
      }
    }
  }

  /**
   * Calculate child generation from parents
   */
  private async calculateChildGeneration(parentIds: string[]): Promise<number> {
    let maxGeneration = 0;
    
    for (const parentId of parentIds) {
      const parent = await this.loadPersona(parentId);
      if (parent) {
        maxGeneration = Math.max(maxGeneration, parent.evolution.generation);
      }
    }
    
    return maxGeneration + 1;
  }

  /**
   * Create offspring from two parents
   */
  private async createOffspringFromParents(
    parent1: PersonaGenome, 
    parent2: PersonaGenome, 
    request: PersonaCrossoverRequest
  ): Promise<PersonaGenome> {
    const childName = request.childName || `${parent1.identity.name}_${parent2.identity.name}_child`;
    
    // Use existing simpleOffspring logic for now
    // TODO: Integrate with GenomeProcessor for proper crossover
    const offspring = this.simpleOffspring(parent1, parent2);
    offspring.identity.name = childName;
    
    return offspring;
  }

  /**
   * Analyze inherited traits from crossover
   */
  private analyzeInheritedTraits(parent1: PersonaGenome, parent2: PersonaGenome, offspring: PersonaGenome): Record<string, string> {
    const traits: Record<string, string> = {};
    
    // Analyze specialization inheritance
    if (offspring.identity.specialization === parent1.identity.specialization) {
      traits.specialization = 'parent1';
    } else if (offspring.identity.specialization === parent2.identity.specialization) {
      traits.specialization = 'parent2';
    }
    
    // Analyze personality trait inheritance
    const p1_creativity = parent1.identity.personality.creativity;
    const p2_creativity = parent2.identity.personality.creativity;
    const child_creativity = offspring.identity.personality.creativity;
    
    if (Math.abs(child_creativity - p1_creativity) < Math.abs(child_creativity - p2_creativity)) {
      traits.creativity = 'parent1';
    } else {
      traits.creativity = 'parent2';
    }
    
    return traits;
  }

  /**
   * Calculate compatibility score between two personas
   */
  private calculateCompatibilityScore(parent1: PersonaGenome, parent2: PersonaGenome): number {
    let score = 0;
    
    // Specialization compatibility
    if (parent1.identity.specialization === parent2.identity.specialization) {
      score += 0.3;
    } else {
      score += 0.1; // Different specializations can still be compatible
    }
    
    // Personality compatibility
    const p1_traits = parent1.identity.personality;
    const p2_traits = parent2.identity.personality;
    
    const personalityDifference = Math.abs(p1_traits.creativity - p2_traits.creativity) +
                                  Math.abs(p1_traits.analytical - p2_traits.analytical) +
                                  Math.abs(p1_traits.collaboration - p2_traits.collaboration);
    
    score += Math.max(0, 0.5 - personalityDifference / 3);
    
    // Generation compatibility
    const generationDiff = Math.abs(parent1.evolution.generation - parent2.evolution.generation);
    score += Math.max(0, 0.2 - generationDiff * 0.05);
    
    return Math.min(1, score);
  }

  /**
   * Apply filters to persona list
   */
  private applyFilters(personas: PersonaGenome[], filters: any): PersonaGenome[] {
    return personas.filter(persona => {
      if (filters.role && persona.identity.role !== filters.role) return false;
      if (filters.specialization && persona.identity.specialization !== filters.specialization) return false;
      if (filters.generation !== undefined && persona.evolution.generation !== filters.generation) return false;
      if (filters.minFitness !== undefined && persona.evolution.fitnessScore < filters.minFitness) return false;
      if (filters.maxFitness !== undefined && persona.evolution.fitnessScore > filters.maxFitness) return false;
      if (filters.hasAncestors !== undefined && (persona.lineage.ancestors.length > 0) !== filters.hasAncestors) return false;
      if (filters.hasDescendants !== undefined && (persona.lineage.descendants.length > 0) !== filters.hasDescendants) return false;
      
      return true;
    });
  }

  /**
   * Sort personas by specified field
   */
  private sortPersonas(personas: PersonaGenome[], sortBy: string, sortOrder: 'asc' | 'desc' = 'asc'): PersonaGenome[] {
    return personas.sort((a, b) => {
      let aValue, bValue;
      
      switch (sortBy) {
        case 'name':
          aValue = a.identity.name;
          bValue = b.identity.name;
          break;
        case 'fitness':
          aValue = a.evolution.fitnessScore;
          bValue = b.evolution.fitnessScore;
          break;
        case 'generation':
          aValue = a.evolution.generation;
          bValue = b.evolution.generation;
          break;
        case 'specialization':
          aValue = a.identity.specialization;
          bValue = b.identity.specialization;
          break;
        default:
          return 0;
      }
      
      if (aValue < bValue) return sortOrder === 'asc' ? -1 : 1;
      if (aValue > bValue) return sortOrder === 'asc' ? 1 : -1;
      return 0;
    });
  }

  /**
   * Calculate generation distribution
   */
  private calculateGenerationDistribution(personas: PersonaGenome[]): Record<number, number> {
    const distribution: Record<number, number> = {};
    
    for (const persona of personas) {
      const gen = persona.evolution.generation;
      distribution[gen] = (distribution[gen] || 0) + 1;
    }
    
    return distribution;
  }

  /**
   * Calculate average fitness
   */
  private calculateAverageFitness(personas: PersonaGenome[]): number {
    if (personas.length === 0) return 0;
    
    const totalFitness = personas.reduce((sum, p) => sum + p.evolution.fitnessScore, 0);
    return totalFitness / personas.length;
  }

  /**
   * Get top performing personas
   */
  private getTopPerformers(personas: PersonaGenome[], limit: number = 10): PersonaGenome[] {
    return personas
      .sort((a, b) => b.evolution.fitnessScore - a.evolution.fitnessScore)
      .slice(0, limit);
  }

  /**
   * Calculate evolution trends
   */
  private calculateEvolutionTrends(personas: PersonaGenome[]): any[] {
    const trends: any[] = [];
    const generationMap = new Map<number, PersonaGenome[]>();
    
    // Group by generation
    for (const persona of personas) {
      const gen = persona.evolution.generation;
      if (!generationMap.has(gen)) {
        generationMap.set(gen, []);
      }
      generationMap.get(gen)!.push(persona);
    }
    
    // Calculate trends per generation
    for (const [generation, generationPersonas] of generationMap) {
      const avgFitness = this.calculateAverageFitness(generationPersonas);
      const specializations = new Set(generationPersonas.map(p => p.identity.specialization));
      const diversity = specializations.size / generationPersonas.length;
      
      trends.push({
        generation,
        averageFitness: avgFitness,
        diversity,
        innovations: 0, // TODO: Calculate innovations
        extinctions: 0  // TODO: Calculate extinctions
      });
    }
    
    return trends.sort((a, b) => a.generation - b.generation);
  }

  /**
   * Validate persona fields
   */
  private validatePersonaFields(persona: PersonaGenome, errors: any[], warnings: any[]): void {
    // Validate personality traits are in valid range
    const personality = persona.identity.personality;
    for (const [trait, value] of Object.entries(personality)) {
      if (value < 0 || value > 1) {
        errors.push({
          field: `personality.${trait}`,
          message: `Personality trait ${trait} must be between 0 and 1`,
          severity: 'error',
          code: 'INVALID_RANGE'
        });
      }
    }
    
    // Validate fitness score
    if (persona.evolution.fitnessScore < 0 || persona.evolution.fitnessScore > 1) {
      errors.push({
        field: 'evolution.fitnessScore',
        message: 'Fitness score must be between 0 and 1',
        severity: 'error',
        code: 'INVALID_RANGE'
      });
    }
    
    // Validate generation
    if (persona.evolution.generation < 0) {
      errors.push({
        field: 'evolution.generation',
        message: 'Generation must be non-negative',
        severity: 'error',
        code: 'INVALID_VALUE'
      });
    }
  }

  /**
   * Validate persona constraints
   */
  private validatePersonaConstraints(persona: PersonaGenome, errors: any[], warnings: any[]): void {
    // Check for required fields
    if (!persona.identity.name.trim()) {
      errors.push({
        field: 'identity.name',
        message: 'Persona name cannot be empty',
        severity: 'error',
        code: 'REQUIRED_FIELD'
      });
    }
    
    // Check mutation rate
    if (persona.reproduction.mutationRate > 0.5) {
      warnings.push({
        field: 'reproduction.mutationRate',
        message: 'High mutation rate may cause instability',
        suggestion: 'Consider reducing mutation rate below 0.5',
        code: 'HIGH_MUTATION_RATE'
      });
    }
  }

  /**
   * Record operation in history
   */
  private recordOperation(operation: PersonaManagerOperation): void {
    this.operationHistory.push(operation);
    
    // Keep only last 1000 operations
    if (this.operationHistory.length > 1000) {
      this.operationHistory = this.operationHistory.slice(-1000);
    }
  }

  /**
   * Record response time
   */
  private recordResponseTime(timeMs: number): void {
    this.responseTimesMs.push(timeMs);
    
    // Keep only last 100 response times
    if (this.responseTimesMs.length > 100) {
      this.responseTimesMs = this.responseTimesMs.slice(-100);
    }
  }

  /**
   * Calculate average response time
   */
  private calculateAverageResponseTime(): number {
    if (this.responseTimesMs.length === 0) return 0;
    
    const total = this.responseTimesMs.reduce((sum, time) => sum + time, 0);
    return total / this.responseTimesMs.length;
  }

  /**
   * Calculate system health
   */
  private calculateSystemHealth(): 'healthy' | 'degraded' | 'unhealthy' {
    const recentOps = this.operationHistory.slice(-20);
    if (recentOps.length === 0) return 'healthy';
    
    const successRate = recentOps.filter(op => op.success).length / recentOps.length;
    const avgResponseTime = this.calculateAverageResponseTime();
    
    if (successRate < 0.8 || avgResponseTime > 5000) {
      return 'unhealthy';
    } else if (successRate < 0.95 || avgResponseTime > 2000) {
      return 'degraded';
    } else {
      return 'healthy';
    }
  }
}

// ==================== REQUEST/RESPONSE UNION TYPES ====================

export type PersonaManagerRequest = 
  | { type: 'create'; data: CreatePersonaRequest }
  | { type: 'search'; data: PersonaSearchRequest }
  | { type: 'update'; data: PersonaUpdateRequest }
  | { type: 'crossover'; data: PersonaCrossoverRequest }
  | { type: 'validate'; data: PersonaValidationRequest }
  | { type: 'statistics'; data: PersonaStatisticsRequest }
  | { type: 'status'; data: {} };

export type PersonaManagerResponse = 
  | { type: 'create'; data: CreatePersonaResponse }
  | { type: 'search'; data: PersonaSearchResponse }
  | { type: 'update'; data: PersonaUpdateResponse }
  | { type: 'crossover'; data: PersonaCrossoverResponse }
  | { type: 'validate'; data: PersonaValidationResponse }
  | { type: 'statistics'; data: PersonaStatisticsResponse }
  | { type: 'status'; data: PersonaManagerStatus };