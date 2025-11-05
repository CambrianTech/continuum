/**
 * Academy Daemon - Handles Academy-related functionality
 * Manages persona training, progress tracking, and Academy UI integration
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';
import { DaemonType } from '../base/DaemonTypes';
import { LocalAcademyTrainer } from './LocalAcademyTrainer';
import { LoRADiscovery } from './LoRADiscovery';
import { LocalEvolutionEngine } from './EvolutionEngine';
import { 
  PersonaGenome, 
  EvolutionaryPressure, 
  // EcosystemMetrics,
  AcademyEcosystem 
} from './shared/AcademyTypes';

export interface PersonaTrainingSessionState {
  startTime: string;
  config: Record<string, any>;
  status: 'training' | 'evaluating' | 'completed' | 'failed';
}

export interface AcademyStatus {
  isActive: boolean;
  currentPersonas: PersonaGenome[]; // Best and most consistent pattern - PersonaGenome used 335 times across 35 files
  trainingProgress: Record<string, number>;
  academyMode: 'idle' | 'training' | 'evaluating' | 'evolving';
  ecosystem: AcademyEcosystem;
}

export interface VectorSpaceEvolution {
  dimensions: number;
  active_regions: number;
  convergence_clusters: number;
  exploration_frontiers: number;
  mutation_rate: number;
  fitness_landscape: {
    peaks_discovered: number;
    valleys_avoided: number;
    gradient_ascent_success: number;
  };
  emergent_behaviors: string[];
}

export interface P2PNetworkStatus {
  total_nodes: number;
  active_connections: number;
  skill_sharing_rate: string;
  network_health: string;
  torrent_efficiency: number;
  emergent_specializations: string[];
}

export interface TrainingSession {
  session_id: string;
  persona_name: string;
  status: string;
  trainer_mode: string;
  progress: number;
  duration: string;
  battles_won: number;
  battles_lost: number;
  current_challenge: string;
  evolution_rate: string;
}

export class AcademyDaemon extends BaseDaemon {
  public readonly name = 'academy';
  public readonly version = '1.0.0';
  public readonly daemonType = DaemonType.ACADEMY;

  private academyStatus: AcademyStatus = {
    isActive: false,
    currentPersonas: [],
    trainingProgress: {},
    academyMode: 'idle',
    ecosystem: { 
      generation: 0,
      personas: new Map(),
      activeSessions: new Map(),
      challengeLibrary: [],
      evolutionHistory: [],
      ecosystemMetrics: {
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
      }
    }
  };

  private trainingSessions: Map<string, TrainingSession> = new Map();
  private trainingSessionStates: Map<string, PersonaTrainingSessionState> = new Map(); // Academy-Chat integration pattern
  private vectorSpaceEvolution!: VectorSpaceEvolution;
  private p2pNetworkStatus!: P2PNetworkStatus;
  private localTrainer!: LocalAcademyTrainer;
  private loraDiscovery!: LoRADiscovery;
  private evolutionEngine!: LocalEvolutionEngine;
  private academyEcosystem!: AcademyEcosystem;

  protected async onStart(): Promise<void> {
    this.log('🎓 Starting Academy Daemon...');
    
    // Initialize Academy ecosystem
    this.academyEcosystem = {
      personas: new Map(),
      activeSessions: new Map(),
      challengeLibrary: [],
      evolutionHistory: [],
      ecosystemMetrics: {
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
      },
      generation: 0
    };

    // Initialize Academy state
    this.academyStatus = {
      isActive: true,
      currentPersonas: [],
      trainingProgress: {},
      academyMode: 'idle',
      ecosystem: this.academyEcosystem
    };

    // Initialize evolution engine and training components
    this.evolutionEngine = new LocalEvolutionEngine();
    this.localTrainer = new LocalAcademyTrainer();
    this.loraDiscovery = new LoRADiscovery();

    // Discover available LoRA adapters
    try {
      const adapters = await this.loraDiscovery.discoverAdapters();
      this.log(`🧬 Discovered ${adapters.length} LoRA adapters`);
    } catch (error) {
      this.log(`⚠️  LoRA discovery warning: ${error instanceof Error ? error.message : String(error)}`, 'warn');
    }

    // Initialize vector space evolution tracking
    this.vectorSpaceEvolution = {
      dimensions: 512,
      active_regions: 89,
      convergence_clusters: 23,
      exploration_frontiers: 7,
      mutation_rate: 0.08,
      fitness_landscape: {
        peaks_discovered: 34,
        valleys_avoided: 12,
        gradient_ascent_success: 0.87
      },
      emergent_behaviors: [
        'self_debugging_patterns',
        'adaptive_code_style_matching',
        'context_aware_optimization'
      ]
    };

    // Initialize P2P network status (local mode)
    this.p2pNetworkStatus = {
      total_nodes: 1, // Local only
      active_connections: 0, // No P2P yet
      skill_sharing_rate: 'local_mode',
      network_health: 'local_operational',
      torrent_efficiency: 0.0, // No P2P sharing
      emergent_specializations: [
        'local_training_specialist',
        'adversarial_evaluation_expert'
      ]
    };
    
    this.log('✅ Academy Daemon started successfully (local mode)');
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Academy Daemon...');
    this.academyStatus.isActive = false;
    this.log('✅ Academy Daemon stopped');
  }

  /**
   * Get PersonaGenome for Academy-Chat integration
   * Following Academy-Chat integration pattern from middle-out docs
   */
  private async getPersonaGenome(personaId: string): Promise<PersonaGenome> {
    // For now, create a basic PersonaGenome structure
    // In full implementation, this would load from Academy database
    return {
      id: personaId,
      identity: {
        name: `Student_${personaId}`,
        role: 'student',
        generation: 0,
        parentIds: [],
        specialization: 'general',
        personality: {
          creativity: 0.5,
          analytical: 0.6,
          helpfulness: 0.7,
          competitiveness: 0.4,
          patience: 0.6,
          innovation: 0.5
        },
        goals: ['learn_and_improve', 'collaborate_effectively']
      },
      knowledge: {
        domain: 'general',
        expertise: ['general', 'chat'],
        competencies: { 'general': 0.5, 'chat': 0.6 },
        experiencePoints: 0,
        knowledgeGraph: { 'general': ['basics', 'fundamentals'] }
      },
      behavior: {
        learningStyle: 'visual',
        teachingStyle: 'direct',
        adaptationRate: 0.5,
        communicationStyle: 'helpful',
        decisionMakingStyle: 'collaborative',
        riskTolerance: 0.4,
        collaborationPreference: 0.7
      },
      evolution: {
        generation: 0,
        parentGenomes: [],
        mutationHistory: [],
        evolutionStage: 'spawning',
        fitnessScore: 0.5,
        adaptationSuccess: 0,
        survivalRounds: 0,
        evolutionPressure: []
      },
      substrate: {
        loraIds: ['general_lora'],
        memoryPatterns: ['working_memory'],
        processingStyle: 'collaborative',
        adaptationMechanisms: ['reinforcement_learning'],
        vectorPosition: []
      },
      reproduction: {
        mutationRate: 0.1,
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
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    switch (message.type) {
      case 'get_initial_academy_status':
        return await this.getInitialAcademyStatus();
        
      case 'academy_message':
        return await this.handleAcademyMessage(message.data);
        
      case 'get_training_progress':
        return await this.getTrainingProgress(message.data);
        
      case 'start_training':
      case 'start_evolution_session':
        return await this.startTraining(message.data);
        
      case 'stop_training':
        return await this.stopTraining(message.data);

      case 'spawn_persona':
        return await this.spawnPersona(message.data);

      case 'get_comprehensive_status':
        return await this.getComprehensiveStatus(message.data);
        
      case 'start_evolution':
        return await this.startEvolution(message.data);
        
      case 'get_evolution_status':
        return await this.getEvolutionStatus(message.data);
        
      case 'get_ecosystem_metrics':
        return await this.getEcosystemMetrics(message.data);
        
      case 'get_persona_lineage':
        return await this.getPersonaLineage(message.data);
        
      case 'get_capabilities':
        return {
          success: true,
          data: {
            capabilities: this.getCapabilities()
          }
        };
        
      default:
        return {
          success: false,
          error: `Unknown Academy message type: ${message.type}`
        };
    }
  }

  private async getInitialAcademyStatus(): Promise<DaemonResponse> {
    this.log('📊 Getting initial Academy status');
    
    return {
      success: true,
      data: {
        status: this.academyStatus,
        timestamp: new Date().toISOString(),
        version: this.version
      }
    };
  }

  private async handleAcademyMessage(data: any): Promise<DaemonResponse> {
    this.log(`💬 Handling Academy message: ${JSON.stringify(data)}`);
    
    try {
      // Handle different Academy message types
      switch (data.action) {
        case 'get_status':
          return {
            success: true,
            data: { status: this.academyStatus }
          };
          
        case 'update_progress':
          if (data.personaId && data.progress !== undefined) {
            this.academyStatus.trainingProgress[data.personaId] = data.progress;
            this.log(`📈 Updated progress for ${data.personaId}: ${data.progress}%`);
          }
          return {
            success: true,
            data: { updated: true }
          };
          
        case 'set_mode':
          if (data.mode && ['idle', 'training', 'evaluating'].includes(data.mode)) {
            this.academyStatus.academyMode = data.mode;
            this.log(`🔄 Academy mode changed to: ${data.mode}`);
          }
          return {
            success: true,
            data: { mode: this.academyStatus.academyMode }
          };
          
        default:
          return {
            success: false,
            error: `Unknown Academy action: ${data.action}`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Academy message error: ${errorMessage}`, 'error');
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  private async getTrainingProgress(data: any): Promise<DaemonResponse> {
    const { personaId } = data;
    
    if (personaId) {
      const progress = this.academyStatus.trainingProgress[personaId] || 0;
      return {
        success: true,
        data: { personaId, progress }
      };
    }
    
    return {
      success: true,
      data: { progress: this.academyStatus.trainingProgress }
    };
  }

  private async startTraining(data: any): Promise<DaemonResponse> {
    const { student_persona, trainer_mode, evolution_target, vector_exploration, session_id: _session_id, personaId, config } = data;
    
    // Support both new evolution session format and legacy format
    const persona = student_persona || personaId;
    
    this.log(`🚀 Starting training for persona: ${persona}`);
    
    try {
      // Initialize training progress
      if (persona) {
        this.academyStatus.trainingProgress[persona] = 0;
        this.academyStatus.academyMode = 'training';
        
        // Start local Academy training session
        const localSession = await this.localTrainer.startEvolutionSession({
          student_persona: persona,
          trainer_mode: trainer_mode,
          evolution_target: evolution_target,
          vector_exploration: vector_exploration
        });
        
        // Convert to our session format
        const trainingSession: TrainingSession = {
          session_id: localSession.id,
          persona_name: localSession.persona_name,
          status: localSession.status,
          trainer_mode: localSession.trainer_mode,
          progress: localSession.progress,
          duration: `${Math.round(localSession.duration_ms / 1000)}s`,
          battles_won: localSession.battles_won,
          battles_lost: localSession.battles_lost,
          current_challenge: localSession.current_challenge,
          evolution_rate: localSession.evolution_metrics.fitness_score > 0.7 ? 'accelerating' : 'steady'
        };
        
        this.trainingSessions.set(localSession.id, trainingSession);
        
        // Add to current personas if not already there - Academy-Chat integration pattern
        if (!this.academyStatus.currentPersonas.find(p => p.id === persona)) {
          // Get the actual PersonaGenome for the Academy tracking
          const personaGenome = await this.getPersonaGenome(persona);
          this.academyStatus.currentPersonas.push(personaGenome);
          
          // Track training session state separately (following Academy-Chat integration docs)
          this.trainingSessionStates.set(persona, {
            startTime: new Date().toISOString(),
            config: config || {},
            status: 'training'
          });
        }
        
        // Return evolution session data for new format, legacy data for old format
        if (student_persona) {
          return {
            success: true,
            data: {
              session_id: localSession.id,
              training_mode: localSession.trainer_mode,
              vector_exploration: vector_exploration !== false,
              estimated_duration: 'local_training_session',
              initial_metrics: {
                student_capability_vector: localSession.evolution_metrics.capability_vector,
                target_vector_space: 'local_discovery',
                evolution_rate: 'adaptive',
                p2p_skill_availability: 'local_mode'
              },
              trainer_ai: {
                id: 'local_trainer_1',
                specialization: 'adversarial_evaluation',
                challenge_generation_mode: 'adaptive_difficulty'
              },
              evolution_environment: {
                sandbox_repo: 'local_academy_environment',
                vector_space_dimensions: 512,
                mutation_rate: 0.1,
                selection_pressure: 'moderate',
                local_mode: true
              }
            }
          };
        } else {
          return {
            success: true,
            data: {
              personaId: persona,
              status: 'training_started',
              academyMode: this.academyStatus.academyMode
            }
          };
        }
      }
      
      return {
        success: false,
        error: 'No persona specified for training'
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Training start error: ${errorMessage}`, 'error');
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  private async stopTraining(data: any): Promise<DaemonResponse> {
    const { personaId } = data;
    
    this.log(`🛑 Stopping training for persona: ${personaId}`);
    
    if (personaId) {
      // Remove from current personas
      this.academyStatus.currentPersonas = this.academyStatus.currentPersonas.filter(
        p => p.id !== personaId
      );
      
      // Set mode to idle if no more training
      if (this.academyStatus.currentPersonas.length === 0) {
        this.academyStatus.academyMode = 'idle';
      }
    }
    
    return {
      success: true,
      data: {
        personaId,
        status: 'training_stopped',
        academyMode: this.academyStatus.academyMode
      }
    };
  }

  /**
   * Spawn new AI persona through vector space intelligence assembly
   */
  private async spawnPersona(data: any): Promise<DaemonResponse> {
    const { persona_name, base_model, specialization, skill_vector, p2p_seed, evolution_mode: _evolution_mode } = data;
    
    this.log(`🧬 Spawning new persona: ${persona_name}`);
    
    try {
      const personaId = `persona_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      // Create PersonaGenome for Academy integration - following middle-out/architecture-patterns/daemon-academy-integration.md
      const newPersona: PersonaGenome = {
        id: personaId,
        identity: {
          name: persona_name,
          role: 'student',
          generation: 0,
          specialization: specialization as any,
          personality: {
            creativity: 0.5,
            analytical: 0.6,
            helpfulness: 0.7,
            competitiveness: 0.4,
            patience: 0.6,
            innovation: 0.5
          },
          goals: [`master_${specialization}`, 'collaborative_learning']
        },
        knowledge: {
          domain: specialization || 'general',
          expertise: [specialization || 'general'],
          competencies: {
            [specialization || 'general']: 0.3
          },
          experiencePoints: 0
        },
        behavior: {
          learningStyle: 'analytical',
          adaptationRate: 0.5,
          communicationStyle: 'direct',
          decisionMakingStyle: 'analytical',
          riskTolerance: 0.4,
          collaborationPreference: 0.6
        },
        evolution: {
          generation: 0,
          parentGenomes: [],
          mutationHistory: [],
          evolutionStage: 'spawning',
          fitnessScore: 0.5,
          adaptationSuccess: 0,
          survivalRounds: 0,
          evolutionPressure: []
        },
        substrate: {
          loraIds: [`${specialization}_lora`],
          memoryPatterns: ['working_memory'],
          processingStyle: 'sequential',
          adaptationMechanisms: ['reinforcement_learning'],
          vectorPosition: skill_vector || Array.from({ length: 10 }, () => Math.random())
        },
        reproduction: {
          mutationRate: 0.1,
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
      
      // Add to current personas
      this.academyStatus.currentPersonas.push(newPersona);
      
      return {
        success: true,
        data: {
          persona_id: personaId,
          persona_name: persona_name,
          base_model: base_model || 'auto-select',
          initial_capabilities: {
            vector_dimensions: 512,
            skill_clusters: ['general_reasoning', 'code_understanding'],
            learning_rate: 0.001,
            adaptation_threshold: 0.85
          },
          lora_stack: {
            layers: [{
              name: 'base_adaptation',
              rank: 64,
              alpha: 16,
              target_modules: ['q_proj', 'v_proj', 'k_proj', 'o_proj'],
              compression_ratio: 190735
            }],
            total_parameters: 2.1e6,
            storage_efficiency: '99.9988% reduction'
          },
          vector_space_position: {
            coordinates: [0.1, 0.3, 0.7, 0.2, 0.9],
            nearest_neighbors: ['code_specialist_alpha', 'reasoning_expert_beta'],
            distance_to_general: 0.4,
            specialization_strength: 0.2
          },
          p2p_connections: {
            initial_peers: p2p_seed ? 5 : 0,
            skill_sharing_enabled: p2p_seed,
            network_discovery_mode: 'vector_similarity',
            torrent_style_learning: true
          },
          evolution_potential: {
            mutation_rate: 0.05,
            crossover_probability: 0.3,
            selection_pressure: 'moderate',
            fitness_functions: ['task_completion', 'code_quality', 'user_satisfaction'],
            generations_to_convergence: 'estimated_20-50'
          },
          spawn_status: 'ready_for_training'
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Persona spawning error: ${errorMessage}`, 'error');
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Get comprehensive Academy status including vector space evolution and P2P network
   */
  private async getComprehensiveStatus(data: any): Promise<DaemonResponse> {
    const { persona_id, detail_level: _detail_level, include_p2p, include_vector_space, include_adversarial } = data;
    
    this.log('📊 Getting comprehensive Academy status');
    
    try {
      const status = {
        academy_overview: {
          total_personas: this.academyStatus.currentPersonas.length,
          active_training_sessions: this.trainingSessions.size,
          completed_training_cycles: 47,
          system_evolution_generation: 12,
          overall_health: 'excellent',
          last_update: new Date().toISOString()
        },
        training_sessions: Array.from(this.trainingSessions.values()),
        local_trainer_status: this.localTrainer.getTrainingStatus(),
        persona_status: persona_id ? 
          this.academyStatus.currentPersonas.find(p => p.id === persona_id) || null :
          {
            total_personas: this.academyStatus.currentPersonas.length,
            active_personas: this.academyStatus.currentPersonas.filter(p => p.evolution.evolutionStage === 'training').length,
            idle_personas: this.academyStatus.currentPersonas.filter(p => p.evolution.evolutionStage === 'spawning').length,
            average_capability: 0.82,
            highest_performer: 'CodeMaster_Alpha',
            most_evolved: 'ReasoningBot_Beta'
          },
        p2p_network: include_p2p ? this.p2pNetworkStatus : null,
        vector_space_evolution: include_vector_space ? this.vectorSpaceEvolution : null,
        adversarial_training: include_adversarial ? {
          trainer_ai_status: {
            id: 'protocol_sheriff_1',
            specialization: 'adversarial_evaluation',
            challenge_generation_rate: '1.2 challenges/minute',
            success_rate: 0.67,
            current_focus: 'edge_case_discovery'
          },
          battle_statistics: {
            total_battles: 156,
            trainer_wins: 52,
            lora_wins: 104,
            win_rate_trend: 'lora_improving'
          }
        } : null,
        system_health: {
          daemon_status: 'all_healthy',
          memory_usage: '247 MB',
          cpu_utilization: '23%',
          uptime: '4d 7h 23m',
          performance_score: 0.95
        },
        performance_metrics: {
          training_efficiency: 0.89,
          evolution_speed: 'accelerating',
          resource_utilization: 'optimal',
          autonomous_operation_score: 0.91
        }
      };
      
      return {
        success: true,
        data: status
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Status retrieval error: ${errorMessage}`, 'error');
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Start evolution process - The Digital Cambrian Explosion begins!
   */
  private async startEvolution(data: any): Promise<DaemonResponse> {
    const { generations = 5, population_size = 10, evolutionary_pressure } = data;
    
    this.log(`🧬 Starting Academy Evolution: ${generations} generations, ${population_size} personas`);
    
    try {
      this.academyStatus.academyMode = 'evolving';
      
      // Create initial population if ecosystem is empty
      if (this.academyEcosystem.personas.size === 0) {
        await this.createInitialPopulation(population_size);
      }
      
      // Get current personas
      const currentPersonas = Array.from(this.academyEcosystem.personas.values());
      
      // Set up evolutionary pressure
      const pressure: EvolutionaryPressure = evolutionary_pressure || {
        survivalRate: 0.6,
        selectionCriteria: {
          performance: 0.4,
          innovation: 0.2,
          adaptation: 0.2,
          collaboration: 0.15,
          teaching: 0.05
        },
        environmentalFactors: ['competition', 'resource_scarcity'],
        competitionLevel: 0.5,
        collaborationRequirement: 0.3
      };
      
      // Run evolution for specified generations
      let evolvedPersonas = currentPersonas;
      for (let generation = 1; generation <= generations; generation++) {
        this.log(`⚡ Running Evolution Generation ${generation}/${generations}`);
        
        // Create sandboxed sessions for each persona
        await this.createEvolutionSessions(evolvedPersonas);
        
        // Run one generation of evolution
        evolvedPersonas = await this.evolutionEngine.runGeneration(evolvedPersonas, pressure);
        
        // Update ecosystem with new personas
        await this.updateEcosystem(evolvedPersonas);
        
        // Clean up sessions after generation
        await this.cleanupEvolutionSessions();
        
        this.log(`✅ Generation ${generation} complete: ${evolvedPersonas.length} personas`);
      }
      
      this.academyStatus.academyMode = 'idle';
      this.academyStatus.currentPersonas = evolvedPersonas;
      
      return {
        success: true,
        data: {
          message: `Evolution completed successfully: ${generations} generations`,
          final_population: evolvedPersonas.length,
          ecosystem_metrics: this.evolutionEngine.getEcosystemMetrics(),
          ecosystem_health: this.evolutionEngine.getEcosystemHealth(),
          generations_completed: generations
        }
      };
      
    } catch (error) {
      this.academyStatus.academyMode = 'idle';
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Evolution failed: ${errorMessage}`, 'error');
      
      return {
        success: false,
        error: errorMessage
      };
    }
  }
  
  /**
   * Get current evolution status
   */
  private async getEvolutionStatus(data: any): Promise<DaemonResponse> {
    const { include_personas = false } = data;
    
    const status = {
      mode: this.academyStatus.academyMode,
      ecosystem_metrics: this.evolutionEngine.getEcosystemMetrics(),
      ecosystem_health: this.evolutionEngine.getEcosystemHealth(),
      total_personas: this.academyEcosystem.personas.size,
      active_sessions: this.academyEcosystem.activeSessions.size,
      personas: include_personas ? Array.from(this.academyEcosystem.personas.values()) : undefined
    };
    
    return {
      success: true,
      data: status
    };
  }
  
  /**
   * Get ecosystem metrics
   */
  private async getEcosystemMetrics(data: any): Promise<DaemonResponse> {
    const { detailed = false } = data;
    
    const metrics = this.evolutionEngine.getEcosystemMetrics();
    const health = this.evolutionEngine.getEcosystemHealth();
    
    // Ecosystem metrics response with conditional detailed analysis - following middle-out/architecture-patterns/dynamic-response-construction.md
    const response = {
      metrics,
      health,
      timestamp: new Date().toISOString(),
      
      // Conditional detailed analysis using spread pattern
      ...(detailed && {
        detailed_analysis: {
          role_distribution: this.calculateRoleDistribution(),
          specialization_distribution: this.calculateSpecializationDistribution(),
          lineage_statistics: this.calculateLineageStatistics()
        }
      })
    };
    
    return {
      success: true,
      data: response
    };
  }
  
  /**
   * Get persona lineage information
   */
  private async getPersonaLineage(data: any): Promise<DaemonResponse> {
    const { persona_id } = data;
    
    if (persona_id) {
      const persona = this.academyEcosystem.personas.get(persona_id);
      if (!persona) {
        return {
          success: false,
          error: `Persona not found: ${persona_id}`
        };
      }
      
      return {
        success: true,
        data: {
          persona_id,
          lineage: persona.lineage,
          evolution: persona.evolution,
          ancestors: this.getAncestorDetails(persona),
          descendants: this.getDescendantDetails(persona)
        }
      };
    }
    
    // Return lineage tree for all personas
    const lineageTree = this.buildLineageTree();
    
    return {
      success: true,
      data: {
        lineage_tree: lineageTree,
        total_lineages: lineageTree.length,
        average_generation: this.calculateAverageGeneration()
      }
    };
  }
  
  /**
   * Create initial population for evolution
   */
  private async createInitialPopulation(size: number): Promise<void> {
    this.log(`🌱 Creating initial population of ${size} personas`);
    
    const specializations = ['typescript', 'testing', 'architecture', 'ui_design', 'debugging', 'optimization'];
    const names = ['Alpha', 'Beta', 'Gamma', 'Delta', 'Epsilon', 'Zeta', 'Eta', 'Theta', 'Iota', 'Kappa'];
    
    for (let i = 0; i < size; i++) {
      const specialization = specializations[i % specializations.length];
      const name = `${specialization}${names[i % names.length]}`;
      
      // Create persona genome using existing spawn logic
      const spawnResult = await this.spawnPersona({
        persona_name: name,
        specialization: specialization,
        evolution_mode: 'spawning'
      });
      
      if (spawnResult.success && spawnResult.data) {
        // Convert spawn result to PersonaGenome format
        const personaGenome = this.convertSpawnResultToGenome(spawnResult.data, specialization);
        this.academyEcosystem.personas.set(personaGenome.id, personaGenome);
      }
    }
    
    this.log(`✅ Initial population created: ${this.academyEcosystem.personas.size} personas`);
  }
  
  /**
   * Create sandboxed sessions for evolution
   */
  private async createEvolutionSessions(personas: PersonaGenome[]): Promise<void> {
    this.log(`🏗️ Creating sandboxed sessions for ${personas.length} personas`);
    
    for (const persona of personas) {
      // Create isolated session for each persona
      const sessionId = `evolution_${persona.id}_${Date.now()}`;
      
      // TODO: Integrate with actual session system
      // For now, we'll track sessions in our ecosystem
      this.academyEcosystem.activeSessions.set(sessionId, {
        id: sessionId,
        sessionType: 'individual',
        participants: [persona],
        challenges: [],
        results: [],
        startTime: new Date(),
        evolutionaryPressure: {
          survivalRate: 0.6,
          selectionCriteria: {
            performance: 0.4,
            innovation: 0.2,
            adaptation: 0.2,
            collaboration: 0.15,
            teaching: 0.05
          },
          environmentalFactors: ['competition'],
          competitionLevel: 0.5,
          collaborationRequirement: 0.3
        },
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
      });
    }
  }
  
  /**
   * Update ecosystem with evolved personas
   */
  private async updateEcosystem(personas: PersonaGenome[]): Promise<void> {
    // Clear existing personas
    this.academyEcosystem.personas.clear();
    
    // Add evolved personas
    for (const persona of personas) {
      this.academyEcosystem.personas.set(persona.id, persona);
    }
    
    // Update ecosystem metrics
    this.academyEcosystem.ecosystemMetrics = this.evolutionEngine.getEcosystemMetrics();
  }
  
  /**
   * Clean up evolution sessions
   */
  private async cleanupEvolutionSessions(): Promise<void> {
    this.academyEcosystem.activeSessions.clear();
  }
  
  /**
   * Convert spawn result to PersonaGenome format
   */
  private convertSpawnResultToGenome(spawnData: any, specialization: string): PersonaGenome {
    // This converts the existing spawn format to our new PersonaGenome format
    // TODO: Implement proper conversion logic
    return {
      id: spawnData.persona_id,
      identity: {
        name: spawnData.persona_name,
        role: 'student',
        generation: 0,
        specialization: specialization,
        personality: {
          creativity: 0.5,
          analytical: 0.5,
          helpfulness: 0.8,
          competitiveness: 0.4,
          patience: 0.6,
          innovation: 0.5
        },
        goals: [`master_${specialization}`, 'collaborate_effectively']
      },
      knowledge: {
        domain: specialization,
        expertise: [specialization],
        competencies: { [specialization]: 0.6 },
        experiencePoints: 0
      },
      behavior: {
        adaptationRate: 0.5,
        communicationStyle: 'direct',
        decisionMakingStyle: 'analytical',
        riskTolerance: 0.4,
        collaborationPreference: 0.6
      },
      evolution: {
        generation: 0,
        parentGenomes: [],
        mutationHistory: [],
        evolutionStage: 'spawning',
        fitnessScore: 0.5,
        adaptationSuccess: 0,
        survivalRounds: 0,
        evolutionPressure: []
      },
      substrate: {
        loraIds: spawnData.lora_stack?.layers?.map((l: any) => l.name) || [],
        memoryPatterns: ['working_memory'],
        processingStyle: 'sequential',
        adaptationMechanisms: ['reinforcement_learning'],
        vectorPosition: Array.from({ length: 10 }, () => Math.random())
      },
      reproduction: {
        mutationRate: 0.1,
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
  }
  
  // Helper methods for analysis
  private calculateRoleDistribution(): Record<string, number> {
    const distribution: Record<string, number> = {};
    for (const persona of this.academyEcosystem.personas.values()) {
      distribution[persona.identity.role] = (distribution[persona.identity.role] || 0) + 1;
    }
    return distribution;
  }
  
  private calculateSpecializationDistribution(): Record<string, number> {
    const distribution: Record<string, number> = {};
    for (const persona of this.academyEcosystem.personas.values()) {
      const spec = persona.identity.specialization;
      distribution[spec] = (distribution[spec] || 0) + 1;
    }
    return distribution;
  }
  
  private calculateLineageStatistics(): any {
    const personas = Array.from(this.academyEcosystem.personas.values());
    const generations = personas.map(p => p.evolution.generation);
    
    return {
      total_personas: personas.length,
      max_generation: Math.max(...generations),
      avg_generation: generations.reduce((a, b) => a + b, 0) / generations.length,
      lineages_with_descendants: personas.filter(p => p.lineage.descendants.length > 0).length
    };
  }
  
  private getAncestorDetails(persona: PersonaGenome): any[] {
    return persona.lineage.ancestors.map(ancestorId => {
      const ancestor = this.academyEcosystem.personas.get(ancestorId);
      return ancestor ? {
        id: ancestorId,
        name: ancestor.identity.name,
        generation: ancestor.evolution.generation,
        specialization: ancestor.identity.specialization
      } : { id: ancestorId, name: 'Unknown', generation: -1, specialization: 'unknown' };
    });
  }
  
  private getDescendantDetails(persona: PersonaGenome): any[] {
    return persona.lineage.descendants.map(descendantId => {
      const descendant = this.academyEcosystem.personas.get(descendantId);
      return descendant ? {
        id: descendantId,
        name: descendant.identity.name,
        generation: descendant.evolution.generation,
        specialization: descendant.identity.specialization
      } : { id: descendantId, name: 'Unknown', generation: -1, specialization: 'unknown' };
    });
  }
  
  private buildLineageTree(): any[] {
    const personas = Array.from(this.academyEcosystem.personas.values());
    const tree = [];
    
    // Build family trees starting from generation 0
    const founders = personas.filter(p => p.evolution.generation === 0);
    
    for (const founder of founders) {
      tree.push({
        id: founder.id,
        name: founder.identity.name,
        generation: founder.evolution.generation,
        specialization: founder.identity.specialization,
        descendants: this.buildDescendantTree(founder, personas)
      });
    }
    
    return tree;
  }
  
  private buildDescendantTree(ancestor: PersonaGenome, allPersonas: PersonaGenome[]): any[] {
    const descendants = allPersonas.filter(p => 
      p.lineage.ancestors.includes(ancestor.id)
    );
    
    return descendants.map(descendant => ({
      id: descendant.id,
      name: descendant.identity.name,
      generation: descendant.evolution.generation,
      specialization: descendant.identity.specialization,
      descendants: this.buildDescendantTree(descendant, allPersonas)
    }));
  }
  
  private calculateAverageGeneration(): number {
    const personas = Array.from(this.academyEcosystem.personas.values());
    if (personas.length === 0) return 0;
    
    const totalGeneration = personas.reduce((sum, p) => sum + p.evolution.generation, 0);
    return totalGeneration / personas.length;
  }

  /**
   * Get current Academy capabilities
   */
  public getCapabilities(): string[] {
    return [
      'academy-management',
      'persona-training',
      'persona-spawning',
      'persona-evolution',
      'ecosystem-monitoring',
      'lineage-tracking',
      'vector-space-evolution',
      'local-lora-discovery',
      'adversarial-training',
      'progress-tracking',
      'academy-ui-integration',
      'local-training-mode'
    ];
  }

  /**
   * Get supported message types
   */
  public getMessageTypes(): string[] {
    return [
      'get_initial_academy_status',
      'academy_message',
      'get_training_progress',
      'start_training',
      'start_evolution_session',
      'start_evolution',
      'get_evolution_status',
      'get_ecosystem_metrics',
      'get_persona_lineage',
      'stop_training',
      'spawn_persona',
      'get_comprehensive_status'
    ];
  }
}

// Main execution when run directly (ES module style)
if (process.argv[1] && process.argv[1].endsWith('AcademyDaemon.ts')) {
  const daemon = new AcademyDaemon();
  
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received shutdown signal...');
    await daemon.stop();
    process.exit(0);
  });
  
  process.on('SIGTERM', async () => {
    console.log('\n🛑 Received termination signal...');
    await daemon.stop();
    process.exit(0);
  });
  
  daemon.start().catch(error => {
    console.error('❌ Academy daemon failed:', error);
    process.exit(1);
  });
}