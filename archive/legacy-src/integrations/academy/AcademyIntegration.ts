/**
 * Academy Integration - Top-level integration orchestrator
 * 
 * Hierarchical Architecture: integrations → daemons → commands → drivers → subsystems
 */

import { BaseDaemon } from '../../daemons/base/BaseDaemon.js';
import { DaemonMessage, DaemonResponse } from '../../daemons/base/DaemonProtocol.js';
import { DaemonType } from '../../daemons/base/DaemonTypes';
import { AcademyDaemon } from '../../daemons/academy/AcademyDaemon.js';
import { PersonaDaemon } from '../../daemons/persona/PersonaDaemon.js';
import { DatabaseDaemon } from '../../daemons/database/DatabaseDaemon.js';
import { ModuleDiscovery, ModuleUtils, type ModuleDependency } from '../../core/modules/index.js';
import { 
  TrainingSessionData, 
  PersonaData, 
  AcademySystemStatus,
  TrainingSessionParams,
  PersonaSpawnParams
} from './types.js';

export interface AcademyIntegrationConfig {
  local_mode: boolean;
  p2p_enabled: boolean;
  max_concurrent_sessions: number;
  training_data_path: string;
  model_cache_path: string;
  evaluation_interval_ms: number;
}

export interface IntegrationStatus {
  academy_daemon: 'running' | 'stopped' | 'error';
  persona_daemon: 'running' | 'stopped' | 'error';
  database_daemon: 'running' | 'stopped' | 'error';
  local_trainer: 'operational' | 'idle' | 'error';
  lora_discovery: 'available' | 'scanning' | 'unavailable';
  integration_health: 'healthy' | 'degraded' | 'failed';
}

/**
 * Academy Integration Module Dependencies
 * Consistent with core module discovery system
 */
export const ACADEMY_MODULE_DEPENDENCIES = {
  academy: {
    name: 'academy',
    type: 'daemon' as const,
    required: true,
    healthCheck: 'get_capabilities',
    config: {}
  },
  persona: {
    name: 'persona', 
    type: 'daemon' as const,
    required: true,
    healthCheck: 'get_capabilities',
    config: {
      // TODO: HARDCODED_ID - Make persona ID configurable
      id: 'academy-persona',
      // TODO: HARDCODED_NAME - Make persona name configurable
      name: 'academy-persona',
      // TODO: HARDCODED_PROVIDER - Make model provider configurable via environment variables
      modelProvider: 'local',
      // TODO: HARDCODED_MODEL - Make default model configurable via environment variables
      modelConfig: { model: 'default' },
      // TODO: HARDCODED_CAPABILITIES - Make capabilities configurable
      capabilities: ['training', 'evaluation'] as string[],
      // TODO: HARDCODED_PATH - Make session directory configurable via environment variables
      sessionDirectory: '.continuum/academy/sessions',
      loraAdapters: [] as string[]
    }
  },
  database: {
    name: 'database',
    type: 'daemon' as const,
    required: true,
    healthCheck: 'get_capabilities',
    config: {}
  }
} as const satisfies Record<string, ModuleDependency>;

/**
 * Academy Integration - Orchestrates the complete Academy ecosystem
 * 
 * This is the top-level integration that coordinates:
 * - AcademyDaemon (training orchestration)
 * - PersonaDaemon (LoRA management) 
 * - DatabaseDaemon (persistence)
 * - Local training capabilities
 * - Future P2P networking
 */
export class AcademyIntegration extends BaseDaemon {
  public readonly name = 'academy-integration';
  public readonly version = '1.0.0';
  public readonly daemonType = DaemonType.ACADEMY;
  
  private config: AcademyIntegrationConfig;
  private daemons: Record<keyof typeof ACADEMY_MODULE_DEPENDENCIES, BaseDaemon>;
  private isInitialized: boolean = false;
  private integrationStartTime?: Date;
  private dependencyIterator = ModuleDiscovery.getInstance().createDependencyIterator(ACADEMY_MODULE_DEPENDENCIES);

  constructor(config: Partial<AcademyIntegrationConfig> = {}) {
    super();
    this.config = {
      local_mode: true,
      p2p_enabled: false,
      max_concurrent_sessions: 3,
      // TODO: HARDCODED_PATH - Make training_data_path configurable via environment variables
      training_data_path: '.continuum/academy/training',
      // TODO: HARDCODED_PATH - Make model_cache_path configurable via environment variables  
      model_cache_path: '.continuum/academy/models',
      // TODO: HARDCODED_TIMEOUT - Make evaluation_interval_ms configurable via environment variables
      evaluation_interval_ms: 30000,
      ...config
    };

    // Initialize daemons using consistent module pattern
    this.daemons = {
      academy: new AcademyDaemon(),
      persona: new PersonaDaemon({ ...ACADEMY_MODULE_DEPENDENCIES.persona.config }),
      database: new DatabaseDaemon()
    };
  }

  /**
   * Initialize the complete Academy integration
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      console.log('🎓 Academy Integration already initialized');
      return;
    }

    console.log('🚀 Initializing Academy Integration...');

    try {
      // Start daemons in dependency order using module system
      const startupOrder = ModuleUtils.calculateStartupOrder(ACADEMY_MODULE_DEPENDENCIES);
      
      for (const daemonName of startupOrder) {
        console.log(`🚀 Starting ${daemonName} daemon...`);
        await this.daemons[daemonName].start();
      }

      // Verify integration health
      const status = await this.getIntegrationStatus();
      if (status.integration_health !== 'healthy') {
        throw new Error(`Integration health check failed: ${status.integration_health}`);
      }

      this.integrationStartTime = new Date();
      this.isInitialized = true;
      console.log('✅ Academy Integration initialized successfully');
      console.log(`🔧 Mode: ${this.config.local_mode ? 'Local' : 'Distributed'} | P2P: ${this.config.p2p_enabled ? 'Enabled' : 'Disabled'}`);

    } catch (error) {
      console.error('❌ Academy Integration initialization failed:', error);
      await this.cleanup();
      throw error;
    }
  }

  /**
   * Shutdown the Academy integration gracefully
   */
  async shutdown(): Promise<void> {
    console.log('🛑 Shutting down Academy Integration...');

    try {
      // Stop daemons in reverse dependency order using module system
      const shutdownOrder = ModuleUtils.calculateShutdownOrder(ACADEMY_MODULE_DEPENDENCIES);
      
      for (const daemonName of shutdownOrder) {
        console.log(`🛑 Stopping ${daemonName} daemon...`);
        await this.daemons[daemonName]?.stop();
      }

      this.isInitialized = false;
      console.log('✅ Academy Integration shutdown complete');

    } catch (error) {
      console.error('❌ Academy Integration shutdown error:', error);
      throw error;
    }
  }

  /**
   * Get comprehensive integration status
   */
  async getIntegrationStatus(): Promise<IntegrationStatus> {
    const status: IntegrationStatus = {
      academy_daemon: 'stopped',
      persona_daemon: 'stopped', 
      database_daemon: 'stopped',
      local_trainer: 'idle',
      lora_discovery: 'unavailable',
      integration_health: 'failed'
    };

    try {
      // Check daemon health using dependency iterator
      const healthChecks = await Promise.all(
        this.dependencyIterator.names.map(async (name) => ({
          name,
          healthy: await this.isDaemonHealthy(this.daemons[name])
        }))
      );

      // Update status based on health checks
      for (const { name, healthy } of healthChecks) {
        const statusKey = `${name}_daemon` as keyof IntegrationStatus;
        if (statusKey in status) {
          (status as any)[statusKey] = healthy ? 'running' : 'stopped';
        }
      }

      // Check local trainer
      if (status.academy_daemon === 'running') {
        try {
          const academyStatus = await this.sendMessage(DaemonType.ACADEMY, 'get_comprehensive_status', {});
          
          if (academyStatus.success) {
            status.local_trainer = 'operational';
            status.lora_discovery = 'available';
          }
        } catch (error) {
          status.local_trainer = 'error';
        }
      }

      // Determine overall health
      const criticalDaemons = ['academy_daemon', 'persona_daemon', 'database_daemon'];
      const runningDaemons = criticalDaemons.filter(daemon => status[daemon as keyof IntegrationStatus] === 'running');

      if (runningDaemons.length === criticalDaemons.length) {
        status.integration_health = 'healthy';
      } else if (runningDaemons.length > 0) {
        status.integration_health = 'degraded';
      } else {
        status.integration_health = 'failed';
      }

    } catch (error) {
      console.error('Status check error:', error);
      status.integration_health = 'failed';
    }

    return status;
  }

  /**
   * Start a training session through the integration
   */
  async startTrainingSession(params: TrainingSessionParams): Promise<TrainingSessionData> {
    if (!this.isInitialized) {
      throw new Error('Academy Integration not initialized');
    }

    console.log(`🎯 Starting training session for ${params.student_persona}`);

    try {
      const result = await this.sendMessage(DaemonType.ACADEMY, 'start_evolution_session', params);

      if (!result.success) {
        throw new Error(result.error || 'Training session failed to start');
      }

      const sessionData = result.data as TrainingSessionData;
      console.log(`✅ Training session started: ${sessionData.session_id}`);
      return sessionData;

    } catch (error) {
      console.error('❌ Training session failed:', error);
      throw error;
    }
  }

  /**
   * Spawn a new persona through the integration
   */
  async spawnPersona(params: PersonaSpawnParams): Promise<PersonaData> {
    if (!this.isInitialized) {
      throw new Error('Academy Integration not initialized');
    }

    console.log(`🧬 Spawning persona: ${params.persona_name}`);

    try {
      const result = await this.sendMessage(DaemonType.ACADEMY, 'spawn_persona', params);

      if (!result.success) {
        throw new Error(result.error || 'Persona spawning failed');
      }

      const personaData = result.data as PersonaData;
      console.log(`✅ Persona spawned: ${personaData.persona_id}`);
      return personaData;

    } catch (error) {
      console.error('❌ Persona spawning failed:', error);
      throw error;
    }
  }

  /**
   * Get comprehensive Academy system status
   */
  async getAcademyStatus(): Promise<AcademySystemStatus> {
    if (!this.isInitialized) {
      throw new Error('Academy Integration not initialized');
    }

    try {
      const [academyResult] = await Promise.all([
        this.sendMessage(DaemonType.ACADEMY, 'get_comprehensive_status', {
          include_p2p: this.config.p2p_enabled,
          include_vector_space: true,
          include_adversarial: true
        }),
        this.getIntegrationStatus()
      ]);

      return {
        academy_daemon: {
          status: academyResult.success ? 'running' : 'error',
          version: this.version,
          uptime_ms: this.integrationStartTime ? Date.now() - this.integrationStartTime.getTime() : 0,
          active_sessions: 0,
          total_personas: 0
        },
        training_system: {
          status: 'operational',
          queue_length: 0,
          active_sessions: [],
          models_cached: 0
        },
        persona_system: {
          status: 'operational',
          active_personas: 0,
          spawning_queue: 0,
          lora_adapters_available: 0
        },
        vector_space: {
          status: 'available',
          dimensions: 0,
          total_vectors: 0,
          index_last_updated: new Date().toISOString()
        }
      } as AcademySystemStatus;

    } catch (error) {
      console.error('❌ Academy status retrieval failed:', error);
      throw error;
    }
  }

  // Private utility methods
  private async isDaemonHealthy(daemon: BaseDaemon): Promise<boolean> {
    try {
      // Basic health check - daemon should respond to capabilities request
      const response = await this.sendMessage(daemon.name as any, 'get_capabilities', {});
      return response.success === true;
    } catch {
      return false;
    }
  }

  private async cleanup(): Promise<void> {
    // Best effort cleanup using dependency iterator
    const shutdownOrder = ModuleUtils.calculateShutdownOrder(ACADEMY_MODULE_DEPENDENCIES);
    
    for (const daemonName of shutdownOrder) {
      try {
        if (this.daemons[daemonName]) {
          await this.daemons[daemonName].stop();
        }
      } catch (error) {
        console.warn(`${daemonName} daemon cleanup warning:`, error);
      }
    }
  }

  /**
   * Start the Academy Integration daemon
   */
  protected async onStart(): Promise<void> {
    await this.initialize();
  }

  /**
   * Stop the Academy Integration daemon
   */
  protected async onStop(): Promise<void> {
    await this.cleanup();
  }

  /**
   * Get comprehensive Academy status
   */
  public async getComprehensiveStatus(): Promise<{
    academy: any;
    integration: IntegrationStatus;
    timestamp: string;
  }> {
    const [academyResult, integrationStatus] = await Promise.all([
      this.sendMessage(DaemonType.ACADEMY, 'get_comprehensive_status', {
        include_p2p: this.config.p2p_enabled,
        include_vector_space: true,
        include_adversarial: true
      }),
      this.getIntegrationStatus()
    ]);

    return {
      academy: academyResult.data,
      integration: integrationStatus,
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Handle daemon messages
   */
  public async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    try {
      switch (message.type) {
        case 'get_integration_status':
          const status = await this.getIntegrationStatus();
          return { success: true, data: status };
          
        case 'get_comprehensive_status':
          const comprehensive = await this.getComprehensiveStatus();
          return { success: true, data: comprehensive };
          
        default:
          return {
            success: false,
            error: `Unknown message type: ${message.type}`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `AcademyIntegration error: ${errorMessage}`
      };
    }
  }

  // Getters for daemon access (if needed by higher-level integrations)
  get academy(): AcademyDaemon { return this.daemons.academy as AcademyDaemon; }
  get persona(): PersonaDaemon { return this.daemons.persona as PersonaDaemon; }
  get database(): DatabaseDaemon { return this.daemons.database as DatabaseDaemon; }
  
  // Generic daemon access using dependency names
  getDaemon<T extends keyof typeof ACADEMY_MODULE_DEPENDENCIES>(name: T): BaseDaemon {
    return this.daemons[name];
  }
}