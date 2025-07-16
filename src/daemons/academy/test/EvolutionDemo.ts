/**
 * Evolution Demo - Demonstrate the Academy Evolution Engine
 * 
 * This shows the revolutionary AI evolution system in action:
 * personas competing, evolving, and improving through challenges.
 */

import { LocalEvolutionEngine } from "../EvolutionEngine";
import { 
  PersonaGenome, 
  EvolutionaryPressure, 
  PersonaRole,
  generateUUID 
} from "../shared/AcademyTypes";

/**
 * Create initial population of personas
 */
function createInitialPopulation(size: number = 10): PersonaGenome[] {
  const personas: PersonaGenome[] = [];
  const specializations = ['typescript', 'testing', 'architecture', 'ui_design', 'debugging', 'optimization'];
  const names = ['Alpha', 'Beta', 'Gamma', 'Delta', 'Epsilon', 'Zeta', 'Eta', 'Theta', 'Iota', 'Kappa'];
  
  for (let i = 0; i < size; i++) {
    const specialization = specializations[i % specializations.length];
    const name = `${specialization}${names[i % names.length]}`;
    
    const persona: PersonaGenome = {
      id: generateUUID(),
      identity: {
        name: name,
        role: "student" as PersonaRole,
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
        learningStyle: "analytical",
        adaptationRate: 0.3 + Math.random() * 0.4,
        communicationStyle: "direct",
        decisionMakingStyle: "analytical",
        riskTolerance: 0.3 + Math.random() * 0.4,
        collaborationPreference: 0.3 + Math.random() * 0.4
      },
      evolution: {
        generation: 0,
        parentGenomes: [],
        mutationHistory: [],
        evolutionStage: "spawning",
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
    
    personas.push(persona);
  }
  
  return personas;
}

/**
 * Create evolutionary pressure configuration
 */
function createEvolutionaryPressure(generation: number): EvolutionaryPressure {
  // Increase pressure over generations
  const pressureMultiplier = Math.min(1.5, 1 + generation * 0.1);
  
  return {
    survivalRate: Math.max(0.4, 0.7 - generation * 0.05), // Gradually decrease survival rate
    selectionCriteria: {
      performance: 0.4 * pressureMultiplier,
      innovation: 0.2 * pressureMultiplier,
      adaptation: 0.2 * pressureMultiplier,
      collaboration: 0.15 * pressureMultiplier,
      teaching: 0.05 * pressureMultiplier
    },
    environmentalFactors: ['competition', 'resource_scarcity', 'complexity_increase'],
    competitionLevel: Math.min(1.0, 0.3 + generation * 0.1),
    collaborationRequirement: Math.min(1.0, 0.2 + generation * 0.05)
  };
}

/**
 * Display generation statistics
 */
function displayGenerationStats(generation: number, personas: PersonaGenome[], engine: LocalEvolutionEngine) {
  console.log(`\n🧬 ======== GENERATION ${generation} ========`);
  console.log(`📊 Population: ${personas.length} personas`);
  
  // Role distribution
  const roleDistribution = personas.reduce((acc, p) => {
    acc[p.identity.role] = (acc[p.identity.role] || 0) + 1;
    return acc;
  }, {} as Record<string, number>);
  
  console.log(`👥 Roles: ${Object.entries(roleDistribution).map(([role, count]) => `${role}: ${count}`).join(', ')}`);
  
  // Specialization distribution
  const specDistribution = personas.reduce((acc, p) => {
    acc[p.identity.specialization] = (acc[p.identity.specialization] || 0) + 1;
    return acc;
  }, {} as Record<string, number>);
  
  console.log(`🎯 Specializations: ${Object.entries(specDistribution).map(([spec, count]) => `${spec}: ${count}`).join(', ')}`);
  
  // Fitness statistics
  const fitnessScores = personas.map(p => p.evolution.fitnessScore);
  const avgFitness = fitnessScores.reduce((a, b) => a + b, 0) / fitnessScores.length;
  const maxFitness = Math.max(...fitnessScores);
  const minFitness = Math.min(...fitnessScores);
  
  console.log(`📈 Fitness: Avg: ${avgFitness.toFixed(2)}, Max: ${maxFitness.toFixed(2)}, Min: ${minFitness.toFixed(2)}`);
  
  // Top performers
  const topPerformers = personas
    .sort((a, b) => b.evolution.fitnessScore - a.evolution.fitnessScore)
    .slice(0, 3);
  
  console.log(`🏆 Top Performers:`);
  topPerformers.forEach((p, i) => {
    console.log(`   ${i + 1}. ${p.identity.name} (${p.identity.role}) - ${p.evolution.fitnessScore.toFixed(2)} fitness`);
  });
  
  // Ecosystem health
  const ecosystemHealth = engine.getEcosystemHealth();
  console.log(`🌿 Ecosystem Health: Diversity: ${ecosystemHealth.diversity.toFixed(2)}, Innovation: ${ecosystemHealth.innovation.toFixed(2)}, Growth: ${ecosystemHealth.growth.toFixed(2)}`);
}

/**
 * Display lineage information
 */
function displayLineageInfo(personas: PersonaGenome[]) {
  console.log(`\n🌳 ======== LINEAGE TRACKING ========`);
  
  // Find personas with interesting lineages
  const withLineage = personas.filter(p => p.lineage.ancestors.length > 0);
  
  if (withLineage.length > 0) {
    console.log(`📊 ${withLineage.length} personas have lineage information:`);
    
    withLineage.slice(0, 5).forEach(persona => {
      console.log(`   ${persona.identity.name} (Gen ${persona.evolution.generation})`);
      console.log(`     Ancestors: ${persona.lineage.ancestors.length}`);
      console.log(`     Lineage Strength: ${persona.lineage.lineageStrength.toFixed(2)}`);
      if (persona.lineage.emergentTraits.length > 0) {
        console.log(`     Emergent Traits: ${persona.lineage.emergentTraits.join(', ')}`);
      }
    });
  }
  
  // Generation statistics
  const generationStats = personas.reduce((acc, p) => {
    acc[p.evolution.generation] = (acc[p.evolution.generation] || 0) + 1;
    return acc;
  }, {} as Record<number, number>);
  
  console.log(`📈 Generation Distribution: ${Object.entries(generationStats).map(([gen, count]) => `Gen ${gen}: ${count}`).join(', ')}`);
}

/**
 * Run evolution demonstration
 */
export async function runEvolutionDemo(generations: number = 5) {
  console.log(`🎓 Starting Academy Evolution Demo - ${generations} generations`);
  console.log(`🌟 This demonstrates the revolutionary AI evolution system where personas compete, evolve, and improve!\n`);
  
  // Initialize evolution engine
  const engine = new LocalEvolutionEngine();
  
  // Create initial population
  let population = createInitialPopulation(10);
  
  // Display initial state
  displayGenerationStats(0, population, engine);
  
  // Run evolution for specified generations
  for (let generation = 1; generation <= generations; generation++) {
    console.log(`\n⚡ Running Generation ${generation}...`);
    
    // Create evolutionary pressure
    const pressure = createEvolutionaryPressure(generation);
    
    // Run one generation
    population = await engine.runGeneration(population, pressure);
    
    // Display results
    displayGenerationStats(generation, population, engine);
    
    // Show lineage information every few generations
    if (generation % 2 === 0) {
      displayLineageInfo(population);
    }
    
    // Brief pause to make it readable
    await new Promise(resolve => setTimeout(resolve, 1000));
  }
  
  console.log(`\n🎉 Evolution Demo Complete!`);
  console.log(`✨ The Academy has successfully evolved ${population.length} personas over ${generations} generations`);
  console.log(`🏆 This is just the beginning - imagine this running continuously with real LoRA training!`);
  
  // Final ecosystem metrics
  const finalMetrics = engine.getEcosystemMetrics();
  console.log(`\n📊 Final Ecosystem Metrics:`);
  console.log(`   Generation: ${finalMetrics.generationNumber}`);
  console.log(`   Total Personas: ${finalMetrics.totalPersonas}`);
  console.log(`   Average Fitness: ${finalMetrics.averageFitness.toFixed(2)}`);
  console.log(`   Diversity Index: ${finalMetrics.diversityIndex.toFixed(2)}`);
  console.log(`   Innovation Rate: ${finalMetrics.innovationRate.toFixed(2)}`);
  console.log(`   Graduation Rate: ${finalMetrics.graduationRate.toFixed(2)}`);
  
  return population;
}

// Run demo if this file is executed directly
if (require.main === module) {
  runEvolutionDemo(5)
    .then(() => process.exit(0))
    .catch(error => {
      console.error('Demo failed:', error);
      process.exit(1);
    });
}