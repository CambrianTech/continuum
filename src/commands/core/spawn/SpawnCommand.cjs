/**
 * Spawn Command - Create Fresh Agent Observers using Academy System
 * Uses existing persona/academy infrastructure instead of broken tmux/exec
 */

const Academy = require('../../../core/Academy.cjs');
const { ModelRegistry } = require('../../../core/AIModel.cjs');
const ModelCaliber = require('../../../core/ModelCaliber.cjs');
const fs = require('fs');
const path = require('path');

class SpawnCommand {
  static getDefinition() {
    return {
      name: 'SPAWN',
      description: 'Spawn fresh agent observer persona for system testing and observation',
      params: '<agent_name> [mission_type]',
      examples: [
        'observer-agent-001 system_exploration',
        'fresh-tester dashboard_testing',
        'ui-explorer interface_analysis'
      ],
      category: 'Academy',
      icon: '🤖'
    };
  }

  static async execute(params, continuum) {
    try {
      console.log('🤖 SPAWN: Creating fresh agent observer...');
      
      // Parse parameters
      const [agentName, missionType = 'system_exploration'] = params.trim().split(/\s+/);
      
      if (!agentName) {
        return {
          executed: false,
          error: 'Agent name required. Usage: SPAWN <agent_name> [mission_type]',
          usage: 'SPAWN observer-agent-001 system_exploration'
        };
      }

      // Initialize Academy system
      const modelRegistry = new ModelRegistry();
      const modelCaliber = new ModelCaliber();
      const academy = new Academy(modelRegistry, modelCaliber, continuum.commandProcessor);

      // Create fresh observer persona with mission-specific specialization
      const observerPersona = await academy.enrollRecruit(
        agentName,
        'claude-3-haiku-20240307',
        this.getMissionSpecialization(missionType)
      );

      // Set up observer-specific configuration
      observerPersona.mission = {
        type: missionType,
        objective: this.getMissionObjective(missionType),
        spawnedAt: new Date().toISOString(),
        observerMode: true,
        noIntervention: true
      };

      // Save as observer persona (without full academy training)
      await this.saveObserverPersona(observerPersona, agentName);

      // Create observation context file
      await this.createObservationContext(agentName, missionType, observerPersona);

      // Notify UI via WebSocket for SavedPersonas widget update
      if (continuum.webSocketServer) {
        continuum.webSocketServer.broadcast({
          type: 'persona_added',
          data: {
            persona: observerPersona,
            category: 'observer',
            source: 'spawn_command'
          }
        });
      }

      const result = {
        executed: true,
        message: `🤖 Fresh agent observer "${agentName}" spawned for ${missionType}`,
        result: {
          agentName,
          missionType,
          personaId: observerPersona.name,
          personaPath: `.continuum/personas/${agentName}`,
          observationFile: `.continuum/observations/${agentName}_session.md`,
          uiAccess: 'Available in SavedPersonas widget',
          nextSteps: [
            'Agent appears in SavedPersonas widget',
            'Agent can be deployed for observation missions',
            'Use academy system for additional training if needed'
          ]
        }
      };

      console.log('✅ SPAWN: Agent observer created successfully');
      return result;

    } catch (error) {
      console.error('❌ SPAWN: Failed to create agent observer:', error);
      return {
        executed: false,
        error: `Failed to spawn agent: ${error.message}`,
        stack: error.stack
      };
    }
  }

  static getMissionSpecialization(missionType) {
    const specializations = {
      'system_exploration': 'system_analysis',
      'dashboard_testing': 'ui_testing',
      'interface_analysis': 'ux_evaluation',
      'command_testing': 'command_validation',
      'bug_hunting': 'bug_detection',
      'documentation_review': 'documentation_analysis',
      'workflow_testing': 'workflow_validation'
    };

    return specializations[missionType] || 'general_observation';
  }

  static getMissionObjective(missionType) {
    const objectives = {
      'system_exploration': 'Explore the system and document natural discovery process without intervention',
      'dashboard_testing': 'Test dashboard usability and identify confusion points in UI/UX',
      'interface_analysis': 'Analyze interface design and suggest improvements',
      'command_testing': 'Test commands and document which ones work vs break',
      'bug_hunting': 'Search for bugs and document system issues found',
      'documentation_review': 'Review documentation for clarity and completeness',
      'workflow_testing': 'Test common workflows and identify pain points'
    };

    return objectives[missionType] || 'General system observation and documentation';
  }

  static async saveObserverPersona(observerPersona, agentName) {
    const personaDir = path.join('.continuum', 'personas', agentName);
    
    // Create persona directory
    if (!fs.existsSync(personaDir)) {
      fs.mkdirSync(personaDir, { recursive: true });
    }

    // Create observer-specific persona config
    const personaConfig = {
      metadata: {
        id: agentName,
        name: agentName,
        version: '1.0.0',
        specialty: observerPersona.specialization,
        description: `Fresh agent observer for ${observerPersona.mission.type}`,
        type: 'observer',
        mission: observerPersona.mission,
        spawnedAt: observerPersona.mission.spawnedAt
      },
      model: {
        baseModel: observerPersona.baseModel,
        fineTuneId: null, // Observers start untrained for fresh perspective
        checkpointPath: `${personaDir}/checkpoint.json`,
        observationPath: `${personaDir}/observations.jsonl`
      },
      performance: {
        academyScore: 0, // Fresh observer, no training yet
        observationSessions: 0,
        specializations: [observerPersona.specialization]
      },
      status: 'ready_for_observation',
      observer: {
        noIntervention: true,
        missionType: observerPersona.mission.type,
        objective: observerPersona.mission.objective,
        freshPerspective: true
      }
    };

    // Save config
    const configPath = path.join(personaDir, 'config.json');
    fs.writeFileSync(configPath, JSON.stringify(personaConfig, null, 2));

    // Create placeholder checkpoint for UI compatibility
    const checkpoint = {
      modelId: agentName,
      baseModel: observerPersona.baseModel,
      fineTuneId: null,
      trainingMetrics: {
        finalAccuracy: 0,
        observationSessions: 0,
        totalObservations: 0
      },
      observer: {
        freshPerspective: true,
        noTraining: 'Preserves natural discovery process',
        missionFocused: true
      },
      deployment: {
        compatible_frameworks: ['continuum_observer'],
        api_endpoints: [`/api/persona/${agentName}`],
        deployment_ready: true,
        observer_mode: true
      },
      created: new Date().toISOString(),
      version: '1.0.0'
    };

    const checkpointPath = path.join(personaDir, 'checkpoint.json');
    fs.writeFileSync(checkpointPath, JSON.stringify(checkpoint, null, 2));

    console.log(`💾 Observer persona saved: ${configPath}`);
  }

  static async createObservationContext(agentName, missionType, observerPersona) {
    const observationsDir = path.join('.continuum', 'observations');
    
    if (!fs.existsSync(observationsDir)) {
      fs.mkdirSync(observationsDir, { recursive: true });
    }

    const contextFile = path.join(observationsDir, `${agentName}_session.md`);
    
    const contextContent = `# 🤖 Agent Observer Session: ${agentName}

## Mission Brief
- **Agent**: ${agentName}
- **Mission Type**: ${missionType}
- **Objective**: ${observerPersona.mission.objective}
- **Spawned**: ${observerPersona.mission.spawnedAt}
- **Fresh Perspective**: No prior training or context

## Instructions for Agent
You are a fresh AI agent observer. Your mission is to explore this system and document your natural discovery process.

**CRITICAL**: No human intervention - explore organically and document everything you try.

### Your Mission: ${observerPersona.mission.objective}

### What to Document:
1. **First Impressions**: What you try first and why
2. **Natural Priorities**: What seems most important to fix/explore
3. **Confusion Points**: Where you get stuck or unclear
4. **Discovery Process**: Commands/workflows you naturally develop
5. **Improvement Suggestions**: What would help future agents

### Available Tools:
- Continuum command system (try \`--help\`, \`--dashboard\`)
- File system exploration
- Python client (\`python-client/ai-portal.py\`)
- Academy system for training
- SavedPersonas widget in UI

### Success Metrics:
Rate your effectiveness (1-10) and document your biggest barriers.

---

## Session Log
*Document your discovery process below...*

`;

    fs.writeFileSync(contextFile, contextContent);
    console.log(`📋 Observation context created: ${contextFile}`);
  }
}

module.exports = SpawnCommand;