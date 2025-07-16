/**
 * PersonaImporter - Flexible persona import system
 * 
 * This system enables bringing any prompt-based persona into the Academy evolution system.
 * Key principle: Any PersonaBase can be enhanced into a PersonaGenome for evolution.
 * 
 * Import sources:
 * - Simple prompt strings
 * - PersonaBase objects
 * - External persona formats
 * - Template-based generation
 */

import { PersonaBase, PersonaType, CommunicationStyle, PersonaTemplates } from './PersonaBase';
import { PersonaGenome, PersonaRole, PersonalityTraits, generateUUID, SPECIALIZATIONS } from './AcademyTypes';

// ==================== IMPORT CONFIGURATION ====================

/**
 * Configuration for importing personas into the Academy
 */
export interface PersonaImportConfig {
  // Basic persona information
  name?: string;
  role?: PersonaRole;
  specialization?: string;
  
  // Behavioral mapping
  communicationStyleMapping?: Record<CommunicationStyle, string>;
  personalityDefaults?: Partial<PersonalityTraits>;
  
  // Evolution settings
  initialFitness?: number;
  mutationRate?: number;
  generation?: number;
  
  // Optional enhancements
  goals?: string[];
  expertise?: string[];
  
  // Import strategy
  preserveOriginal?: boolean;
  enhancementLevel?: 'minimal' | 'standard' | 'full';
}

/**
 * Default import configuration
 */
export const DEFAULT_IMPORT_CONFIG: PersonaImportConfig = {
  role: 'student',
  communicationStyleMapping: {
    'professional': 'direct',
    'casual': 'conversational',
    'academic': 'analytical',
    'technical': 'precise',
    'creative': 'expressive',
    'supportive': 'encouraging',
    'direct': 'direct',
    'diplomatic': 'diplomatic'
  },
  personalityDefaults: {
    creativity: 0.5,
    analytical: 0.6,
    helpfulness: 0.7,
    competitiveness: 0.4,
    patience: 0.6,
    innovation: 0.5
  },
  initialFitness: 0.5,
  mutationRate: 0.1,
  generation: 0,
  preserveOriginal: true,
  enhancementLevel: 'standard'
};

// ==================== PERSONA IMPORTER ====================

/**
 * Main persona importer class
 */
export class PersonaImporter {
  private config: PersonaImportConfig;

  constructor(config: PersonaImportConfig = {}) {
    this.config = { ...DEFAULT_IMPORT_CONFIG, ...config };
  }

  /**
   * Import from a simple prompt string
   */
  async importFromPrompt(prompt: string, name?: string): Promise<PersonaGenome> {
    // Create a basic PersonaBase from the prompt
    const personaBase: PersonaBase = {
      id: generateUUID(),
      name: name || this.generateNameFromPrompt(prompt),
      prompt,
      description: `Imported persona from prompt`,
      created: Date.now(),
      metadata: {
        version: '1.0.0',
        tags: ['imported', 'prompt-based'],
        category: 'imported',
        importedAt: Date.now()
      }
    };

    return this.enhanceToGenome(personaBase);
  }

  /**
   * Import from PersonaBase object
   */
  async importFromBase(personaBase: PersonaBase): Promise<PersonaGenome> {
    return this.enhanceToGenome(personaBase);
  }

  /**
   * Import from template
   */
  async importFromTemplate(templateName: keyof typeof PersonaTemplates, customName?: string): Promise<PersonaGenome> {
    const template = PersonaTemplates[templateName];
    if (!template) {
      throw new Error(`Template ${templateName} not found`);
    }

    const personaBase = template(customName);
    return this.enhanceToGenome(personaBase);
  }

  /**
   * Import multiple personas from various sources
   */
  async importBatch(sources: Array<{
    type: 'prompt' | 'base' | 'template';
    data: any;
    name?: string;
    config?: Partial<PersonaImportConfig>;
  }>): Promise<PersonaGenome[]> {
    const promises = sources.map(async (source) => {
      const tempImporter = new PersonaImporter({ ...this.config, ...source.config });
      
      switch (source.type) {
        case 'prompt':
          return tempImporter.importFromPrompt(source.data, source.name);
        case 'base':
          return tempImporter.importFromBase(source.data);
        case 'template':
          return tempImporter.importFromTemplate(source.data, source.name);
        default:
          throw new Error(`Unsupported source type: ${source.type}`);
      }
    });

    return Promise.all(promises);
  }

  // ==================== ENHANCEMENT METHODS ====================

  /**
   * Enhance PersonaBase to PersonaGenome
   */
  private async enhanceToGenome(personaBase: PersonaBase): Promise<PersonaGenome> {
    const specialization = this.inferSpecialization(personaBase);
    const personality = this.inferPersonality(personaBase);
    const communicationStyle = this.inferCommunicationStyle(personaBase);

    const genome: PersonaGenome = {
      // Inherit all PersonaBase properties
      ...personaBase,
      
      // Enhanced identity
      identity: {
        role: this.config.role || 'student',
        generation: this.config.generation || 0,
        parentIds: [],
        specialization,
        personality,
        goals: this.config.goals || this.generateGoals(specialization, this.config.role || 'student')
      },

      // Knowledge system
      knowledge: {
        domain: specialization,
        expertise: this.config.expertise || [specialization, 'general'],
        competencies: this.generateCompetencies(specialization),
        experiencePoints: 0,
        knowledgeGraph: this.generateKnowledgeGraph(specialization)
      },

      // Behavior system
      behavior: {
        learningStyle: this.inferLearningStyle(personality),
        teachingStyle: this.config.role === 'teacher' ? this.inferTeachingStyle(personality) : undefined,
        adaptationRate: personality.innovation * 0.8 + personality.analytical * 0.2,
        communicationStyle,
        decisionMakingStyle: personality.analytical > 0.6 ? 'analytical' : 'intuitive',
        riskTolerance: personality.innovation * 0.7 + personality.competitiveness * 0.3,
        collaborationPreference: personality.helpfulness * 0.6 + (1 - personality.competitiveness) * 0.4
      },

      // Evolution system
      evolution: {
        generation: this.config.generation || 0,
        parentGenomes: [],
        mutationHistory: [],
        evolutionStage: 'spawning',
        fitnessScore: this.config.initialFitness || 0.5,
        adaptationSuccess: 0,
        survivalRounds: 0,
        evolutionPressure: []
      },

      // Substrate system
      substrate: {
        loraIds: this.generateLoraIds(specialization),
        memoryPatterns: ['working_memory', 'episodic_memory'],
        processingStyle: personality.analytical > 0.6 ? 'sequential' : 'parallel',
        adaptationMechanisms: ['reinforcement_learning', 'self_reflection'],
        vectorPosition: this.generateVectorPosition()
      },

      // Reproduction system
      reproduction: {
        mutationRate: this.config.mutationRate || 0.1,
        reproductionEligibility: true,
        breedingSuccess: 0,
        offspringCount: 0
      },

      // Lineage system
      lineage: {
        ancestors: [],
        descendants: [],
        siblings: [],
        generation: this.config.generation || 0,
        lineageStrength: 0.5,
        emergentTraits: []
      }
    };

    return genome;
  }

  // ==================== INFERENCE METHODS ====================

  /**
   * Infer specialization from persona content
   */
  private inferSpecialization(personaBase: PersonaBase): string {
    if (this.config.specialization) {
      return this.config.specialization;
    }

    const prompt = personaBase.prompt.toLowerCase();
    const metadata = personaBase.metadata;

    // Check for explicit specialization in metadata
    if (metadata?.specialization) {
      return metadata.specialization;
    }

    // Infer from prompt content
    const specializationKeywords = {
      'typescript': ['typescript', 'ts', 'javascript', 'js', 'node', 'npm'],
      'python': ['python', 'py', 'pandas', 'numpy', 'django', 'flask'],
      'react': ['react', 'jsx', 'redux', 'component', 'hook'],
      'ui_ux': ['ui', 'ux', 'design', 'interface', 'user experience'],
      'data_science': ['data', 'science', 'analytics', 'machine learning', 'ml'],
      'creative_writing': ['write', 'story', 'creative', 'narrative', 'character'],
      'research': ['research', 'analyze', 'study', 'investigate', 'academic'],
      'teaching': ['teach', 'educate', 'instruct', 'mentor', 'tutor'],
      'general': ['help', 'assist', 'general', 'support', 'question']
    };

    for (const [spec, keywords] of Object.entries(specializationKeywords)) {
      if (keywords.some(keyword => prompt.includes(keyword))) {
        return spec;
      }
    }

    return 'general';
  }

  /**
   * Infer personality traits from persona content
   */
  private inferPersonality(personaBase: PersonaBase): PersonalityTraits {
    const prompt = personaBase.prompt.toLowerCase();
    const defaults = this.config.personalityDefaults || {};

    // Analyze prompt for personality indicators
    const traits = {
      creativity: this.analyzeTraitInPrompt(prompt, ['creative', 'innovative', 'imaginative', 'original'], defaults.creativity || 0.5),
      analytical: this.analyzeTraitInPrompt(prompt, ['analytical', 'logical', 'systematic', 'methodical'], defaults.analytical || 0.6),
      helpfulness: this.analyzeTraitInPrompt(prompt, ['helpful', 'supportive', 'assist', 'guide'], defaults.helpfulness || 0.7),
      competitiveness: this.analyzeTraitInPrompt(prompt, ['competitive', 'challenge', 'excel', 'win'], defaults.competitiveness || 0.4),
      patience: this.analyzeTraitInPrompt(prompt, ['patient', 'understanding', 'calm', 'gentle'], defaults.patience || 0.6),
      innovation: this.analyzeTraitInPrompt(prompt, ['innovative', 'novel', 'breakthrough', 'pioneer'], defaults.innovation || 0.5)
    };

    return traits;
  }

  /**
   * Analyze trait presence in prompt
   */
  private analyzeTraitInPrompt(prompt: string, keywords: string[], defaultValue: number): number {
    const matches = keywords.filter(keyword => prompt.includes(keyword)).length;
    if (matches === 0) return defaultValue;
    
    // Boost based on keyword matches
    const boost = Math.min(matches * 0.2, 0.4);
    return Math.min(1.0, defaultValue + boost);
  }

  /**
   * Infer communication style
   */
  private inferCommunicationStyle(personaBase: PersonaBase): string {
    const prompt = personaBase.prompt.toLowerCase();
    const mapping = this.config.communicationStyleMapping || {};

    // Check for explicit style indicators
    const styleKeywords = {
      'professional': ['professional', 'formal', 'business'],
      'casual': ['casual', 'friendly', 'relaxed'],
      'academic': ['academic', 'scholarly', 'research'],
      'technical': ['technical', 'precise', 'detailed'],
      'creative': ['creative', 'artistic', 'expressive'],
      'supportive': ['supportive', 'encouraging', 'nurturing'],
      'direct': ['direct', 'straightforward', 'clear'],
      'diplomatic': ['diplomatic', 'tactful', 'considerate']
    };

    for (const [style, keywords] of Object.entries(styleKeywords)) {
      if (keywords.some(keyword => prompt.includes(keyword))) {
        return mapping[style as CommunicationStyle] || style;
      }
    }

    return 'professional';
  }

  // ==================== GENERATION METHODS ====================

  /**
   * Generate name from prompt
   */
  private generateNameFromPrompt(prompt: string): string {
    const words = prompt.split(' ').slice(0, 10);
    const relevantWords = words.filter(word => 
      word.length > 3 && 
      !['you', 'are', 'the', 'and', 'with', 'that', 'will', 'help'].includes(word.toLowerCase())
    );

    if (relevantWords.length > 0) {
      return `${relevantWords[0].charAt(0).toUpperCase()}${relevantWords[0].slice(1)} Persona`;
    }

    return `Imported Persona ${Date.now()}`;
  }

  /**
   * Generate goals based on specialization and role
   */
  private generateGoals(specialization: string, role: PersonaRole): string[] {
    const baseGoals = [`master_${specialization}`, 'continuous_learning'];
    
    if (role === 'teacher') {
      baseGoals.push('effective_teaching', 'student_success');
    } else if (role === 'student') {
      baseGoals.push('knowledge_acquisition', 'skill_development');
    }

    return baseGoals;
  }

  /**
   * Generate competencies
   */
  private generateCompetencies(specialization: string): Record<string, number> {
    const competencies: Record<string, number> = {
      [specialization]: 0.6,
      'general': 0.4,
      'communication': 0.5,
      'problem_solving': 0.5
    };

    return competencies;
  }

  /**
   * Generate knowledge graph
   */
  private generateKnowledgeGraph(specialization: string): Record<string, string[]> {
    const graph: Record<string, string[]> = {
      [specialization]: ['fundamentals', 'advanced_concepts', 'best_practices'],
      'learning': ['methods', 'strategies', 'reflection'],
      'communication': ['expression', 'listening', 'feedback']
    };

    return graph;
  }

  /**
   * Generate learning style
   */
  private inferLearningStyle(personality: PersonalityTraits): string {
    if (personality.analytical > 0.7) return 'analytical';
    if (personality.creativity > 0.7) return 'creative';
    if (personality.helpfulness > 0.7) return 'collaborative';
    return 'adaptive';
  }

  /**
   * Generate teaching style
   */
  private inferTeachingStyle(personality: PersonalityTraits): string {
    if (personality.patience > 0.7) return 'supportive';
    if (personality.analytical > 0.7) return 'structured';
    if (personality.creativity > 0.7) return 'innovative';
    return 'balanced';
  }

  /**
   * Generate LoRA IDs
   */
  private generateLoraIds(specialization: string): string[] {
    return [
      `${specialization}_lora`,
      'base_reasoning_lora',
      'communication_lora'
    ];
  }

  /**
   * Generate vector position
   */
  private generateVectorPosition(): number[] {
    return Array.from({ length: 128 }, () => Math.random() * 2 - 1);
  }
}

// ==================== CONVENIENCE FUNCTIONS ====================

/**
 * Quick import from prompt
 */
export async function importPersonaFromPrompt(
  prompt: string, 
  name?: string, 
  config?: PersonaImportConfig
): Promise<PersonaGenome> {
  const importer = new PersonaImporter(config);
  return importer.importFromPrompt(prompt, name);
}

/**
 * Quick import from PersonaBase
 */
export async function importPersonaFromBase(
  personaBase: PersonaBase, 
  config?: PersonaImportConfig
): Promise<PersonaGenome> {
  const importer = new PersonaImporter(config);
  return importer.importFromBase(personaBase);
}

/**
 * Quick import from template
 */
export async function importPersonaFromTemplate(
  templateName: keyof typeof PersonaTemplates, 
  customName?: string, 
  config?: PersonaImportConfig
): Promise<PersonaGenome> {
  const importer = new PersonaImporter(config);
  return importer.importFromTemplate(templateName, customName);
}

/**
 * Demo: Import a complete teacher-student pair
 */
export async function importTeacherStudentPair(
  subject: string,
  config?: PersonaImportConfig
): Promise<{ teacher: PersonaGenome; student: PersonaGenome }> {
  const importer = new PersonaImporter(config);
  
  const teacher = await importer.importFromTemplate('teacher', `${subject} Teacher`);
  const student = await importer.importFromTemplate('student', `${subject} Student`);
  
  return { teacher, student };
}

// ==================== EXPORTS ====================

export {
  PersonaImportConfig,
  PersonaImporter,
  DEFAULT_IMPORT_CONFIG
};