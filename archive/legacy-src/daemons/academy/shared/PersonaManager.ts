/**
 * PersonaManager - Manages persona lifecycle and operations
 * 
 * Handles persona creation, spawning, conversion, and basic lifecycle management.
 * Follows middle-out architecture pattern with clean separation of concerns.
 */

import { PersonaGenome, generateUUID } from './AcademyTypes';

/**
 * PersonaManager handles all persona lifecycle operations
 */
export class PersonaManager {
  private personas: Map<string, PersonaGenome> = new Map();
  
  /**
   * Create initial population for evolution
   */
  async createInitialPopulation(size: number): Promise<PersonaGenome[]> {
    const personas: PersonaGenome[] = [];
    const specializations = ['typescript', 'testing', 'architecture', 'ui_design', 'debugging', 'optimization'];
    const names = ['Alpha', 'Beta', 'Gamma', 'Delta', 'Epsilon', 'Zeta', 'Eta', 'Theta', 'Iota', 'Kappa'];
    
    for (let i = 0; i < size; i++) {
      const specialization = specializations[i % specializations.length];
      const name = `${specialization}${names[i % names.length]}`;
      
      const persona = this.createPersonaGenome(name, specialization);
      personas.push(persona);
      this.personas.set(persona.id, persona);
    }
    
    return personas;
  }
  
  /**
   * Create a new PersonaGenome with default settings
   */
  createPersonaGenome(name: string, specialization: string): PersonaGenome {
    const personaId = generateUUID();
    
    return {
      id: personaId,
      identity: {
        name: name,
        role: 'student',
        generation: 0,
        specialization: specialization,
        personality: {
          creativity: 0.3 + Math.random() * 0.4,
          analytical: 0.3 + Math.random() * 0.4,
          helpfulness: 0.3 + Math.random() * 0.4,
          competitiveness: 0.3 + Math.random() * 0.4,
          patience: 0.3 + Math.random() * 0.4,
          innovation: 0.3 + Math.random() * 0.4
        },
        goals: [`master_${specialization}`, 'collaborate_effectively', 'teach_others']
      },
      knowledge: {
        domain: specialization,
        expertise: [specialization, 'problem_solving'],
        competencies: {
          [specialization]: 0.4 + Math.random() * 0.3,
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
        loraIds: [`${specialization}_lora`, 'base_reasoning_lora'],
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
   * Convert spawn result to PersonaGenome format
   */
  convertSpawnResultToGenome(spawnData: any, specialization: string): PersonaGenome {
    const genome = this.createPersonaGenome(spawnData.persona_name, specialization);
    
    // Override with spawn data
    genome.id = spawnData.persona_id;
    genome.substrate.loraIds = spawnData.lora_stack?.layers?.map((l: any) => l.name) || [];
    
    return genome;
  }
  
  /**
   * Get persona by ID
   */
  getPersona(id: string): PersonaGenome | undefined {
    return this.personas.get(id);
  }
  
  /**
   * Get all personas
   */
  getAllPersonas(): PersonaGenome[] {
    return Array.from(this.personas.values());
  }
  
  /**
   * Update persona in collection
   */
  updatePersona(persona: PersonaGenome): void {
    this.personas.set(persona.id, persona);
  }
  
  /**
   * Update ecosystem with new personas
   */
  updatePersonaCollection(personas: PersonaGenome[]): void {
    this.personas.clear();
    for (const persona of personas) {
      this.personas.set(persona.id, persona);
    }
  }
  
  /**
   * Get persona count
   */
  getPersonaCount(): number {
    return this.personas.size;
  }
  
  /**
   * Calculate role distribution
   */
  calculateRoleDistribution(): Record<string, number> {
    const distribution: Record<string, number> = {};
    for (const persona of this.personas.values()) {
      distribution[persona.identity.role] = (distribution[persona.identity.role] || 0) + 1;
    }
    return distribution;
  }
  
  /**
   * Calculate specialization distribution
   */
  calculateSpecializationDistribution(): Record<string, number> {
    const distribution: Record<string, number> = {};
    for (const persona of this.personas.values()) {
      const spec = persona.identity.specialization;
      distribution[spec] = (distribution[spec] || 0) + 1;
    }
    return distribution;
  }
  
  /**
   * Get persona lineage details
   */
  getPersonaLineage(personaId: string): any {
    const persona = this.personas.get(personaId);
    if (!persona) return null;
    
    return {
      persona_id: personaId,
      lineage: persona.lineage,
      evolution: persona.evolution,
      ancestors: this.getAncestorDetails(persona),
      descendants: this.getDescendantDetails(persona)
    };
  }
  
  /**
   * Build complete lineage tree
   */
  buildLineageTree(): any[] {
    const personas = Array.from(this.personas.values());
    const tree = [];
    
    // Build family trees starting from generation 0
    const founders = personas.filter(p => p.evolution.generation === 0);
    
    for (const founder of founders) {
      tree.push({
        id: founder.id,
        name: founder.identity.name,
        generation: founder.evolution.generation,
        specialization: founder.identity.specialization,
        descendants: this.buildDescendantTree(founder, personas)
      });
    }
    
    return tree;
  }
  
  /**
   * Calculate lineage statistics
   */
  calculateLineageStatistics(): any {
    const personas = Array.from(this.personas.values());
    const generations = personas.map(p => p.evolution.generation);
    
    return {
      total_personas: personas.length,
      max_generation: Math.max(...generations),
      avg_generation: generations.reduce((a, b) => a + b, 0) / generations.length,
      lineages_with_descendants: personas.filter(p => p.lineage.descendants.length > 0).length
    };
  }
  
  /**
   * Calculate average generation
   */
  calculateAverageGeneration(): number {
    const personas = Array.from(this.personas.values());
    if (personas.length === 0) return 0;
    
    const totalGeneration = personas.reduce((sum, p) => sum + p.evolution.generation, 0);
    return totalGeneration / personas.length;
  }
  
  // Private helper methods
  
  private getAncestorDetails(persona: PersonaGenome): any[] {
    return persona.lineage.ancestors.map(ancestorId => {
      const ancestor = this.personas.get(ancestorId);
      return ancestor ? {
        id: ancestorId,
        name: ancestor.identity.name,
        generation: ancestor.evolution.generation,
        specialization: ancestor.identity.specialization
      } : { id: ancestorId, name: 'Unknown', generation: -1, specialization: 'unknown' };
    });
  }
  
  private getDescendantDetails(persona: PersonaGenome): any[] {
    return persona.lineage.descendants.map(descendantId => {
      const descendant = this.personas.get(descendantId);
      return descendant ? {
        id: descendantId,
        name: descendant.identity.name,
        generation: descendant.evolution.generation,
        specialization: descendant.identity.specialization
      } : { id: descendantId, name: 'Unknown', generation: -1, specialization: 'unknown' };
    });
  }
  
  private buildDescendantTree(ancestor: PersonaGenome, allPersonas: PersonaGenome[]): any[] {
    const descendants = allPersonas.filter(p => 
      p.lineage.ancestors.includes(ancestor.id)
    );
    
    return descendants.map(descendant => ({
      id: descendant.id,
      name: descendant.identity.name,
      generation: descendant.evolution.generation,
      specialization: descendant.identity.specialization,
      descendants: this.buildDescendantTree(descendant, allPersonas)
    }));
  }
}
