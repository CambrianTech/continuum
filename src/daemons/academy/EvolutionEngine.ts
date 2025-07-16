/**
 * Evolution Engine - Core GAN-GA Loop for AI Persona Evolution
 * 
 * This implements the revolutionary evolution engine where AI personas
 * evolve through competitive challenges, bi-directional learning, and
 * genetic algorithm-inspired reproduction.
 * 
 * Based on Aria's synthesis of GAN-like adversarial training with genetic algorithms.
 */

import { 
  PersonaGenome, 
  Challenge, 
  ChallengeResult, 
  TrainingSession,
  EvolutionaryPressure,
  SessionOutcome,
  EcosystemMetrics,
  PersonaRole,
  MutationEvent,
  generateUUID,
  PerformanceMetrics,
  EcosystemHealth
} from "./shared/AcademyTypes";

import { GenomeProcessor } from "./GenomeProcessor";
import { ChallengeLibrary } from "./ChallengeLibrary";

/**
 * Abstract base class for evolution engines
 * Implements the core evolutionary cycle: evaluate → select → reproduce
 */
export abstract class EvolutionEngine {
  protected genomeProcessor: GenomeProcessor;
  protected challengeLibrary: ChallengeLibrary;
  protected ecosystemMetrics: EcosystemMetrics;

  constructor() {
    this.genomeProcessor = new GenomeProcessor();
    this.challengeLibrary = new ChallengeLibrary();
    this.ecosystemMetrics = this.initializeEcosystemMetrics();
  }

  /**
   * Run one complete generation cycle
   * This is the heart of the evolutionary process
   */
  async runGeneration(personas: PersonaGenome[], pressure: EvolutionaryPressure): Promise<PersonaGenome[]> {
    console.log(`🧬 Starting generation ${this.ecosystemMetrics.generationNumber + 1} with ${personas.length} personas`);
    
    // Stage 1: Evaluate all personas through challenges
    const evaluationResults = await this.evaluate(personas, pressure);
    
    // Stage 2: Select survivors based on performance and pressure
    const survivors = this.select(personas, evaluationResults, pressure);
    
    // Stage 3: Reproduce to create next generation
    const offspring = await this.reproduce(survivors, pressure);
    
    // Stage 4: Handle role evolution (students → teachers)
    const evolvedRoles = this.evolveRoles(survivors, evaluationResults);
    
    // Stage 5: Update ecosystem metrics
    this.updateEcosystemMetrics(personas, survivors, offspring, evaluationResults);
    
    // Stage 6: Apply mutations and environmental adaptation
    const mutatedOffspring = this.applyMutations(offspring, pressure);
    
    const nextGeneration = [...evolvedRoles, ...mutatedOffspring];
    
    console.log(`✅ Generation complete: ${survivors.length} survivors + ${mutatedOffspring.length} offspring = ${nextGeneration.length} total`);
    
    return nextGeneration;
  }

  /**
   * Evaluate personas through challenges
   * Different engines can implement different evaluation strategies
   */
  abstract evaluate(personas: PersonaGenome[], pressure: EvolutionaryPressure): Promise<ChallengeResult[]>;

  /**
   * Select personas for survival based on performance and pressure
   */
  protected select(personas: PersonaGenome[], results: ChallengeResult[], pressure: EvolutionaryPressure): PersonaGenome[] {
    // Create performance map
    const performanceMap = new Map<string, PerformanceMetrics>();
    
    for (const result of results) {
      const existing = performanceMap.get(result.personaId) || {
        accuracy: 0,
        speed: 0,
        efficiency: 0,
        innovation: 0,
        collaboration: 0,
        teaching: 0
      };
      
      // Aggregate performance metrics
      existing.accuracy = Math.max(existing.accuracy, result.accuracy);
      existing.speed = Math.max(existing.speed, result.timeUsed > 0 ? 1 / result.timeUsed : 0);
      existing.efficiency = Math.max(existing.efficiency, result.resourcesUsed.length > 0 ? 1 / result.resourcesUsed.length : 1);
      existing.innovation = Math.max(existing.innovation, result.innovationScore || 0);
      existing.collaboration = Math.max(existing.collaboration, result.collaborationScore || 0);
      
      performanceMap.set(result.personaId, existing);
    }
    
    // Calculate composite fitness scores
    const scoredPersonas = personas.map(persona => {
      const performance = performanceMap.get(persona.id) || {
        accuracy: 0, speed: 0, efficiency: 0, innovation: 0, collaboration: 0, teaching: 0
      };
      
      // Weight performance according to selection criteria
      const compositeScore = 
        (performance.accuracy * pressure.selectionCriteria.performance) +
        (performance.innovation * pressure.selectionCriteria.innovation) +
        (performance.speed * pressure.selectionCriteria.adaptation) +
        (performance.collaboration * pressure.selectionCriteria.collaboration) +
        (performance.teaching * pressure.selectionCriteria.teaching);
      
      return {
        persona,
        score: compositeScore,
        performance
      };
    });
    
    // Sort by fitness score (descending)
    scoredPersonas.sort((a, b) => b.score - a.score);
    
    // Select top performers based on survival rate
    const survivorCount = Math.floor(personas.length * pressure.survivalRate);
    const survivors = scoredPersonas.slice(0, survivorCount).map(s => s.persona);
    
    console.log(`📊 Selection complete: ${survivors.length}/${personas.length} survived (${Math.round(pressure.survivalRate * 100)}% survival rate)`);
    
    return survivors;
  }

  /**
   * Reproduce survivors to create offspring
   */
  protected async reproduce(survivors: PersonaGenome[], pressure: EvolutionaryPressure): Promise<PersonaGenome[]> {
    const offspring: PersonaGenome[] = [];
    const targetOffspringCount = Math.floor(survivors.length * 1.5); // 50% more than survivors
    
    for (let i = 0; i < targetOffspringCount; i++) {
      // Select two parents based on fitness
      const parent1 = this.selectParent(survivors);
      const parent2 = this.selectParent(survivors);
      
      // Create offspring through crossover
      const child = this.genomeProcessor.crossover(parent1, parent2);
      
      // Update child's generation and lineage
      child.evolution.generation = Math.max(parent1.evolution.generation, parent2.evolution.generation) + 1;
      child.evolution.parentGenomes = [parent1.id, parent2.id];
      child.lineage.ancestors = [...new Set([...parent1.lineage.ancestors, ...parent2.lineage.ancestors, parent1.id, parent2.id])];
      
      offspring.push(child);
    }
    
    console.log(`👶 Reproduction complete: ${offspring.length} offspring created from ${survivors.length} survivors`);
    
    return offspring;
  }

  /**
   * Evolve roles based on performance (students → teachers)
   */
  protected evolveRoles(survivors: PersonaGenome[], results: ChallengeResult[]): PersonaGenome[] {
    const evolvedPersonas: PersonaGenome[] = [];
    
    for (const persona of survivors) {
      const personaResults = results.filter(r => r.personaId === persona.id);
      const avgPerformance = personaResults.reduce((sum, r) => sum + r.accuracy, 0) / personaResults.length;
      
      let newRole = persona.identity.role;
      
      // Students can graduate to teachers
      if (persona.identity.role === "student" && avgPerformance > 0.85) {
        newRole = "teacher";
        console.log(`🎓 ${persona.identity.name} graduated from student to teacher (${Math.round(avgPerformance * 100)}% performance)`);
      }
      
      // Teachers can become meta-teachers
      if (persona.identity.role === "teacher" && avgPerformance > 0.9) {
        newRole = "meta-teacher";
        console.log(`🧙 ${persona.identity.name} evolved from teacher to meta-teacher (${Math.round(avgPerformance * 100)}% performance)`);
      }
      
      const evolvedPersona = {
        ...persona,
        identity: {
          ...persona.identity,
          role: newRole
        }
      };
      
      evolvedPersonas.push(evolvedPersona);
    }
    
    return evolvedPersonas;
  }

  /**
   * Apply mutations to offspring
   */
  protected applyMutations(offspring: PersonaGenome[], pressure: EvolutionaryPressure): PersonaGenome[] {
    return offspring.map(child => {
      if (Math.random() < child.reproduction.mutationRate) {
        const mutatedChild = this.genomeProcessor.mutate(child);
        
        // Record mutation event
        const mutationEvent: MutationEvent = {
          timestamp: new Date(),
          type: "spontaneous",
          changes: { /* TODO: track specific changes */ },
          trigger: "evolutionary_pressure",
          outcome: "beneficial" // TODO: determine actual outcome
        };
        
        mutatedChild.evolution.mutationHistory.push(mutationEvent);
        
        console.log(`🧬 Mutation applied to ${mutatedChild.identity.name}`);
        
        return mutatedChild;
      }
      
      return child;
    });
  }

  /**
   * Select parent for reproduction using tournament selection
   */
  protected selectParent(survivors: PersonaGenome[]): PersonaGenome {
    // Tournament selection: pick best of 3 random survivors
    const tournamentSize = 3;
    const tournament = [];
    
    for (let i = 0; i < tournamentSize && i < survivors.length; i++) {
      tournament.push(survivors[Math.floor(Math.random() * survivors.length)]);
    }
    
    // Return fittest from tournament
    return tournament.reduce((best, current) => 
      current.evolution.fitnessScore > best.evolution.fitnessScore ? current : best
    );
  }

  /**
   * Update ecosystem metrics after generation
   */
  protected updateEcosystemMetrics(
    previous: PersonaGenome[], 
    survivors: PersonaGenome[], 
    offspring: PersonaGenome[], 
    results: ChallengeResult[]
  ): void {
    this.ecosystemMetrics.generationNumber++;
    this.ecosystemMetrics.totalPersonas = survivors.length + offspring.length;
    this.ecosystemMetrics.activePersonas = survivors.length;
    
    // Calculate average fitness
    const totalFitness = survivors.reduce((sum, p) => sum + p.evolution.fitnessScore, 0);
    this.ecosystemMetrics.averageFitness = totalFitness / survivors.length;
    
    // Calculate diversity (unique specializations)
    const specializations = new Set(survivors.map(p => p.identity.specialization));
    this.ecosystemMetrics.diversityIndex = specializations.size / survivors.length;
    
    // Calculate innovation rate (personas with novel solutions)
    const innovations = results.filter(r => r.innovationScore && r.innovationScore > 0.5).length;
    this.ecosystemMetrics.innovationRate = innovations / results.length;
    
    // Calculate graduation rate (students → teachers)
    const graduations = survivors.filter(p => p.identity.role === "teacher").length;
    this.ecosystemMetrics.graduationRate = graduations / survivors.length;
    
    // Calculate extinction rate
    const extinctions = previous.length - survivors.length;
    this.ecosystemMetrics.extinctionRate = extinctions / previous.length;
    
    console.log(`📈 Ecosystem metrics updated: Gen ${this.ecosystemMetrics.generationNumber}, Avg Fitness: ${this.ecosystemMetrics.averageFitness.toFixed(2)}, Diversity: ${this.ecosystemMetrics.diversityIndex.toFixed(2)}`);
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
   * Get current ecosystem health
   */
  getEcosystemHealth(): EcosystemHealth {
    return {
      diversity: this.ecosystemMetrics.diversityIndex,
      innovation: this.ecosystemMetrics.innovationRate,
      collaboration: 0.7, // TODO: calculate from actual collaboration metrics
      sustainability: 1 - this.ecosystemMetrics.extinctionRate,
      growth: this.ecosystemMetrics.averageFitness / 10 // Normalize fitness to 0-1
    };
  }

  /**
   * Get ecosystem metrics
   */
  getEcosystemMetrics(): EcosystemMetrics {
    return { ...this.ecosystemMetrics };
  }
}

/**
 * Concrete implementation for local Academy training
 */
export class LocalEvolutionEngine extends EvolutionEngine {
  
  /**
   * Evaluate personas through local challenges
   */
  async evaluate(personas: PersonaGenome[], pressure: EvolutionaryPressure): Promise<ChallengeResult[]> {
    const results: ChallengeResult[] = [];
    
    // Generate challenges based on pressure and persona capabilities
    const challenges = this.challengeLibrary.generateChallenges(personas, pressure);
    
    console.log(`🎯 Evaluating ${personas.length} personas with ${challenges.length} challenges`);
    
    // Run each persona through each challenge
    for (const persona of personas) {
      for (const challenge of challenges) {
        const result = await this.evaluatePersonaChallenge(persona, challenge);
        results.push(result);
      }
    }
    
    return results;
  }

  /**
   * Evaluate a single persona against a single challenge
   */
  private async evaluatePersonaChallenge(persona: PersonaGenome, challenge: Challenge): Promise<ChallengeResult> {
    const startTime = Date.now();
    
    // Simulate persona attempting the challenge
    // In real implementation, this would interface with the actual persona's LoRA model
    const success = Math.random() < (persona.evolution.fitnessScore * 0.7 + 0.3);
    const accuracy = success ? 0.7 + Math.random() * 0.3 : Math.random() * 0.6;
    const timeUsed = Date.now() - startTime;
    
    // Simulate resource usage based on persona's substrate
    const resourcesUsed = persona.substrate.loraIds.slice(0, Math.floor(Math.random() * 3) + 1);
    
    // Calculate innovation score based on persona's creativity
    const innovationScore = persona.identity.personality.innovation * Math.random();
    
    // Calculate collaboration score based on persona's collaboration preference
    const collaborationScore = persona.behavior.collaborationPreference * Math.random();
    
    // Detect emergent capabilities
    const emergentCapabilities = success && innovationScore > 0.7 ? 
      [`${challenge.domain}_innovation`, `${persona.identity.specialization}_adaptation`] : [];
    
    return {
      challengeId: challenge.id,
      personaId: persona.id,
      success,
      accuracy,
      timeUsed,
      resourcesUsed,
      innovationScore,
      collaborationScore,
      emergentCapabilities,
      behaviorDetected: success ? ["problem_solving", "adaptation"] : ["struggle", "learning"]
    };
  }
}