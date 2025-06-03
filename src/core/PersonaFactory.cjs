/**
 * Persona Factory - AI-driven persona construction and deployment
 * Creates specialized AI personas on demand and trains them through Academy
 */

const Academy = require('./Academy.cjs');

class PersonaFactory {
  constructor(modelRegistry, modelCaliber) {
    this.modelRegistry = modelRegistry;
    this.modelCaliber = modelCaliber;
    this.academy = new Academy(modelRegistry, modelCaliber);
    this.constructionQueue = [];
    this.activePersonas = new Map();
  }

  /**
   * AI-driven persona construction based on requirements
   */
  async constructPersona(requirements) {
    console.log(`🏭 Persona Factory: Constructing new persona for requirements:`);
    console.log(`   📋 Task: ${requirements.task}`);
    console.log(`   🎯 Performance: ${requirements.requiredAccuracy || 85}%`);
    console.log(`   ⚡ Speed: ${requirements.maxLatency || 'standard'}`);
    console.log(`   💰 Budget: ${requirements.maxCost || 'balanced'}`);

    // AI-driven persona design
    const personaSpec = await this.designPersona(requirements);
    console.log(`\\n🧠 AI Designer recommends: ${personaSpec.name}`);
    console.log(`   🎭 Specialization: ${personaSpec.specialization}`);
    console.log(`   📊 Expected Performance: ${personaSpec.expectedAccuracy}%`);

    // Send to Academy for training
    console.log(`\\n🚨 Sending ${personaSpec.name} to Academy...`);
    const trainedPersona = await this.academy.trainNewPersona(
      personaSpec.name,
      personaSpec.specialization,
      personaSpec.trainingRounds
    );

    // Performance validation
    if (trainedPersona.graduationScore >= (requirements.requiredAccuracy || 0.85)) {
      console.log(`✅ ${personaSpec.name} meets performance requirements!`);
      this.activePersonas.set(personaSpec.name, trainedPersona);
      return trainedPersona;
    } else {
      console.log(`⚠️ ${personaSpec.name} needs additional training or redesign`);
      return await this.redesignAndRetrain(personaSpec, requirements);
    }
  }

  /**
   * AI designs persona based on requirements
   */
  async designPersona(requirements) {
    // Persona naming based on task + random elements
    const taskKeywords = requirements.task.toLowerCase().split(' ');
    const nameBase = taskKeywords[0] || 'specialist';
    const suffix = this.generatePersonaSuffix();
    const name = `${nameBase}-${suffix}`;

    // Determine specialization from task
    let specialization = 'general_purpose';
    if (requirements.task.toLowerCase().includes('security')) {
      specialization = 'security_validation';
    } else if (requirements.task.toLowerCase().includes('protocol')) {
      specialization = 'protocol_enforcement';
    } else if (requirements.task.toLowerCase().includes('command')) {
      specialization = 'command_detection';
    } else if (requirements.task.toLowerCase().includes('legal')) {
      specialization = 'legal_compliance';
    } else if (requirements.task.toLowerCase().includes('code')) {
      specialization = 'code_analysis';
    }

    // Performance-based training intensity
    const targetAccuracy = requirements.requiredAccuracy || 85;
    let trainingRounds = 5; // default
    if (targetAccuracy >= 95) trainingRounds = 15;
    else if (targetAccuracy >= 90) trainingRounds = 10;

    return {
      name,
      specialization,
      expectedAccuracy: targetAccuracy,
      trainingRounds,
      baseModel: this.selectBaseModel(requirements),
      requirements
    };
  }

  /**
   * Generate random persona suffix
   */
  generatePersonaSuffix() {
    const suffixes = [
      'alpha', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot',
      'phoenix', 'ranger', 'scout', 'guardian', 'sentinel', 'vanguard',
      'prime', 'nexus', 'core', 'elite', 'expert', 'master'
    ];
    const numbers = Math.floor(Math.random() * 100);
    const suffix = suffixes[Math.floor(Math.random() * suffixes.length)];
    return `${suffix}${numbers}`;
  }

  /**
   * Select optimal base model for requirements
   */
  selectBaseModel(requirements) {
    if (requirements.maxCost === 'minimal') {
      return 'claude-3-haiku-20240307'; // Fast and cheap
    } else if (requirements.maxCost === 'premium') {
      return 'claude-3-sonnet-20240229'; // Balanced
    } else {
      return 'claude-3-haiku-20240307'; // Default to fast
    }
  }

  /**
   * Redesign and retrain if persona doesn't meet requirements
   */
  async redesignAndRetrain(originalSpec, requirements) {
    console.log(`🔄 Redesigning ${originalSpec.name} for better performance...`);
    
    // Upgrade training intensity
    const newSpec = {
      ...originalSpec,
      name: `${originalSpec.name}-v2`,
      trainingRounds: originalSpec.trainingRounds + 5,
      baseModel: this.upgradeBaseModel(originalSpec.baseModel)
    };

    console.log(`🚨 Sending improved ${newSpec.name} to Academy...`);
    return await this.academy.trainNewPersona(
      newSpec.name,
      newSpec.specialization,
      newSpec.trainingRounds
    );
  }

  /**
   * Upgrade base model for better performance
   */
  upgradeBaseModel(currentModel) {
    if (currentModel === 'claude-3-haiku-20240307') {
      return 'claude-3-sonnet-20240229';
    }
    return currentModel; // Already at max
  }

  /**
   * Deploy persona for specific task
   */
  async deployPersona(personaName, taskContext) {
    const persona = this.activePersonas.get(personaName);
    if (!persona) {
      throw new Error(`Persona ${personaName} not found. Need to construct first?`);
    }

    console.log(`🚀 Deploying ${personaName} for task: ${taskContext.task}`);
    console.log(`   🎓 Academy Score: ${(persona.graduationScore * 100).toFixed(1)}%`);
    console.log(`   🎭 Specialization: ${persona.specialization}`);

    return {
      persona,
      deployment: {
        deployedAt: new Date().toISOString(),
        task: taskContext.task,
        sessionId: `session_${Date.now()}`
      }
    };
  }

  /**
   * Construct persona squad for complex tasks
   */
  async constructSquad(squadRequirements) {
    console.log(`🚀 Persona Factory: Constructing squad for complex task`);
    console.log(`   📋 Mission: ${squadRequirements.mission}`);
    console.log(`   👥 Required Roles: ${squadRequirements.roles.join(', ')}`);

    const squad = {
      name: squadRequirements.name || `squad_${Date.now()}`,
      mission: squadRequirements.mission,
      members: new Map(),
      constructedAt: new Date().toISOString()
    };

    for (const role of squadRequirements.roles) {
      console.log(`\\n👷 Constructing ${role} specialist...`);
      
      const roleRequirements = {
        task: `${role} specialist for ${squadRequirements.mission}`,
        requiredAccuracy: squadRequirements.minAccuracy || 85,
        maxLatency: squadRequirements.maxLatency || 'standard',
        maxCost: squadRequirements.budget || 'balanced'
      };

      const persona = await this.constructPersona(roleRequirements);
      squad.members.set(role, persona);
    }

    console.log(`\\n✅ Squad construction complete!`);
    console.log(`👥 ${squad.name}: ${squad.members.size} specialized personas ready`);

    return squad;
  }

  /**
   * Performance benchmarking for personas
   */
  async benchmarkPersona(personaName, testSuite) {
    const persona = this.activePersonas.get(personaName);
    if (!persona) {
      throw new Error(`Persona ${personaName} not found`);
    }

    console.log(`🏃 Benchmarking ${personaName}...`);
    
    // Run performance tests
    const results = {
      accuracy: persona.graduationScore,
      speed: Math.random() * 1000 + 500, // Simulated latency
      cost: Math.random() * 0.01 + 0.001, // Simulated cost
      specialization: persona.specialization,
      benchmark: testSuite || 'standard',
      benchmarkedAt: new Date().toISOString()
    };

    console.log(`📊 Benchmark Results for ${personaName}:`);
    console.log(`   🎯 Accuracy: ${(results.accuracy * 100).toFixed(1)}%`);
    console.log(`   ⚡ Speed: ${results.speed.toFixed(0)}ms`);
    console.log(`   💰 Cost: $${results.cost.toFixed(4)}`);

    return results;
  }

  /**
   * Get factory statistics
   */
  getFactoryStats() {
    return {
      totalConstructed: this.activePersonas.size,
      activePersonas: Array.from(this.activePersonas.keys()),
      queueSize: this.constructionQueue.length,
      academyStats: this.academy.bootCampStats
    };
  }
}

module.exports = PersonaFactory;