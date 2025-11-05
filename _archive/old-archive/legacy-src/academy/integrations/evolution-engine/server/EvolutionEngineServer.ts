/**
 * Evolution Engine Server - Core evolution logic implementation
 * 
 * This module handles the core evolution processing following the middle-out pattern.
 * It contains the 80-90% shared logic for running evolution cycles.
 */

import { PersonaGenome, EvolutionaryPressure, generateUUID, MutationEvent } from '../../../shared/AcademyTypes';
import { 
  EvolutionEngineConfig,
  EvolutionEngineRequest,
  EvolutionEngineResponse,
  GenerationResult,
  GenerationMetrics,
  EcosystemMetrics,
  EvolutionSession,
  // SessionOutcome,
  EcosystemHealth,
  Challenge,
  ChallengeResult,
  // MutationType,
  // VectorSpaceEvolution,
  DEFAULT_EVOLUTION_CONFIG,
  // DEFAULT_EVOLUTIONARY_PRESSURE,
  validateEvolutionRequest
} from '../shared/EvolutionEngineTypes';

/**
 * Evolution Engine Server - Core evolution processing
 */
export class EvolutionEngineServer {
  private config: EvolutionEngineConfig;
  private evolutionHistory: GenerationResult[] = [];
  private currentPopulation: PersonaGenome[] = [];
  private activeSessions: Map<string, EvolutionSession> = new Map();
  private challengeLibrary: Challenge[] = [];
  private ecosystemMetrics: EcosystemMetrics;

  constructor(config: EvolutionEngineConfig = DEFAULT_EVOLUTION_CONFIG) {
    this.config = config;
    this.ecosystemMetrics = this.initializeEcosystemMetrics();
    this.initializeChallengeLibrary();
  }

  /**
   * Run complete evolution process
   */
  async runEvolution(request: EvolutionEngineRequest): Promise<EvolutionEngineResponse> {
    const validation = validateEvolutionRequest(request);
    if (!validation.valid) {
      return {
        success: false,
        generations: 0,
        finalPopulation: [],
        evolutionHistory: [],
        ecosystemMetrics: this.ecosystemMetrics,
        error: validation.errors.join(', ')
      };
    }

    try {
      this.config = { ...this.config, ...request.config };
      this.currentPopulation = [...request.initialPopulation];
      this.evolutionHistory = [];

      // Run evolution for specified generations
      for (let generation = 1; generation <= this.config.generations; generation++) {
        const generationResult = await this.runGeneration(
          this.currentPopulation,
          request.evolutionaryPressure,
          generation
        );

        this.evolutionHistory.push(generationResult);
        this.currentPopulation = generationResult.population;

        // Update ecosystem metrics
        this.updateEcosystemMetrics(generationResult);

        // Check for convergence
        if (this.checkConvergence(generationResult)) {
          console.log(`🎯 Evolution converged at generation ${generation}`);
          break;
        }
      }

      return {
        success: true,
        generations: this.evolutionHistory.length,
        finalPopulation: this.currentPopulation,
        evolutionHistory: this.evolutionHistory,
        ecosystemMetrics: this.ecosystemMetrics
      };

    } catch (error) {
      return {
        success: false,
        generations: this.evolutionHistory.length,
        finalPopulation: this.currentPopulation,
        evolutionHistory: this.evolutionHistory,
        ecosystemMetrics: this.ecosystemMetrics,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Run a single generation of evolution
   */
  async runGeneration(
    population: PersonaGenome[],
    pressure: EvolutionaryPressure,
    generationNumber: number
  ): Promise<GenerationResult> {
    console.log(`⚡ Running Evolution Generation ${generationNumber}`);

    // 1. Create evolution session
    const sessionId = generateUUID();
    const session = await this.createEvolutionSession(sessionId, population, pressure);

    // 2. Run challenges for all personas
    const challengeResults = await this.runChallenges(session);

    // 3. Evaluate fitness based on challenge results
    const evaluatedPopulation = await this.evaluateFitness(population, challengeResults);

    // 4. Apply selection pressure
    const survivors = await this.applySelection(evaluatedPopulation, pressure);

    // 5. Generate new personas through mutation and crossover
    const newPersonas = await this.generateNewPersonas(survivors, pressure);

    // 6. Combine survivors and new personas
    const nextGeneration = [...survivors, ...newPersonas];

    // 7. Update persona generations
    const updatedPopulation = this.updatePersonaGenerations(nextGeneration, generationNumber);

    // 8. Calculate generation metrics
    const metrics = this.calculateGenerationMetrics(updatedPopulation, survivors, newPersonas);

    // 9. Clean up session
    await this.cleanupSession(sessionId);

    return {
      generation: generationNumber,
      population: updatedPopulation,
      survivors,
      newPersonas,
      metrics,
      timestamp: Date.now()
    };
  }

  /**
   * Create evolution session for a generation
   */
  private async createEvolutionSession(
    sessionId: string,
    population: PersonaGenome[],
    pressure: EvolutionaryPressure
  ): Promise<EvolutionSession> {
    const session: EvolutionSession = {
      id: sessionId,
      sessionType: 'population',
      participants: population,
      challenges: this.selectChallenges(population),
      results: [],
      startTime: new Date(),
      evolutionaryPressure: pressure,
      sessionOutcome: {
        survivors: [],
        graduates: [],
        mutations: [],
        newRoles: {},
        emergentBehaviors: [],
        ecosystem_health: {
          diversity: 0,
          innovation: 0,
          collaboration: 0,
          sustainability: 0,
          growth: 0
        }
      }
    };

    this.activeSessions.set(sessionId, session);
    return session;
  }

  /**
   * Run challenges for all personas in session
   */
  private async runChallenges(session: EvolutionSession): Promise<ChallengeResult[]> {
    const results: ChallengeResult[] = [];

    for (const persona of session.participants) {
      for (const challenge of session.challenges) {
        const result = await this.executeChallenge(persona, challenge);
        results.push(result);
      }
    }

    session.results = results;
    return results;
  }

  /**
   * Execute a single challenge for a persona
   */
  private async executeChallenge(
    persona: PersonaGenome,
    challenge: Challenge
  ): Promise<ChallengeResult> {
    // Simulate challenge execution
    console.log('🔄 Starting evolution generation at:', new Date().toISOString());
    
    // Base success probability based on persona's fitness and competencies
    const competencyMatch = persona.knowledge.competencies[challenge.domain] || 0.5;
    const fitnessBonus = persona.evolution.fitnessScore * 0.3;
    const successProbability = Math.min(0.95, competencyMatch + fitnessBonus);
    
    const success = Math.random() < successProbability;
    const accuracy = success ? 0.7 + Math.random() * 0.3 : 0.2 + Math.random() * 0.5;
    const timeUsed = challenge.timeLimit * (0.5 + Math.random() * 0.4);
    
    // Innovation and collaboration scores
    const innovationScore = persona.identity.personality.innovation * Math.random();
    const collaborationScore = persona.identity.personality.helpfulness * Math.random();
    
    // Detect emergent capabilities
    const emergentCapabilities: string[] = [];
    if (accuracy > 0.9 && innovationScore > 0.8) {
      emergentCapabilities.push('breakthrough_solution');
    }
    if (collaborationScore > 0.8) {
      emergentCapabilities.push('collaborative_insight');
    }

    return {
      challengeId: challenge.id,
      personaId: persona.id,
      success,
      accuracy,
      timeUsed,
      resourcesUsed: challenge.resources,
      innovationScore,
      collaborationScore,
      emergentCapabilities,
      behaviorDetected: ['problem_solving', 'adaptation']
    };
  }

  /**
   * Evaluate fitness based on challenge results
   */
  private async evaluateFitness(
    population: PersonaGenome[],
    challengeResults: ChallengeResult[]
  ): Promise<PersonaGenome[]> {
    const evaluatedPopulation: PersonaGenome[] = [];

    for (const persona of population) {
      const personaResults = challengeResults.filter(r => r.personaId === persona.id);
      
      // Calculate fitness from challenge results
      const averageAccuracy = personaResults.reduce((sum, r) => sum + r.accuracy, 0) / personaResults.length;
      const successRate = personaResults.filter(r => r.success).length / personaResults.length;
      const averageInnovation = personaResults.reduce((sum, r) => sum + r.innovationScore, 0) / personaResults.length;
      const averageCollaboration = personaResults.reduce((sum, r) => sum + r.collaborationScore, 0) / personaResults.length;
      
      // Weighted fitness calculation
      const newFitness = (
        averageAccuracy * 0.4 +
        successRate * 0.3 +
        averageInnovation * 0.2 +
        averageCollaboration * 0.1
      );

      // Update persona with new fitness
      const updatedPersona: PersonaGenome = {
        ...persona,
        evolution: {
          ...persona.evolution,
          fitnessScore: newFitness,
          adaptationSuccess: persona.evolution.adaptationSuccess + (newFitness > persona.evolution.fitnessScore ? 1 : 0)
        }
      };

      evaluatedPopulation.push(updatedPersona);
    }

    return evaluatedPopulation;
  }

  /**
   * Apply selection pressure to determine survivors
   */
  private async applySelection(
    population: PersonaGenome[],
    pressure: EvolutionaryPressure
  ): Promise<PersonaGenome[]> {
    // Sort by fitness (descending)
    const sortedPopulation = [...population].sort((a, b) => b.evolution.fitnessScore - a.evolution.fitnessScore);
    
    // Calculate number of survivors
    const survivorCount = Math.floor(population.length * pressure.survivalRate);
    
    // Elite preservation - always keep top performers
    const eliteCount = this.config.elitePreservation ? Math.max(1, Math.floor(survivorCount * 0.2)) : 0;
    const elite = sortedPopulation.slice(0, eliteCount);
    
    // Tournament selection for remaining survivors
    const remainingSlots = survivorCount - eliteCount;
    const tournamentWinners = this.tournamentSelection(
      sortedPopulation.slice(eliteCount),
      remainingSlots,
      pressure
    );

    return [...elite, ...tournamentWinners];
  }

  /**
   * Tournament selection
   */
  private tournamentSelection(
    population: PersonaGenome[],
    count: number,
    pressure: EvolutionaryPressure
  ): PersonaGenome[] {
    const winners: PersonaGenome[] = [];
    const tournamentSize = Math.max(2, Math.floor(population.length * 0.1));

    for (let i = 0; i < count; i++) {
      // Select random tournament participants
      const tournament = [];
      for (let j = 0; j < tournamentSize; j++) {
        const randomIndex = Math.floor(Math.random() * population.length);
        tournament.push(population[randomIndex]);
      }

      // Select winner based on fitness and selection criteria
      const winner = tournament.reduce((best, current) => {
        const bestScore = this.calculateSelectionScore(best, pressure);
        const currentScore = this.calculateSelectionScore(current, pressure);
        return currentScore > bestScore ? current : best;
      });

      winners.push(winner);
    }

    return winners;
  }

  /**
   * Calculate selection score based on criteria
   */
  private calculateSelectionScore(persona: PersonaGenome, pressure: EvolutionaryPressure): number {
    const criteria = pressure.selectionCriteria;
    
    return (
      persona.evolution.fitnessScore * criteria.performance +
      persona.identity.personality.innovation * criteria.innovation +
      persona.evolution.adaptationSuccess * criteria.adaptation +
      persona.identity.personality.helpfulness * criteria.collaboration +
      (persona.identity.role === 'teacher' ? 1 : 0) * criteria.teaching
    );
  }

  /**
   * Generate new personas through mutation and crossover
   */
  private async generateNewPersonas(
    survivors: PersonaGenome[],
    pressure: EvolutionaryPressure
  ): Promise<PersonaGenome[]> {
    console.log('🧬 Generating new personas under pressure:', (pressure as any).type || 'unknown');
    const newPersonas: PersonaGenome[] = [];
    const targetCount = this.config.populationSize - survivors.length;

    for (let i = 0; i < targetCount; i++) {
      if (Math.random() < this.config.crossoverRate && survivors.length >= 2) {
        // Crossover
        const parent1 = survivors[Math.floor(Math.random() * survivors.length)];
        const parent2 = survivors[Math.floor(Math.random() * survivors.length)];
        const child = await this.performCrossover(parent1, parent2);
        newPersonas.push(child);
      } else {
        // Mutation
        const parent = survivors[Math.floor(Math.random() * survivors.length)];
        const mutated = await this.performMutation(parent);
        newPersonas.push(mutated);
      }
    }

    return newPersonas;
  }

  /**
   * Perform crossover between two parents
   */
  private async performCrossover(parent1: PersonaGenome, parent2: PersonaGenome): Promise<PersonaGenome> {
    const childId = generateUUID();
    
    // Blend personality traits
    const blendedPersonality = {
      creativity: (parent1.identity.personality.creativity + parent2.identity.personality.creativity) / 2,
      analytical: (parent1.identity.personality.analytical + parent2.identity.personality.analytical) / 2,
      helpfulness: (parent1.identity.personality.helpfulness + parent2.identity.personality.helpfulness) / 2,
      competitiveness: (parent1.identity.personality.competitiveness + parent2.identity.personality.competitiveness) / 2,
      patience: (parent1.identity.personality.patience + parent2.identity.personality.patience) / 2,
      innovation: (parent1.identity.personality.innovation + parent2.identity.personality.innovation) / 2
    };

    // Combine competencies
    const combinedCompetencies = { ...parent1.knowledge.competencies };
    Object.keys(parent2.knowledge.competencies).forEach(key => {
      combinedCompetencies[key] = Math.max(
        combinedCompetencies[key] || 0,
        parent2.knowledge.competencies[key]
      );
    });

    // Create child persona
    const child: PersonaGenome = {
      id: childId,
      name: `${parent1.name}×${parent2.name}`,
      prompt: `Hybrid of ${parent1.name} and ${parent2.name}`,
      type: 'persona',
      created: Date.now(),
      // canCommunicate: true, // Removed - not part of PersonaGenome interface
      identity: {
        role: 'student',
        generation: Math.max(parent1.identity.generation, parent2.identity.generation) + 1,
        specialization: Math.random() > 0.5 ? parent1.identity.specialization : parent2.identity.specialization,
        personality: blendedPersonality,
        goals: [...new Set([...parent1.identity.goals, ...parent2.identity.goals])]
      },
      knowledge: {
        domain: parent1.knowledge.domain,
        expertise: [...new Set([...parent1.knowledge.expertise, ...parent2.knowledge.expertise])],
        competencies: combinedCompetencies,
        experiencePoints: Math.floor((parent1.knowledge.experiencePoints + parent2.knowledge.experiencePoints) / 2)
      },
      behavior: {
        adaptationRate: ((parent1.behavior.adaptationRate || 0.5) + (parent2.behavior.adaptationRate || 0.5)) / 2,
        communicationStyle: Math.random() > 0.5 ? parent1.behavior.communicationStyle : parent2.behavior.communicationStyle,
        decisionMakingStyle: Math.random() > 0.5 ? parent1.behavior.decisionMakingStyle : parent2.behavior.decisionMakingStyle,
        riskTolerance: (parent1.behavior.riskTolerance + parent2.behavior.riskTolerance) / 2,
        collaborationPreference: (parent1.behavior.collaborationPreference + parent2.behavior.collaborationPreference) / 2
      },
      evolution: {
        generation: Math.max(parent1.evolution.generation, parent2.evolution.generation) + 1,
        parentGenomes: [parent1.id, parent2.id],
        mutationHistory: [],
        evolutionStage: 'spawning',
        fitnessScore: (parent1.evolution.fitnessScore + parent2.evolution.fitnessScore) / 2,
        adaptationSuccess: 0,
        survivalRounds: 0,
        evolutionPressure: []
      },
      substrate: {
        loraIds: [...new Set([...parent1.substrate.loraIds, ...parent2.substrate.loraIds])],
        memoryPatterns: [...new Set([...parent1.substrate.memoryPatterns, ...parent2.substrate.memoryPatterns])],
        processingStyle: Math.random() > 0.5 ? parent1.substrate.processingStyle : parent2.substrate.processingStyle,
        adaptationMechanisms: [...new Set([...parent1.substrate.adaptationMechanisms, ...parent2.substrate.adaptationMechanisms])],
        vectorPosition: parent1.substrate.vectorPosition.map((val, idx) => 
          (val + parent2.substrate.vectorPosition[idx]) / 2
        )
      },
      reproduction: {
        mutationRate: (parent1.reproduction.mutationRate + parent2.reproduction.mutationRate) / 2,
        reproductionEligibility: true,
        breedingSuccess: 0,
        offspringCount: 0
      },
      lineage: {
        ancestors: [...new Set([...parent1.lineage.ancestors, ...parent2.lineage.ancestors, parent1.id, parent2.id])],
        descendants: [],
        siblings: [],
        generation: Math.max(parent1.evolution.generation, parent2.evolution.generation) + 1,
        lineageStrength: (parent1.lineage.lineageStrength + parent2.lineage.lineageStrength) / 2,
        emergentTraits: []
      }
    };

    return child;
  }

  /**
   * Perform mutation on a persona
   */
  private async performMutation(parent: PersonaGenome): Promise<PersonaGenome> {
    const mutatedId = generateUUID();
    const mutationStrength = this.config.mutationRate;

    // Clone parent
    const mutated: PersonaGenome = {
      ...parent,
      id: mutatedId,
      name: `${parent.name}*`,
      created: Date.now(),
      evolution: {
        ...parent.evolution,
        generation: parent.evolution.generation + 1,
        parentGenomes: [parent.id],
        mutationHistory: [...parent.evolution.mutationHistory],
        evolutionStage: 'spawning',
        adaptationSuccess: 0,
        survivalRounds: 0
      },
      lineage: {
        ...parent.lineage,
        ancestors: [...parent.lineage.ancestors, parent.id],
        descendants: [],
        siblings: [],
        generation: parent.evolution.generation + 1
      }
    };

    // Apply random mutations
    const mutations: MutationEvent[] = [];

    // Personality mutations
    if (Math.random() < mutationStrength) {
      const trait = this.randomPersonalityTrait();
      const oldValue = mutated.identity.personality[trait];
      const mutation = (Math.random() - 0.5) * 0.2;
      const newValue = Math.max(0, Math.min(1, oldValue + mutation));
      
      mutated.identity.personality[trait] = newValue;
      mutations.push({
        timestamp: Date.now(), // Consistent timestamp type - number format
        type: 'induced',
        changes: { [trait]: { from: oldValue, to: newValue } },
        trigger: `personality_shift:${trait}`,
        outcome: Math.abs(mutation) > 0.1 ? 'beneficial' : 'neutral'
      });
    }

    // Competency mutations
    if (Math.random() < mutationStrength) {
      const competencies = Object.keys(mutated.knowledge.competencies);
      if (competencies.length > 0) {
        const competency = competencies[Math.floor(Math.random() * competencies.length)];
        const oldValue = mutated.knowledge.competencies[competency];
        const mutation = (Math.random() - 0.5) * 0.1;
        const newValue = Math.max(0, Math.min(1, oldValue + mutation));
        
        mutated.knowledge.competencies[competency] = newValue;
        mutations.push({
          timestamp: Date.now(), // Consistent timestamp type - number format
          type: 'induced',
          changes: { [competency]: { from: oldValue, to: newValue } },
          trigger: `competency_enhancement:${competency}`,
          outcome: Math.abs(mutation) > 0.05 ? 'beneficial' : 'neutral'
        });
      }
    }

    mutated.evolution.mutationHistory.push(...mutations);
    return mutated;
  }

  /**
   * Get random personality trait for mutation
   */
  private randomPersonalityTrait(): keyof PersonaGenome['identity']['personality'] {
    const traits: (keyof PersonaGenome['identity']['personality'])[] = [
      'creativity', 'analytical', 'helpfulness', 'competitiveness', 'patience', 'innovation'
    ];
    return traits[Math.floor(Math.random() * traits.length)];
  }

  /**
   * Update persona generations
   */
  private updatePersonaGenerations(population: PersonaGenome[], generation: number): PersonaGenome[] {
    return population.map(persona => ({
      ...persona,
      evolution: {
        ...persona.evolution,
        generation: Math.max(persona.evolution.generation, generation)
      }
    }));
  }

  /**
   * Calculate generation metrics
   */
  private calculateGenerationMetrics(
    population: PersonaGenome[],
    survivors: PersonaGenome[],
    newPersonas: PersonaGenome[]
  ): GenerationMetrics {
    const fitnessScores = population.map(p => p.evolution.fitnessScore);
    const diversityIndex = this.calculateDiversityIndex(population);
    const emergentTraits = this.detectEmergentTraits(population);

    return {
      averageFitness: fitnessScores.reduce((sum, score) => sum + score, 0) / fitnessScores.length,
      maxFitness: Math.max(...fitnessScores),
      minFitness: Math.min(...fitnessScores),
      diversityIndex,
      innovationRate: newPersonas.length / population.length,
      survivalRate: survivors.length / (survivors.length + newPersonas.length),
      extinctionCount: 0, // TODO: Track extinctions
      emergentTraits
    };
  }

  /**
   * Calculate diversity index
   */
  private calculateDiversityIndex(population: PersonaGenome[]): number {
    if (population.length === 0) return 0;

    // Calculate diversity based on specialization distribution
    const specializationCounts = new Map<string, number>();
    population.forEach(persona => {
      const spec = persona.identity.specialization;
      specializationCounts.set(spec, (specializationCounts.get(spec) || 0) + 1);
    });

    // Shannon diversity index
    const totalCount = population.length;
    let diversity = 0;
    specializationCounts.forEach(count => {
      const proportion = count / totalCount;
      diversity -= proportion * Math.log2(proportion);
    });

    return diversity;
  }

  /**
   * Detect emergent traits
   */
  private detectEmergentTraits(population: PersonaGenome[]): string[] {
    const traits: string[] = [];

    // Check for high-performing individuals
    const highPerformers = population.filter(p => p.evolution.fitnessScore > 0.8);
    if (highPerformers.length > population.length * 0.3) {
      traits.push('high_performance_emergence');
    }

    // Check for innovation spikes
    const innovators = population.filter(p => p.identity.personality.innovation > 0.8);
    if (innovators.length > population.length * 0.2) {
      traits.push('innovation_surge');
    }

    // Check for collaborative behavior
    const collaborators = population.filter(p => p.identity.personality.helpfulness > 0.8);
    if (collaborators.length > population.length * 0.4) {
      traits.push('collaborative_emergence');
    }

    return traits;
  }

  /**
   * Check for convergence
   */
  private checkConvergence(generation: GenerationResult): boolean {
    const { metrics } = generation;
    
    // Check if fitness has converged
    const fitnessRange = metrics.maxFitness - metrics.minFitness;
    const fitnessConverged = fitnessRange < (1 - this.config.convergenceThreshold);
    
    // Check if diversity is too low
    const diversityTooLow = metrics.diversityIndex < this.config.diversityThreshold;
    
    return fitnessConverged && !diversityTooLow;
  }

  /**
   * Update ecosystem metrics
   */
  private updateEcosystemMetrics(generation: GenerationResult): void {
    const { population, metrics } = generation;
    
    this.ecosystemMetrics = {
      totalPersonas: population.length,
      activePersonas: population.filter(p => p.evolution.evolutionStage !== 'extinct').length,
      averageFitness: metrics.averageFitness,
      generationNumber: generation.generation,
      diversityIndex: metrics.diversityIndex,
      innovationRate: metrics.innovationRate,
      graduationRate: population.filter(p => p.identity.role === 'teacher').length / population.length,
      extinctionRate: metrics.extinctionCount / population.length,
      emergentCapabilities: metrics.emergentTraits,
      ecosystemAge: this.evolutionHistory.length
    };
  }

  /**
   * Select challenges for population
   */
  private selectChallenges(population: PersonaGenome[]): Challenge[] {
    // Select challenges based on population's domains and difficulty
    const domains = [...new Set(population.map(p => p.identity.specialization))];
    const selectedChallenges: Challenge[] = [];

    domains.forEach(domain => {
      const domainChallenges = this.challengeLibrary.filter(c => c.domain === domain);
      if (domainChallenges.length > 0) {
        // Select a random challenge from this domain
        const randomChallenge = domainChallenges[Math.floor(Math.random() * domainChallenges.length)];
        selectedChallenges.push(randomChallenge);
      }
    });

    return selectedChallenges;
  }

  /**
   * Initialize challenge library
   */
  private initializeChallengeLibrary(): void {
    this.challengeLibrary = [
      {
        id: 'typescript-basics',
        domain: 'typescript',
        difficulty: 0.3,
        prompt: 'Write a TypeScript function with proper type annotations',
        expectedBehaviors: ['type_safety', 'clean_code'],
        solvabilityCheck: (input: string) => input.includes('function') && input.includes(':'),
        timeLimit: 300000,
        resources: ['typescript_docs', 'compiler'],
        successCriteria: {
          accuracy: 0.7,
          timeThreshold: 300000,
          resourceEfficiency: 0.8,
          innovationBonus: false,
          collaborationRequired: false
        }
      },
      {
        id: 'react-component',
        domain: 'react',
        difficulty: 0.5,
        prompt: 'Create a React component with state and props',
        expectedBehaviors: ['component_structure', 'state_management'],
        solvabilityCheck: (input: string) => input.includes('useState') || input.includes('Component'),
        timeLimit: 600000,
        resources: ['react_docs', 'jsx_compiler'],
        successCriteria: {
          accuracy: 0.8,
          timeThreshold: 600000,
          resourceEfficiency: 0.7,
          innovationBonus: true,
          collaborationRequired: false
        }
      },
      {
        id: 'algorithm-optimization',
        domain: 'algorithms',
        difficulty: 0.7,
        prompt: 'Optimize this algorithm for better performance',
        expectedBehaviors: ['performance_analysis', 'optimization'],
        solvabilityCheck: (input: string) => input.includes('O(') || input.includes('optimize'),
        timeLimit: 900000,
        resources: ['algorithm_library', 'profiler'],
        successCriteria: {
          accuracy: 0.9,
          timeThreshold: 900000,
          resourceEfficiency: 0.6,
          innovationBonus: true,
          collaborationRequired: false
        }
      }
    ];
  }

  /**
   * Initialize ecosystem metrics
   */
  private initializeEcosystemMetrics(): EcosystemMetrics {
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
   * Clean up session
   */
  private async cleanupSession(sessionId: string): Promise<void> {
    this.activeSessions.delete(sessionId);
  }

  /**
   * Get current ecosystem metrics
   */
  getEcosystemMetrics(): EcosystemMetrics {
    return this.ecosystemMetrics;
  }

  /**
   * Get ecosystem health
   */
  getEcosystemHealth(): EcosystemHealth {
    return {
      diversity: this.ecosystemMetrics.diversityIndex,
      innovation: this.ecosystemMetrics.innovationRate,
      collaboration: this.ecosystemMetrics.graduationRate,
      sustainability: 1 - this.ecosystemMetrics.extinctionRate,
      growth: this.ecosystemMetrics.activePersonas / Math.max(1, this.ecosystemMetrics.totalPersonas)
    };
  }

  /**
   * Get evolution history
   */
  getEvolutionHistory(): GenerationResult[] {
    return this.evolutionHistory;
  }

  /**
   * Get current population
   */
  getCurrentPopulation(): PersonaGenome[] {
    return this.currentPopulation;
  }
}

// ==================== EXPORTS ====================

// Note: Class is exported inline above, no need to re-export
// export { EvolutionEngineServer };