/**
 * Challenge Library - Dynamic Challenge Generation for Persona Evolution
 * 
 * This creates adaptive challenges that evolve with the persona ecosystem,
 * providing appropriate difficulty progression and domain diversity.
 * 
 * Challenges are the selective pressure that drives persona evolution.
 */

import { 
  Challenge, 
  PersonaGenome, 
  EvolutionaryPressure, 
  generateUUID 
} from "./shared/AcademyTypes";

/**
 * ChallengeLibrary generates and manages challenges for persona evolution
 */
export class ChallengeLibrary {
  private challengeTemplates: ChallengeTemplate[] = [];
  // private generatedChallenges: Challenge[] = [];
  
  constructor() {
    this.initializeChallengeTemplates();
  }
  
  /**
   * Generate challenges based on persona capabilities and evolutionary pressure
   */
  generateChallenges(personas: PersonaGenome[], pressure: EvolutionaryPressure): Challenge[] {
    const challenges: Challenge[] = [];
    
    // Analyze persona capabilities to create appropriate challenges
    const domainDistribution = this.analyzePersonaDomains(personas);
    const averageFitness = this.calculateAverageFitness(personas);
    
    // Generate challenges for each domain
    for (const [domain, count] of domainDistribution) {
      const domainChallenges = this.generateDomainChallenges(domain, count, averageFitness, pressure);
      challenges.push(...domainChallenges);
    }
    
    // Add cross-domain challenges for collaboration
    const crossDomainChallenges = this.generateCrossDomainChallenges(personas, pressure);
    challenges.push(...crossDomainChallenges);
    
    console.log(`🎯 Generated ${challenges.length} challenges across ${domainDistribution.size} domains`);
    
    return challenges;
  }
  
  /**
   * Generate challenges for a specific domain
   */
  private generateDomainChallenges(
    domain: string, 
    personaCount: number, 
    averageFitness: number, 
    pressure: EvolutionaryPressure
  ): Challenge[] {
    const challenges: Challenge[] = [];
    const templates = this.challengeTemplates.filter(t => t.domain === domain);
    
    // Generate 2-3 challenges per domain
    const challengeCount = Math.min(3, Math.max(1, personaCount));
    
    for (let i = 0; i < challengeCount; i++) {
      const template = this.selectTemplate(templates, averageFitness, pressure);
      if (template) {
        const challenge = this.instantiateChallenge(template, averageFitness, pressure);
        challenges.push(challenge);
      }
    }
    
    return challenges;
  }
  
  /**
   * Generate cross-domain challenges that require collaboration
   */
  private generateCrossDomainChallenges(_personas: PersonaGenome[], pressure: EvolutionaryPressure): Challenge[] {
    const challenges: Challenge[] = [];
    
    // Only generate collaborative challenges if pressure requires it
    if (pressure.selectionCriteria.collaboration > 0.3) {
      const collaborativeChallenge: Challenge = {
        id: generateUUID(),
        domain: "collaboration",
        difficulty: 0.7,
        prompt: "Work together to design and implement a complete feature with testing and documentation",
        expectedBehaviors: ["collaboration", "communication", "knowledge_sharing", "coordination"],
        solvabilityCheck: (input: string) => {
          return input.includes("collaboration") && input.includes("implementation") && input.includes("testing");
        },
        timeLimit: 1800000, // 30 minutes
        resources: ["multiple_lora_adapters", "shared_memory", "communication_channel"],
        successCriteria: {
          accuracy: 0.8,
          timeThreshold: 1800000,
          resourceEfficiency: 0.6,
          innovationBonus: true,
          collaborationRequired: true
        }
      };
      
      challenges.push(collaborativeChallenge);
    }
    
    return challenges;
  }
  
  /**
   * Select appropriate template based on fitness and pressure
   */
  private selectTemplate(
    templates: ChallengeTemplate[], 
    averageFitness: number, 
    pressure: EvolutionaryPressure
  ): ChallengeTemplate | null {
    if (templates.length === 0) return null;
    
    // Filter templates by appropriate difficulty
    const targetDifficulty = this.calculateTargetDifficulty(averageFitness, pressure);
    const suitableTemplates = templates.filter(t => 
      Math.abs(t.difficulty - targetDifficulty) < 0.3
    );
    
    if (suitableTemplates.length === 0) {
      // Fall back to any template if no suitable ones found
      return templates[Math.floor(Math.random() * templates.length)];
    }
    
    // Select randomly from suitable templates
    return suitableTemplates[Math.floor(Math.random() * suitableTemplates.length)];
  }
  
  /**
   * Calculate target difficulty based on fitness and pressure
   */
  private calculateTargetDifficulty(averageFitness: number, pressure: EvolutionaryPressure): number {
    // Base difficulty on average fitness
    let difficulty = averageFitness * 0.8 + 0.2; // Scale from 0.2 to 1.0
    
    // Adjust based on competition level
    difficulty += pressure.competitionLevel * 0.2;
    
    // Ensure difficulty is within bounds
    return Math.max(0.1, Math.min(0.9, difficulty));
  }
  
  /**
   * Instantiate a challenge from a template
   */
  private instantiateChallenge(
    template: ChallengeTemplate, 
    averageFitness: number, 
    pressure: EvolutionaryPressure
  ): Challenge {
    const challenge: Challenge = {
      id: generateUUID(),
      domain: template.domain,
      difficulty: this.calculateTargetDifficulty(averageFitness, pressure),
      prompt: this.instantiatePrompt(template.promptTemplate, template.variables),
      expectedBehaviors: template.expectedBehaviors,
      solvabilityCheck: template.solvabilityCheck,
      timeLimit: template.timeLimit,
      resources: template.resources,
      successCriteria: {
        accuracy: Math.max(0.5, averageFitness * 0.9),
        timeThreshold: template.timeLimit,
        resourceEfficiency: 0.7,
        innovationBonus: pressure.selectionCriteria.innovation > 0.3,
        collaborationRequired: pressure.selectionCriteria.collaboration > 0.5
      }
    };
    
    return challenge;
  }
  
  /**
   * Instantiate prompt from template with variables
   */
  private instantiatePrompt(template: string, variables: Record<string, string[]>): string {
    let prompt = template;
    
    // Replace variables with random choices
    for (const [variable, choices] of Object.entries(variables)) {
      const choice = choices[Math.floor(Math.random() * choices.length)];
      prompt = prompt.replace(`{${variable}}`, choice);
    }
    
    return prompt;
  }
  
  /**
   * Analyze persona domains to understand ecosystem composition
   */
  private analyzePersonaDomains(personas: PersonaGenome[]): Map<string, number> {
    const domains = new Map<string, number>();
    
    for (const persona of personas) {
      const domain = persona.identity.specialization;
      domains.set(domain, (domains.get(domain) || 0) + 1);
    }
    
    return domains;
  }
  
  /**
   * Calculate average fitness across all personas
   */
  private calculateAverageFitness(personas: PersonaGenome[]): number {
    if (personas.length === 0) return 0.5;
    
    const totalFitness = personas.reduce((sum, p) => sum + p.evolution.fitnessScore, 0);
    return totalFitness / personas.length;
  }
  
  /**
   * Initialize challenge templates
   */
  private initializeChallengeTemplates(): void {
    this.challengeTemplates = [
      // TypeScript challenges
      {
        domain: "typescript",
        difficulty: 0.3,
        promptTemplate: "Implement a {complexity} TypeScript function that handles {scenario} with proper error handling and type safety",
        variables: {
          complexity: ["simple", "moderately complex", "advanced"],
          scenario: ["user authentication", "data validation", "API integration", "async operations"]
        },
        expectedBehaviors: ["type_safety", "error_handling", "clean_code"],
        solvabilityCheck: (input: string) => input.includes("function") && input.includes("TypeScript"),
        timeLimit: 300000, // 5 minutes
        resources: ["typescript_lora", "code_examples", "documentation"]
      },
      
      // Testing challenges
      {
        domain: "testing",
        difficulty: 0.4,
        promptTemplate: "Write comprehensive tests for a {component_type} that covers {test_scenarios} with {coverage_requirement}% coverage",
        variables: {
          component_type: ["React component", "API endpoint", "utility function", "database layer"],
          test_scenarios: ["happy path and edge cases", "error conditions", "integration scenarios"],
          coverage_requirement: ["80", "85", "90", "95"]
        },
        expectedBehaviors: ["test_coverage", "edge_case_handling", "documentation"],
        solvabilityCheck: (input: string) => input.includes("test") && input.includes("coverage"),
        timeLimit: 600000, // 10 minutes
        resources: ["testing_lora", "jest_framework", "testing_utilities"]
      },
      
      // Architecture challenges
      {
        domain: "architecture",
        difficulty: 0.6,
        promptTemplate: "Design a {system_type} architecture that supports {requirements} while maintaining {quality_attributes}",
        variables: {
          system_type: ["microservices", "monolithic", "serverless", "hybrid"],
          requirements: ["high scalability", "real-time processing", "data consistency", "fault tolerance"],
          quality_attributes: ["performance", "maintainability", "security", "extensibility"]
        },
        expectedBehaviors: ["system_design", "trade_off_analysis", "scalability_planning"],
        solvabilityCheck: (input: string) => input.includes("architecture") && input.includes("design"),
        timeLimit: 1200000, // 20 minutes
        resources: ["architecture_lora", "design_patterns", "system_examples"]
      },
      
      // UI Design challenges
      {
        domain: "ui_design",
        difficulty: 0.5,
        promptTemplate: "Create a {ui_component} design that provides {user_experience} for {user_type} users",
        variables: {
          ui_component: ["dashboard", "form", "navigation", "data visualization"],
          user_experience: ["intuitive interaction", "accessibility", "responsive design", "performance"],
          user_type: ["novice", "expert", "mobile", "desktop"]
        },
        expectedBehaviors: ["user_experience", "accessibility", "visual_design"],
        solvabilityCheck: (input: string) => input.includes("design") && input.includes("user"),
        timeLimit: 900000, // 15 minutes
        resources: ["ui_design_lora", "design_systems", "user_research"]
      },
      
      // Debugging challenges
      {
        domain: "debugging",
        difficulty: 0.7,
        promptTemplate: "Debug a {bug_type} issue in {system_context} that causes {symptoms} under {conditions}",
        variables: {
          bug_type: ["memory leak", "race condition", "performance bottleneck", "integration failure"],
          system_context: ["web application", "API service", "database query", "background process"],
          symptoms: ["slow response times", "crashes", "incorrect results", "resource exhaustion"],
          conditions: ["high load", "specific user actions", "edge cases", "concurrent access"]
        },
        expectedBehaviors: ["problem_analysis", "systematic_debugging", "root_cause_identification"],
        solvabilityCheck: (input: string) => input.includes("debug") && input.includes("issue"),
        timeLimit: 1800000, // 30 minutes
        resources: ["debugging_lora", "profiling_tools", "logging_systems"]
      },
      
      // Optimization challenges
      {
        domain: "optimization",
        difficulty: 0.8,
        promptTemplate: "Optimize {system_component} to improve {performance_metric} by {improvement_target}% while maintaining {constraints}",
        variables: {
          system_component: ["database queries", "API endpoints", "rendering pipeline", "data processing"],
          performance_metric: ["response time", "throughput", "memory usage", "CPU utilization"],
          improvement_target: ["20", "30", "50", "75"],
          constraints: ["backwards compatibility", "code readability", "system reliability", "security"]
        },
        expectedBehaviors: ["performance_analysis", "optimization_strategies", "measurement_validation"],
        solvabilityCheck: (input: string) => input.includes("optimize") && input.includes("performance"),
        timeLimit: 1500000, // 25 minutes
        resources: ["optimization_lora", "profiling_tools", "benchmarking_frameworks"]
      }
    ];
  }
}

/**
 * Challenge template for generating specific challenges
 */
interface ChallengeTemplate {
  domain: string;
  difficulty: number;
  promptTemplate: string;
  variables: Record<string, string[]>;
  expectedBehaviors: string[];
  solvabilityCheck: (input: string) => boolean;
  timeLimit: number;
  resources: string[];
}