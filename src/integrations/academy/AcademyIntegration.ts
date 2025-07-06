/**
 * Academy Integration - Top-level integration orchestrator
 * 
 * Hierarchical Architecture: integrations → daemons → commands → drivers → subsystems
 */

import { BaseDaemon } from '../../daemons/base/BaseDaemon.js';
import { DaemonMessage, DaemonResponse } from '../../daemons/base/DaemonProtocol.js';
import { AcademyDaemon } from '../../daemons/academy/AcademyDaemon.js';
import { PersonaDaemon } from '../../daemons/persona/PersonaDaemon.js';
import { DatabaseDaemon } from '../../daemons/database/DatabaseDaemon.js';

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
  
  private config: AcademyIntegrationConfig;
  private academyDaemon: AcademyDaemon;
  private personaDaemon: PersonaDaemon;
  private databaseDaemon: DatabaseDaemon;
  private isInitialized: boolean = false;

  constructor(config: Partial<AcademyIntegrationConfig> = {}) {
    super();
    this.config = {
      local_mode: true,
      p2p_enabled: false,
      max_concurrent_sessions: 3,
      training_data_path: '.continuum/academy/training',
      model_cache_path: '.continuum/academy/models',
      evaluation_interval_ms: 30000,
      ...config
    };

    // Initialize daemons
    this.academyDaemon = new AcademyDaemon();
    this.personaDaemon = new PersonaDaemon({
      id: 'academy-persona',
      name: 'academy-persona',
      modelProvider: 'local',
      modelConfig: { model: 'default' },
      capabilities: ['training', 'evaluation'],
      sessionDirectory: '.continuum/academy/sessions',
      loraAdapters: []
    });
    this.databaseDaemon = new DatabaseDaemon();
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
      // Start daemons in dependency order
      console.log('📦 Starting DatabaseDaemon...');
      await this.databaseDaemon.start();

      console.log('🤖 Starting PersonaDaemon...');
      await this.personaDaemon.start();

      console.log('🎓 Starting AcademyDaemon...');
      await this.academyDaemon.start();

      // Verify integration health
      const status = await this.getIntegrationStatus();
      if (status.integration_health !== 'healthy') {
        throw new Error(`Integration health check failed: ${status.integration_health}`);
      }

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
      // Stop daemons in reverse dependency order
      if (this.academyDaemon) {
        await this.academyDaemon.stop();
      }

      if (this.personaDaemon) {
        await this.personaDaemon.stop();
      }

      if (this.databaseDaemon) {
        await this.databaseDaemon.stop();
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
      // Check daemon health
      if (this.academyDaemon && await this.isDaemonHealthy(this.academyDaemon)) {
        status.academy_daemon = 'running';
      }

      if (this.personaDaemon && await this.isDaemonHealthy(this.personaDaemon)) {
        status.persona_daemon = 'running';
      }

      if (this.databaseDaemon && await this.isDaemonHealthy(this.databaseDaemon)) {
        status.database_daemon = 'running';
      }

      // Check local trainer
      if (status.academy_daemon === 'running') {
        try {
          const academyStatus = await this.sendMessage('academy', 'get_comprehensive_status', {});
          
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
  async startTrainingSession(params: {
    student_persona: string;
    trainer_mode?: string;
    evolution_target?: string;
    vector_exploration?: boolean;
  }): Promise<any> {
    if (!this.isInitialized) {
      throw new Error('Academy Integration not initialized');
    }

    console.log(`🎯 Starting training session for ${params.student_persona}`);

    try {
      const result = await this.sendMessage('academy', 'start_evolution_session', params);

      if (!result.success) {
        throw new Error(result.error || 'Training session failed to start');
      }

      const sessionData = result.data as any; // TODO: Add proper training session type
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
  async spawnPersona(params: {
    persona_name: string;
    base_model?: string;
    specialization?: string;
    skill_vector?: number[];
    p2p_seed?: boolean;
  }): Promise<any> {
    if (!this.isInitialized) {
      throw new Error('Academy Integration not initialized');
    }

    console.log(`🧬 Spawning persona: ${params.persona_name}`);

    try {
      const result = await this.sendMessage('academy', 'spawn_persona', params);

      if (!result.success) {
        throw new Error(result.error || 'Persona spawning failed');
      }

      const personaData = result.data as any; // TODO: Add proper persona type
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
  async getAcademyStatus(): Promise<any> {
    if (!this.isInitialized) {
      throw new Error('Academy Integration not initialized');
    }

    try {
      const [academyResult, integrationStatus] = await Promise.all([
        this.sendMessage('academy', 'get_comprehensive_status', {
          include_p2p: this.config.p2p_enabled,
          include_vector_space: true,
          include_adversarial: true
        }),
        this.getIntegrationStatus()
      ]);

      return {
        academy_data: academyResult.success ? academyResult.data : null,
        integration_status: integrationStatus,
        config: this.config,
        timestamp: new Date().toISOString()
      };

    } catch (error) {
      console.error('❌ Academy status retrieval failed:', error);
      throw error;
    }
  }

  // Private utility methods
  private async isDaemonHealthy(daemon: any): Promise<boolean> {
    try {
      // Basic health check - daemon should respond to capabilities request
      const response = await this.sendMessage(daemon.name, 'get_capabilities', {});
      return response.success === true;
    } catch {
      return false;
    }
  }

  private async cleanup(): Promise<void> {
    // Best effort cleanup of any started components
    try {
      if (this.academyDaemon) await this.academyDaemon.stop();
    } catch (error) {
      console.warn('Academy daemon cleanup warning:', error);
    }

    try {
      if (this.personaDaemon) await this.personaDaemon.stop();
    } catch (error) {
      console.warn('Persona daemon cleanup warning:', error);
    }

    try {
      if (this.databaseDaemon) await this.databaseDaemon.stop();
    } catch (error) {
      console.warn('Database daemon cleanup warning:', error);
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
  public async getComprehensiveStatus(): Promise<any> {
    const [academyResult, integrationStatus] = await Promise.all([
      this.sendMessage('academy', 'get_comprehensive_status', {
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
  get academy(): AcademyDaemon { return this.academyDaemon; }
  get persona(): PersonaDaemon { return this.personaDaemon; }
  get database(): DatabaseDaemon { return this.databaseDaemon; }
}