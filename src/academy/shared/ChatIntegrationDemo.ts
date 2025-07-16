/**
 * Chat Integration Demo - Universal Chat Participant System
 * 
 * This demonstrates how the universal chat participant system works:
 * - Humans, AIs, and personas all work through the same interface
 * - Smart inheritance and overrides enable seamless interoperability
 * - Any chat participant can be enhanced into a persona
 * - Any persona can be imported into the Academy for evolution
 * 
 * The beauty: Everything works through the same universal interface!
 */

import { 
  ChatParticipant,
  HumanParticipant,
  AIAssistantParticipant,
  createHumanParticipant,
  createAIAssistantParticipant,
  createClaudeParticipant,
  ParticipantRegistry
} from './ChatParticipant';

import { 
  PersonaBase,
  PersonaTemplates,
  createPersona,
  createTypedPersona
} from './PersonaBase';

import { 
  PersonaImporter,
  importPersonaFromPrompt,
  importPersonaFromBase,
  importPersonaFromTemplate
} from './PersonaImporter';

import { PersonaGenome } from './AcademyTypes';

// ==================== DEMONSTRATION ====================

/**
 * Comprehensive demonstration of the universal chat system
 */
export class ChatIntegrationDemo {
  private registry: ParticipantRegistry;
  private importer: PersonaImporter;

  constructor() {
    this.registry = new ParticipantRegistry();
    this.importer = new PersonaImporter();
  }

  /**
   * Demo 1: Setting up diverse chat participants
   */
  async setupChatParticipants(): Promise<void> {
    console.log('🚀 Demo 1: Setting up diverse chat participants');
    
    // 1. Create human participant (the user)
    const human = createHumanParticipant('Joel', {
      role: 'user',
      preferences: {
        communicationStyle: 'direct',
        topics: ['ai', 'programming', 'architecture'],
        expertise: ['typescript', 'system_design', 'ai_evolution'],
        language: 'en'
      }
    });
    
    console.log('👤 Human participant created:', {
      id: human.id,
      name: human.name,
      type: human.type,
      preferences: human.preferences
    });

    // 2. Create AI assistant participant (Claude)
    const claude = createClaudeParticipant();
    
    console.log('🤖 AI assistant participant created:', {
      id: claude.id,
      name: claude.name,
      type: claude.type,
      model: claude.model,
      capabilities: claude.capabilities
    });

    // 3. Create custom persona from prompt
    const customPersona = createPersona({
      name: 'TypeScript Expert',
      prompt: 'You are a TypeScript expert who helps with type systems, modern patterns, and best practices. You write clean, type-safe code.',
      description: 'Specialized TypeScript development assistant',
      type: 'specialist',
      communicationStyle: 'technical',
      capabilities: ['typescript', 'type_systems', 'code_review']
    });

    console.log('🎭 Custom persona created:', {
      id: customPersona.id,
      name: customPersona.name,
      type: customPersona.type,
      prompt: customPersona.prompt.substring(0, 50) + '...'
    });

    // 4. Create persona from template
    const teacherPersona = PersonaTemplates.teacher('React Teacher', 'react');
    
    console.log('🎓 Teacher persona from template:', {
      id: teacherPersona.id,
      name: teacherPersona.name,
      type: teacherPersona.type,
      prompt: teacherPersona.prompt.substring(0, 50) + '...'
    });

    // Register all participants
    this.registry.register(human);
    this.registry.register(claude);
    this.registry.register(customPersona);
    this.registry.register(teacherPersona);

    console.log(`✅ All participants registered! Total: ${this.registry.count()}`);
  }

  /**
   * Demo 2: Seamless chat interaction between different participant types
   */
  async demonstrateUniversalInterface(): Promise<void> {
    console.log('\n🔄 Demo 2: Universal interface demonstration');
    
    // Get all participants
    const allParticipants = this.registry.getAll();
    
    console.log('📋 All participants can use the same interface:');
    allParticipants.forEach(participant => {
      console.log(`- ${participant.name} (${participant.type}): ${participant.canCommunicate ? '✅ Can communicate' : '❌ Cannot communicate'}`);
    });

    // Demonstrate polymorphism - same interface, different implementations
    console.log('\n🎭 Polymorphic behavior demonstration:');
    
    const human = this.registry.getByType('human')[0] as HumanParticipant;
    const ai = this.registry.getByType('ai_assistant')[0] as AIAssistantParticipant;
    const persona = this.registry.getByType('persona')[0] as PersonaBase;

    // All use the same base interface but have different capabilities
    console.log('Human participant:', {
      name: human.name,
      hasPreferences: !!human.preferences,
      communicationStyle: human.preferences?.communicationStyle
    });

    console.log('AI participant:', {
      name: ai.name,
      hasCapabilities: !!ai.capabilities,
      model: ai.model
    });

    console.log('Persona participant:', {
      name: persona.name,
      hasPrompt: !!persona.prompt,
      hasRAG: !!persona.rag
    });
  }

  /**
   * Demo 3: Converting any chat participant into a persona
   */
  async demonstrateParticipantToPersona(): Promise<void> {
    console.log('\n🔄 Demo 3: Converting chat participants to personas');
    
    // Get the human participant
    const human = this.registry.getByType('human')[0] as HumanParticipant;
    
    // Convert human to persona (for representation in Academy)
    const humanPersona = createPersona({
      name: `${human.name} (Human Persona)`,
      prompt: `You represent ${human.name}, a human with expertise in ${human.preferences?.expertise?.join(', ')}. You communicate in a ${human.preferences?.communicationStyle} style.`,
      description: `Persona representation of human user ${human.name}`,
      type: 'assistant',
      communicationStyle: human.preferences?.communicationStyle as any || 'professional',
      capabilities: human.preferences?.expertise || ['general']
    });

    console.log('👤➡️🎭 Human converted to persona:', {
      originalType: human.type,
      personaType: humanPersona.type,
      name: humanPersona.name,
      prompt: humanPersona.prompt.substring(0, 80) + '...'
    });

    // Get the AI participant
    const ai = this.registry.getByType('ai_assistant')[0] as AIAssistantParticipant;
    
    // Convert AI to persona (for specialization)
    const aiPersona = createPersona({
      name: `${ai.name} (Specialized)`,
      prompt: `You are ${ai.name}, specialized for TypeScript development. You leverage your ${ai.capabilities?.join(', ')} capabilities to provide expert TypeScript guidance.`,
      description: `Specialized version of ${ai.name} for TypeScript development`,
      type: 'specialist',
      communicationStyle: 'technical',
      capabilities: ['typescript', 'programming', 'analysis']
    });

    console.log('🤖➡️🎭 AI converted to specialized persona:', {
      originalType: ai.type,
      personaType: aiPersona.type,
      name: aiPersona.name,
      specialization: 'typescript'
    });

    // Register the new personas
    this.registry.register(humanPersona);
    this.registry.register(aiPersona);
  }

  /**
   * Demo 4: Importing personas into Academy for evolution
   */
  async demonstrateAcademyImport(): Promise<void> {
    console.log('\n🎓 Demo 4: Importing personas into Academy');
    
    // Get some personas to import
    const personas = this.registry.getByType('persona') as PersonaBase[];
    
    console.log(`📥 Importing ${personas.length} personas into Academy...`);
    
    const genomicPersonas: PersonaGenome[] = [];
    
    for (const persona of personas.slice(0, 3)) { // Import first 3
      try {
        // Import persona into Academy (enhance to PersonaGenome)
        const genomicPersona = await this.importer.importFromBase(persona);
        genomicPersonas.push(genomicPersona);
        
        console.log(`✅ Imported ${persona.name}:`, {
          originalType: 'PersonaBase',
          enhancedType: 'PersonaGenome',
          specialization: genomicPersona.identity.specialization,
          role: genomicPersona.identity.role,
          fitnessScore: genomicPersona.evolution.fitnessScore
        });
      } catch (error) {
        console.error(`❌ Failed to import ${persona.name}:`, error);
      }
    }

    console.log(`🧬 Academy now has ${genomicPersonas.length} genomic personas ready for evolution!`);
    
    // Demonstrate genomic features
    if (genomicPersonas.length > 0) {
      const sample = genomicPersonas[0];
      console.log('\n🧬 Sample genomic persona features:', {
        name: sample.name,
        basePrompt: sample.prompt.substring(0, 50) + '...',
        personality: sample.identity.personality,
        competencies: sample.knowledge.competencies,
        evolutionStage: sample.evolution.evolutionStage,
        generation: sample.evolution.generation
      });
    }
  }

  /**
   * Demo 5: Creating personas from simple prompts
   */
  async demonstratePromptImport(): Promise<void> {
    console.log('\n💬 Demo 5: Creating personas from simple prompts');
    
    // Simple prompt strings that can become personas
    const prompts = [
      "You are a helpful Python developer who loves clean code and testing.",
      "You are a creative writer who specializes in science fiction and worldbuilding.",
      "You are a patient teacher who explains complex concepts in simple terms."
    ];

    console.log('📝 Converting simple prompts to Academy-ready personas...');
    
    for (const [index, prompt] of prompts.entries()) {
      try {
        // Direct prompt to genomic persona
        const genomicPersona = await importPersonaFromPrompt(prompt, `Prompt Persona ${index + 1}`);
        
        console.log(`✅ Created from prompt ${index + 1}:`, {
          name: genomicPersona.name,
          specialization: genomicPersona.identity.specialization,
          role: genomicPersona.identity.role,
          originalPrompt: prompt.substring(0, 50) + '...'
        });
      } catch (error) {
        console.error(`❌ Failed to create from prompt ${index + 1}:`, error);
      }
    }
  }

  /**
   * Demo 6: Template-based persona creation
   */
  async demonstrateTemplateImport(): Promise<void> {
    console.log('\n📋 Demo 6: Template-based persona creation');
    
    // Create teacher-student pairs for different subjects
    const subjects = ['typescript', 'python', 'react'];
    
    for (const subject of subjects) {
      try {
        // Import teacher from template
        const teacher = await importPersonaFromTemplate('teacher', `${subject} Teacher`);
        const student = await importPersonaFromTemplate('student', `${subject} Student`);
        
        console.log(`🎓 Created ${subject} pair:`, {
          teacher: {
            name: teacher.name,
            role: teacher.identity.role,
            specialization: teacher.identity.specialization
          },
          student: {
            name: student.name,
            role: student.identity.role,
            specialization: student.identity.specialization
          }
        });
      } catch (error) {
        console.error(`❌ Failed to create ${subject} pair:`, error);
      }
    }
  }

  /**
   * Run the complete demonstration
   */
  async runCompleteDemo(): Promise<void> {
    console.log('🎪 Universal Chat Participant System Demo');
    console.log('==========================================');
    
    await this.setupChatParticipants();
    await this.demonstrateUniversalInterface();
    await this.demonstrateParticipantToPersona();
    await this.demonstrateAcademyImport();
    await this.demonstratePromptImport();
    await this.demonstrateTemplateImport();
    
    console.log('\n🎉 Demo complete! Key takeaways:');
    console.log('✅ Universal interface works for humans, AIs, and personas');
    console.log('✅ Smart inheritance enables seamless interoperability');
    console.log('✅ Any chat participant can become a persona');
    console.log('✅ Any persona can be imported into the Academy');
    console.log('✅ Simple prompts can become Academy-ready genomic personas');
    console.log('✅ Templates provide quick persona creation');
    console.log('✅ Everything works through the same modular system!');
  }
}

// ==================== USAGE EXAMPLES ====================

/**
 * Example 1: Quick persona creation and Academy import
 */
export async function quickPersonaDemo(): Promise<void> {
  console.log('🚀 Quick Persona Demo');
  
  // Create a persona from a simple prompt
  const persona = await importPersonaFromPrompt(
    "You are a helpful coding assistant who specializes in JavaScript and enjoys teaching.",
    "JS Helper"
  );
  
  console.log('Created persona:', {
    name: persona.name,
    type: persona.type,
    specialization: persona.identity.specialization,
    role: persona.identity.role,
    canEvolvve: !!persona.evolution
  });
}

/**
 * Example 2: Human-AI-Persona integration
 */
export async function integrationExample(): Promise<void> {
  console.log('🔄 Integration Example');
  
  // Create human representation
  const human = createHumanParticipant('Developer', {
    preferences: {
      expertise: ['typescript', 'react'],
      communicationStyle: 'direct'
    }
  });
  
  // Create AI assistant
  const ai = createClaudeParticipant();
  
  // Create custom persona
  const persona = createPersona({
    name: 'Code Reviewer',
    prompt: 'You review code for best practices, security, and performance.',
    type: 'specialist',
    communicationStyle: 'technical'
  });
  
  // All can participate in the same chat system
  const participants = [human, ai, persona];
  
  console.log('All participants ready for chat:', 
    participants.map(p => ({
      name: p.name,
      type: p.type,
      canCommunicate: p.canCommunicate
    }))
  );
}

/**
 * Example 3: Academy evolution pipeline
 */
export async function evolutionPipelineExample(): Promise<void> {
  console.log('🧬 Evolution Pipeline Example');
  
  // Start with a simple prompt
  const prompt = "You are a beginner-friendly programming tutor.";
  
  // Convert to genomic persona
  const genomicPersona = await importPersonaFromPrompt(prompt, "Tutor Bot");
  
  // Now it's ready for Academy evolution
  console.log('Ready for evolution:', {
    name: genomicPersona.name,
    hasEvolution: !!genomicPersona.evolution,
    generation: genomicPersona.evolution.generation,
    fitnessScore: genomicPersona.evolution.fitnessScore,
    canMutate: genomicPersona.reproduction.reproductionEligibility
  });
}

// ==================== EXPORTS ====================

export {
  ChatIntegrationDemo,
  quickPersonaDemo,
  integrationExample,
  evolutionPipelineExample
};