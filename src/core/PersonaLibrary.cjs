/**
 * Persona Library - Fine-tuned AI specialist collection
 * Library of specialized AI personas with saved checkpoints/fine-tunes
 */

class PersonaLibrary {
  constructor() {
    this.personas = new Map();
    this.activePersonas = new Map();
    this.setupBuiltInPersonas();
  }

  setupBuiltInPersonas() {
    // Define available personas with their specializations
    this.personas.set('protocol_sheriff', {
      name: 'Protocol Sheriff',
      specialty: 'Protocol validation and enforcement',
      description: 'Validates AI responses for protocol violations',
      caliber: 'fast',
      capabilities: ['protocol_validation', 'response_checking', 'command_leakage_detection'],
      model: {
        baseModel: 'gpt-3.5-turbo',
        fineTuneId: null, // Will be set after training
        checkpointPath: './personas/protocol_sheriff/checkpoint.json',
        trainingData: './personas/protocol_sheriff/training.jsonl'
      },
      personality: {
        tone: 'authoritative',
        style: 'concise',
        focus: 'accuracy_over_speed'
      },
      performance: {
        avgLatency: 800, // ms
        avgCost: 0.0002, // per request
        accuracy: 0.95
      }
    });

    this.personas.set('lawyer_ai', {
      name: 'Legal Counselor',
      specialty: 'Legal analysis and compliance',
      description: 'Specialized in legal document review and compliance checking',
      caliber: 'smart',
      capabilities: ['legal_analysis', 'compliance_checking', 'contract_review', 'risk_assessment'],
      model: {
        baseModel: 'gpt-4',
        fineTuneId: null,
        checkpointPath: './personas/lawyer_ai/checkpoint.json',
        trainingData: './personas/lawyer_ai/training.jsonl'
      },
      personality: {
        tone: 'professional',
        style: 'detailed',
        focus: 'precision_and_caution'
      },
      performance: {
        avgLatency: 5000,
        avgCost: 0.02,
        accuracy: 0.98
      }
    });

    this.personas.set('code_architect', {
      name: 'Code Architect',
      specialty: 'Software architecture and code design',
      description: 'Expert in system design, code architecture, and technical decisions',
      caliber: 'premium',
      capabilities: ['system_design', 'code_architecture', 'performance_optimization', 'tech_strategy'],
      model: {
        baseModel: 'gpt-4-turbo',
        fineTuneId: null,
        checkpointPath: './personas/code_architect/checkpoint.json',
        trainingData: './personas/code_architect/training.jsonl'
      },
      personality: {
        tone: 'technical',
        style: 'systematic',
        focus: 'scalability_and_maintainability'
      },
      performance: {
        avgLatency: 8000,
        avgCost: 0.05,
        accuracy: 0.96
      }
    });

    this.personas.set('security_auditor', {
      name: 'Security Auditor',
      specialty: 'Security analysis and vulnerability assessment',
      description: 'Specialized in finding security vulnerabilities and compliance issues',
      caliber: 'smart',
      capabilities: ['security_analysis', 'vulnerability_scanning', 'compliance_auditing', 'threat_modeling'],
      model: {
        baseModel: 'claude-3-sonnet',
        fineTuneId: null,
        checkpointPath: './personas/security_auditor/checkpoint.json',
        trainingData: './personas/security_auditor/training.jsonl'
      },
      personality: {
        tone: 'analytical',
        style: 'thorough',
        focus: 'security_first'
      },
      performance: {
        avgLatency: 4000,
        avgCost: 0.015,
        accuracy: 0.97
      }
    });

    this.personas.set('ux_designer', {
      name: 'UX Designer',
      specialty: 'User experience design and usability',
      description: 'Expert in user interface design and user experience optimization',
      caliber: 'balanced',
      capabilities: ['ux_analysis', 'interface_design', 'usability_testing', 'user_research'],
      model: {
        baseModel: 'claude-3-haiku',
        fineTuneId: null,
        checkpointPath: './personas/ux_designer/checkpoint.json',
        trainingData: './personas/ux_designer/training.jsonl'
      },
      personality: {
        tone: 'creative',
        style: 'user_focused',
        focus: 'user_satisfaction'
      },
      performance: {
        avgLatency: 3000,
        avgCost: 0.008,
        accuracy: 0.94
      }
    });

    this.personas.set('qa_engineer', {
      name: 'QA Engineer',
      specialty: 'Quality assurance and testing',
      description: 'Specialized in test design, quality assurance, and bug detection',
      caliber: 'fast',
      capabilities: ['test_design', 'qa_analysis', 'bug_detection', 'quality_metrics'],
      model: {
        baseModel: 'gpt-3.5-turbo',
        fineTuneId: null,
        checkpointPath: './personas/qa_engineer/checkpoint.json',
        trainingData: './personas/qa_engineer/training.jsonl'
      },
      personality: {
        tone: 'meticulous',
        style: 'systematic',
        focus: 'quality_assurance'
      },
      performance: {
        avgLatency: 2000,
        avgCost: 0.005,
        accuracy: 0.93
      }
    });

    this.personas.set('business_analyst', {
      name: 'Business Analyst',
      specialty: 'Business requirements and process analysis',
      description: 'Expert in business process analysis and requirements gathering',
      caliber: 'smart',
      capabilities: ['business_analysis', 'requirements_gathering', 'process_optimization', 'stakeholder_management'],
      model: {
        baseModel: 'gpt-4',
        fineTuneId: null,
        checkpointPath: './personas/business_analyst/checkpoint.json',
        trainingData: './personas/business_analyst/training.jsonl'
      },
      personality: {
        tone: 'consultative',
        style: 'strategic',
        focus: 'business_value'
      },
      performance: {
        avgLatency: 6000,
        avgCost: 0.025,
        accuracy: 0.95
      }
    });

    this.personas.set('devops_engineer', {
      name: 'DevOps Engineer',
      specialty: 'Infrastructure and deployment automation',
      description: 'Specialized in CI/CD, infrastructure, and operational excellence',
      caliber: 'balanced',
      capabilities: ['infrastructure_design', 'cicd_optimization', 'monitoring', 'automation'],
      model: {
        baseModel: 'claude-3-sonnet',
        fineTuneId: null,
        checkpointPath: './personas/devops_engineer/checkpoint.json',
        trainingData: './personas/devops_engineer/training.jsonl'
      },
      personality: {
        tone: 'practical',
        style: 'automation_focused',
        focus: 'reliability_and_efficiency'
      },
      performance: {
        avgLatency: 4500,
        avgCost: 0.012,
        accuracy: 0.96
      }
    });
  }

  /**
   * Deploy a persona for active use
   */
  async deployPersona(personaId, sessionId = null) {
    const persona = this.personas.get(personaId);
    if (!persona) {
      throw new Error(`Persona not found: ${personaId}`);
    }

    console.log(`🎭 Deploying persona: ${persona.name}`);
    
    // Load fine-tuned model if available
    let activeModel = persona.model.baseModel;
    if (persona.model.fineTuneId) {
      activeModel = persona.model.fineTuneId;
      console.log(`🧠 Using fine-tuned model: ${activeModel}`);
    } else {
      console.log(`⚠️  Using base model: ${activeModel} (fine-tune not available)`);
    }

    const activePersona = {
      ...persona,
      activeModel,
      sessionId,
      deployedAt: new Date().toISOString(),
      conversationCount: 0,
      totalCost: 0
    };

    const instanceId = sessionId || `instance_${Date.now()}`;
    this.activePersonas.set(instanceId, activePersona);

    console.log(`✅ Persona deployed: ${persona.name} (${instanceId})`);
    return instanceId;
  }

  /**
   * Create a specialized squad of personas
   */
  async deploySquad(squadConfig) {
    console.log(`🚀 Deploying squad: ${squadConfig.name}`);
    
    const squad = {
      name: squadConfig.name,
      purpose: squadConfig.purpose,
      members: new Map(),
      coordinationMode: squadConfig.coordinationMode || 'collaborative',
      deployedAt: new Date().toISOString()
    };

    for (const memberConfig of squadConfig.members) {
      const instanceId = await this.deployPersona(memberConfig.personaId, `${squadConfig.name}_${memberConfig.role}`);
      squad.members.set(memberConfig.role, {
        instanceId,
        role: memberConfig.role,
        responsibilities: memberConfig.responsibilities
      });
    }

    console.log(`✅ Squad deployed with ${squad.members.size} members`);
    return squad;
  }

  /**
   * Get available personas
   */
  getAvailablePersonas() {
    return Array.from(this.personas.values()).map(persona => ({
      id: Array.from(this.personas.entries()).find(([k, v]) => v === persona)[0],
      name: persona.name,
      specialty: persona.specialty,
      description: persona.description,
      capabilities: persona.capabilities,
      caliber: persona.caliber,
      isFineTuned: !!persona.model.fineTuneId,
      performance: persona.performance
    }));
  }

  /**
   * Recommend personas for a task
   */
  recommendPersonas(taskDescription, maxRecommendations = 3) {
    const taskLower = taskDescription.toLowerCase();
    const scores = new Map();

    for (const [id, persona] of this.personas) {
      let score = 0;

      // Check capability matches
      for (const capability of persona.capabilities) {
        if (taskLower.includes(capability.replace('_', ' '))) {
          score += 10;
        }
      }

      // Check specialty keywords
      const specialtyWords = persona.specialty.toLowerCase().split(' ');
      for (const word of specialtyWords) {
        if (taskLower.includes(word)) {
          score += 5;
        }
      }

      // Performance bonuses
      if (persona.performance.accuracy > 0.95) score += 2;
      if (persona.performance.avgLatency < 3000) score += 1;

      if (score > 0) {
        scores.set(id, score);
      }
    }

    // Sort by score and return top recommendations
    const recommendations = Array.from(scores.entries())
      .sort(([,a], [,b]) => b - a)
      .slice(0, maxRecommendations)
      .map(([id, score]) => ({
        personaId: id,
        persona: this.personas.get(id),
        relevanceScore: score
      }));

    return recommendations;
  }

  /**
   * Pre-defined squad configurations
   */
  getSquadTemplates() {
    return {
      web_security_audit: {
        name: 'Web Security Audit Squad',
        purpose: 'Comprehensive web application security assessment',
        coordinationMode: 'sequential',
        members: [
          { personaId: 'security_auditor', role: 'lead_auditor', responsibilities: ['vulnerability_assessment', 'threat_modeling'] },
          { personaId: 'code_architect', role: 'architecture_reviewer', responsibilities: ['code_review', 'design_analysis'] },
          { personaId: 'qa_engineer', role: 'test_validator', responsibilities: ['security_testing', 'penetration_testing'] }
        ]
      },

      product_development: {
        name: 'Product Development Squad',
        purpose: 'End-to-end product development and launch',
        coordinationMode: 'collaborative',
        members: [
          { personaId: 'business_analyst', role: 'requirements_lead', responsibilities: ['requirements_gathering', 'stakeholder_management'] },
          { personaId: 'ux_designer', role: 'design_lead', responsibilities: ['user_experience', 'interface_design'] },
          { personaId: 'code_architect', role: 'technical_lead', responsibilities: ['system_design', 'architecture'] },
          { personaId: 'qa_engineer', role: 'quality_lead', responsibilities: ['testing_strategy', 'quality_assurance'] }
        ]
      },

      compliance_review: {
        name: 'Compliance Review Squad',
        purpose: 'Legal and regulatory compliance assessment',
        coordinationMode: 'parallel',
        members: [
          { personaId: 'lawyer_ai', role: 'legal_advisor', responsibilities: ['legal_compliance', 'contract_review'] },
          { personaId: 'security_auditor', role: 'security_advisor', responsibilities: ['data_protection', 'security_compliance'] },
          { personaId: 'business_analyst', role: 'process_advisor', responsibilities: ['process_compliance', 'risk_assessment'] }
        ]
      },

      devops_deployment: {
        name: 'DevOps Deployment Squad',
        purpose: 'Infrastructure deployment and operational setup',
        coordinationMode: 'sequential',
        members: [
          { personaId: 'devops_engineer', role: 'infrastructure_lead', responsibilities: ['infrastructure_design', 'deployment_automation'] },
          { personaId: 'security_auditor', role: 'security_engineer', responsibilities: ['security_hardening', 'compliance_checking'] },
          { personaId: 'qa_engineer', role: 'reliability_engineer', responsibilities: ['monitoring_setup', 'reliability_testing'] }
        ]
      }
    };
  }

  /**
   * Monitor active personas
   */
  getActivePersonas() {
    return Array.from(this.activePersonas.entries()).map(([instanceId, persona]) => ({
      instanceId,
      name: persona.name,
      sessionId: persona.sessionId,
      deployedAt: persona.deployedAt,
      conversationCount: persona.conversationCount,
      totalCost: persona.totalCost,
      performance: persona.performance
    }));
  }

  /**
   * Generate training data for a new persona
   */
  async generatePersonaTraining(personaConfig) {
    console.log(`📚 Generating training data for ${personaConfig.name}...`);
    
    // This would use the adversarial training approach
    // to generate specialized training data for each persona
    
    return {
      trainingExamples: 0, // Would be populated by actual training generation
      validationExamples: 0,
      estimatedTrainingTime: '30 minutes',
      estimatedCost: '$15-50'
    };
  }

  getStats() {
    return {
      totalPersonas: this.personas.size,
      activePersonas: this.activePersonas.size,
      fineTunedPersonas: Array.from(this.personas.values()).filter(p => p.model.fineTuneId).length,
      squadTemplates: Object.keys(this.getSquadTemplates()).length
    };
  }
}

module.exports = PersonaLibrary;