/**
 * Genome Processor - Crossover, Mutation, and Evaluation for Persona Genomes
 * 
 * This implements the genetic algorithm operations for persona evolution:
 * - Crossover: Combining two parent genomes to create offspring
 * - Mutation: Introducing random variations for exploration
 * - Evaluation: Assessing genome fitness through challenges
 * 
 * Based on Aria's synthesis of genetic algorithms with LoRA-based AI personas.
 */

import { 
  PersonaGenome, 
  Challenge, 
  PersonaIdentity,
  PersonaKnowledge,
  PersonaBehavior,
  PersonaSubstrate,
  PersonaLineage,
  MutationEvent,
  EvolutionaryPressure,
  generateUUID,
  PersonalityTraits
} from "./shared/AcademyTypes";

/**
 * GenomeProcessor handles all genetic operations on persona genomes
 */
export class GenomeProcessor {
  
  /**
   * Crossover: Merge two parent genomes to create offspring
   * This is where genetic diversity and innovation come from
   */
  crossover(parent1: PersonaGenome, parent2: PersonaGenome): PersonaGenome {
    console.log(`🧬 Crossover: ${parent1.identity.name} + ${parent2.identity.name}`);
    
    const childId = generateUUID();
    
    // Create child identity by blending parents
    const childIdentity = this.crossoverIdentity(parent1.identity, parent2.identity);
    
    // Merge knowledge from both parents
    const childKnowledge = this.crossoverKnowledge(parent1.knowledge, parent2.knowledge);
    
    // Blend behavioral traits
    const childBehavior = this.crossoverBehavior(parent1.behavior, parent2.behavior);
    
    // Combine substrate capabilities
    const childSubstrate = this.crossoverSubstrate(parent1.substrate, parent2.substrate);
    
    // Create new lineage
    const childLineage = this.createChildLineage(parent1, parent2);
    
    // Set up reproduction parameters
    const childReproduction = {
      mutationRate: (parent1.reproduction.mutationRate + parent2.reproduction.mutationRate) / 2,
      crossoverWeights: this.mergeWeights(parent1.reproduction.crossoverWeights, parent2.reproduction.crossoverWeights),
      reproductionEligibility: true,
      breedingSuccess: 0,
      offspringCount: 0,
      compatibilityMatrix: {}
    };
    
    // Initialize evolution parameters
    const childEvolution = {
      generation: Math.max(parent1.evolution.generation, parent2.evolution.generation) + 1,
      parentGenomes: [parent1.id, parent2.id],
      mutationHistory: [],
      evolutionStage: "spawning" as const,
      fitnessScore: (parent1.evolution.fitnessScore + parent2.evolution.fitnessScore) / 2,
      adaptationSuccess: 0,
      survivalRounds: 0,
      evolutionPressure: [...new Set([...parent1.evolution.evolutionPressure, ...parent2.evolution.evolutionPressure])]
    };
    
    const child: PersonaGenome = {
      id: childId,
      identity: childIdentity,
      knowledge: childKnowledge,
      behavior: childBehavior,
      evolution: childEvolution,
      substrate: childSubstrate,
      reproduction: childReproduction,
      lineage: childLineage
    };
    
    console.log(`👶 Child created: ${child.identity.name} (Gen ${child.evolution.generation})`);
    
    return child;
  }
  
  /**
   * Mutation: Introduce random variations to explore new possibilities
   */
  mutate(genome: PersonaGenome): PersonaGenome {
    const mutated = JSON.parse(JSON.stringify(genome)); // Deep copy
    const mutations: string[] = [];
    
    // Mutate personality traits
    if (Math.random() < genome.reproduction.mutationRate) {
      const trait = this.randomChoice(['creativity', 'analytical', 'helpfulness', 'competitiveness', 'patience', 'innovation']);
      const delta = (Math.random() - 0.5) * 0.2; // ±10% change
      mutated.identity.personality[trait] = Math.max(0, Math.min(1, mutated.identity.personality[trait] + delta));
      mutations.push(`personality.${trait}`);
    }
    
    // Mutate behavior patterns
    if (Math.random() < genome.reproduction.mutationRate) {
      const newAdaptationRate = genome.behavior.adaptationRate || 0.5;
      const delta = (Math.random() - 0.5) * 0.3; // ±15% change
      mutated.behavior.adaptationRate = Math.max(0.1, Math.min(1, newAdaptationRate + delta));
      mutations.push('behavior.adaptationRate');
    }
    
    // Mutate specialization
    if (Math.random() < genome.reproduction.mutationRate * 0.5) { // Lower chance for major changes
      const specializations = ['typescript', 'testing', 'architecture', 'ui_design', 'debugging', 'optimization'];
      const currentIndex = specializations.indexOf(genome.identity.specialization);
      const newIndex = (currentIndex + 1 + Math.floor(Math.random() * (specializations.length - 1))) % specializations.length;
      mutated.identity.specialization = specializations[newIndex];
      mutations.push('identity.specialization');
    }
    
    // Mutate LoRA adapters (add/remove)
    if (Math.random() < genome.reproduction.mutationRate) {
      const availableLoRAs = ['typescript-expert', 'testing-master', 'architecture-guru', 'ui-designer', 'debugger-pro'];
      
      if (Math.random() < 0.7) {
        // Add new LoRA
        const newLoRA = this.randomChoice(availableLoRAs.filter(l => !mutated.substrate.loraIds.includes(l)));
        if (newLoRA) {
          mutated.substrate.loraIds.push(newLoRA);
          mutations.push(`substrate.loraIds.add(${newLoRA})`);
        }
      } else {
        // Remove existing LoRA
        if (mutated.substrate.loraIds.length > 1) {
          const indexToRemove = Math.floor(Math.random() * mutated.substrate.loraIds.length);
          const removedLoRA = mutated.substrate.loraIds.splice(indexToRemove, 1)[0];
          mutations.push(`substrate.loraIds.remove(${removedLoRA})`);
        }
      }
    }
    
    // Mutate vector position slightly
    if (Math.random() < genome.reproduction.mutationRate) {
      for (let i = 0; i < mutated.substrate.vectorPosition.length; i++) {
        if (Math.random() < 0.1) { // 10% chance to mutate each dimension
          const delta = (Math.random() - 0.5) * 0.1; // ±5% change
          mutated.substrate.vectorPosition[i] = Math.max(0, Math.min(1, mutated.substrate.vectorPosition[i] + delta));
        }
      }
      mutations.push('substrate.vectorPosition');
    }
    
    // Record mutation event
    if (mutations.length > 0) {
      const mutationEvent: MutationEvent = {
        timestamp: new Date(),
        type: "spontaneous",
        changes: mutations.reduce((acc, m) => ({ ...acc, [m]: true }), {}),
        trigger: "genetic_algorithm",
        outcome: "beneficial" // Default assumption, will be evaluated later
      };
      
      mutated.evolution.mutationHistory.push(mutationEvent);
      
      console.log(`🧬 Mutation applied to ${mutated.identity.name}: ${mutations.join(', ')}`);
    }
    
    return mutated;
  }
  
  /**
   * Evaluate genome fitness through challenges
   */
  async evaluate(genome: PersonaGenome, challenges: Challenge[]): Promise<number> {
    let totalScore = 0;
    const evaluationResults = [];
    
    for (const challenge of challenges) {
      const score = await this.evaluateChallenge(genome, challenge);
      evaluationResults.push(score);
      totalScore += score;
    }
    
    const averageScore = totalScore / challenges.length;
    
    // Update genome fitness
    genome.evolution.fitnessScore = averageScore;
    
    console.log(`📊 Evaluated ${genome.identity.name}: ${averageScore.toFixed(2)} average fitness`);
    
    return averageScore;
  }
  
  /**
   * Select genomes for reproduction based on evolutionary pressure
   */
  selectForReproduction(genomes: PersonaGenome[], pressure: EvolutionaryPressure): PersonaGenome[] {
    // Sort by fitness score
    const sortedGenomes = [...genomes].sort((a, b) => b.evolution.fitnessScore - a.evolution.fitnessScore);
    
    // Select top performers based on survival rate
    const survivalCount = Math.floor(genomes.length * pressure.survivalRate);
    const survivors = sortedGenomes.slice(0, survivalCount);
    
    console.log(`🏆 Selected ${survivors.length} genomes for reproduction (${Math.round(pressure.survivalRate * 100)}% survival rate)`);
    
    return survivors;
  }
  
  // ==================== PRIVATE HELPER METHODS ====================
  
  /**
   * Crossover identity traits
   */
  private crossoverIdentity(parent1: PersonaIdentity, parent2: PersonaIdentity): PersonaIdentity {
    // Create child name by combining parent names
    const childName = this.createChildName(parent1.name, parent2.name);
    
    // Blend personality traits
    const childPersonality: PersonalityTraits = {
      creativity: (parent1.personality.creativity + parent2.personality.creativity) / 2,
      analytical: (parent1.personality.analytical + parent2.personality.analytical) / 2,
      helpfulness: (parent1.personality.helpfulness + parent2.personality.helpfulness) / 2,
      competitiveness: (parent1.personality.competitiveness + parent2.personality.competitiveness) / 2,
      patience: (parent1.personality.patience + parent2.personality.patience) / 2,
      innovation: (parent1.personality.innovation + parent2.personality.innovation) / 2
    };
    
    // Inherit specialization from fitter parent
    const specialization = Math.random() < 0.5 ? parent1.specialization : parent2.specialization;
    
    // Merge goals
    const childGoals = [...new Set([...parent1.goals, ...parent2.goals])];
    
    return {
      name: childName,
      role: "student", // Children start as students
      generation: Math.max(parent1.generation, parent2.generation) + 1,
      parentIds: [parent1.name, parent2.name],
      specialization,
      personality: childPersonality,
      goals: childGoals
    };
  }
  
  /**
   * Crossover knowledge systems
   */
  private crossoverKnowledge(parent1: PersonaKnowledge, parent2: PersonaKnowledge): PersonaKnowledge {
    // Merge expertise from both parents
    const childExpertise = [...new Set([...parent1.expertise, ...parent2.expertise])];
    
    // Blend competencies
    const childCompetencies: Record<string, number> = {};
    const allSkills = new Set([...Object.keys(parent1.competencies), ...Object.keys(parent2.competencies)]);
    
    for (const skill of allSkills) {
      const p1Score = parent1.competencies[skill] || 0;
      const p2Score = parent2.competencies[skill] || 0;
      childCompetencies[skill] = (p1Score + p2Score) / 2;
    }
    
    // Choose primary domain
    const childDomain = Math.random() < 0.5 ? parent1.domain : parent2.domain;
    
    return {
      domain: childDomain,
      expertise: childExpertise,
      competencies: childCompetencies,
      experiencePoints: Math.floor((parent1.experiencePoints + parent2.experiencePoints) / 2),
      knowledgeGraph: this.mergeKnowledgeGraphs(parent1.knowledgeGraph, parent2.knowledgeGraph)
    };
  }
  
  /**
   * Crossover behavior patterns
   */
  private crossoverBehavior(parent1: PersonaBehavior, parent2: PersonaBehavior): PersonaBehavior {
    return {
      learningStyle: Math.random() < 0.5 ? parent1.learningStyle : parent2.learningStyle,
      teachingStyle: Math.random() < 0.5 ? parent1.teachingStyle : parent2.teachingStyle,
      adaptationRate: (parent1.adaptationRate || 0.5 + parent2.adaptationRate || 0.5) / 2,
      communicationStyle: Math.random() < 0.5 ? parent1.communicationStyle : parent2.communicationStyle,
      decisionMakingStyle: Math.random() < 0.5 ? parent1.decisionMakingStyle : parent2.decisionMakingStyle,
      riskTolerance: (parent1.riskTolerance + parent2.riskTolerance) / 2,
      collaborationPreference: (parent1.collaborationPreference + parent2.collaborationPreference) / 2
    };
  }
  
  /**
   * Crossover substrate capabilities
   */
  private crossoverSubstrate(parent1: PersonaSubstrate, parent2: PersonaSubstrate): PersonaSubstrate {
    // Merge LoRA adapters
    const childLoraIds = [...new Set([...parent1.loraIds, ...parent2.loraIds])];
    
    // Blend vector positions
    const childVectorPosition = parent1.vectorPosition.map((val, i) => 
      (val + parent2.vectorPosition[i]) / 2
    );
    
    return {
      loraIds: childLoraIds,
      memoryPatterns: [...new Set([...parent1.memoryPatterns, ...parent2.memoryPatterns])],
      processingStyle: Math.random() < 0.5 ? parent1.processingStyle : parent2.processingStyle,
      adaptationMechanisms: [...new Set([...parent1.adaptationMechanisms, ...parent2.adaptationMechanisms])],
      sentinelTraits: { ...parent1.sentinelTraits, ...parent2.sentinelTraits },
      vectorPosition: childVectorPosition
    };
  }
  
  /**
   * Create child lineage
   */
  private createChildLineage(parent1: PersonaGenome, parent2: PersonaGenome): PersonaLineage {
    const childAncestors = [...new Set([
      ...parent1.lineage.ancestors,
      ...parent2.lineage.ancestors,
      parent1.id,
      parent2.id
    ])];
    
    return {
      ancestors: childAncestors,
      descendants: [],
      siblings: [],
      generation: Math.max(parent1.evolution.generation, parent2.evolution.generation) + 1,
      lineageStrength: (parent1.lineage.lineageStrength + parent2.lineage.lineageStrength) / 2,
      emergentTraits: [...new Set([...parent1.lineage.emergentTraits, ...parent2.lineage.emergentTraits])]
    };
  }
  
  /**
   * Create child name from parent names
   */
  private createChildName(parent1Name: string, parent2Name: string): string {
    // Extract base names (remove suffixes like "Alpha", "Beta", etc.)
    const baseName1 = parent1Name.split(/[_\-]/)[0];
    const baseName2 = parent2Name.split(/[_\-]/)[0];
    
    // Create hybrid name
    const prefixes = [baseName1, baseName2];
    const suffixes = ['Neo', 'Prime', 'Hybrid', 'Evolved', 'Gen2', 'Fusion'];
    
    const prefix = this.randomChoice(prefixes);
    const suffix = this.randomChoice(suffixes);
    
    return `${prefix}${suffix}_${generateUUID().slice(0, 8)}`;
  }
  
  /**
   * Evaluate genome against single challenge
   */
  private async evaluateChallenge(genome: PersonaGenome, challenge: Challenge): Promise<number> {
    // Simulate challenge evaluation based on genome capabilities
    
    // Base score from specialization match
    let score = genome.identity.specialization === challenge.domain ? 0.8 : 0.4;
    
    // Adjust based on personality traits
    score *= 1 + (genome.identity.personality.innovation - 0.5) * 0.2;
    score *= 1 + (genome.identity.personality.analytical - 0.5) * 0.2;
    
    // Adjust based on relevant expertise
    const relevantExpertise = genome.knowledge.expertise.filter(e => 
      challenge.prompt.toLowerCase().includes(e.toLowerCase())
    ).length;
    score *= 1 + relevantExpertise * 0.1;
    
    // Adjust based on adaptation rate
    score *= 1 + (genome.behavior.adaptationRate || 0.5) * 0.3;
    
    // Add some randomness for unpredictability
    score *= 0.8 + Math.random() * 0.4;
    
    return Math.max(0, Math.min(1, score));
  }
  
  /**
   * Merge crossover weights
   */
  private mergeWeights(weights1?: Record<string, number>, weights2?: Record<string, number>): Record<string, number> {
    const merged: Record<string, number> = {};
    const allKeys = new Set([...Object.keys(weights1 || {}), ...Object.keys(weights2 || {})]);
    
    for (const key of allKeys) {
      const w1 = weights1?.[key] || 0;
      const w2 = weights2?.[key] || 0;
      merged[key] = (w1 + w2) / 2;
    }
    
    return merged;
  }
  
  /**
   * Merge knowledge graphs
   */
  private mergeKnowledgeGraphs(graph1?: Record<string, string[]>, graph2?: Record<string, string[]>): Record<string, string[]> {
    const merged: Record<string, string[]> = {};
    const allConcepts = new Set([...Object.keys(graph1 || {}), ...Object.keys(graph2 || {})]);
    
    for (const concept of allConcepts) {
      const connections1 = graph1?.[concept] || [];
      const connections2 = graph2?.[concept] || [];
      merged[concept] = [...new Set([...connections1, ...connections2])];
    }
    
    return merged;
  }
  
  /**
   * Get random choice from array
   */
  private randomChoice<T>(array: T[]): T {
    return array[Math.floor(Math.random() * array.length)];
  }
}