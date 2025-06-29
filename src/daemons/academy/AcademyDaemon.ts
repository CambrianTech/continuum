/**
 * Academy Daemon - Handles Academy-related functionality
 * Manages persona training, progress tracking, and Academy UI integration
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';

export interface AcademyStatus {
  isActive: boolean;
  currentPersonas: any[];
  trainingProgress: Record<string, number>;
  academyMode: 'idle' | 'training' | 'evaluating';
}

export class AcademyDaemon extends BaseDaemon {
  public readonly name = 'academy';
  public readonly version = '1.0.0';

  private academyStatus: AcademyStatus = {
    isActive: false,
    currentPersonas: [],
    trainingProgress: {},
    academyMode: 'idle'
  };

  protected async onStart(): Promise<void> {
    this.log('🎓 Starting Academy Daemon...');
    
    // Initialize Academy state
    this.academyStatus = {
      isActive: true,
      currentPersonas: [],
      trainingProgress: {},
      academyMode: 'idle'
    };
    
    this.log('✅ Academy Daemon started successfully');
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Academy Daemon...');
    this.academyStatus.isActive = false;
    this.log('✅ Academy Daemon stopped');
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
        return await this.startTraining(message.data);
        
      case 'stop_training':
        return await this.stopTraining(message.data);
        
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
    const { personaId, config } = data;
    
    this.log(`🚀 Starting training for persona: ${personaId}`);
    
    // Initialize training progress
    if (personaId) {
      this.academyStatus.trainingProgress[personaId] = 0;
      this.academyStatus.academyMode = 'training';
      
      // Add to current personas if not already there
      if (!this.academyStatus.currentPersonas.find(p => p.id === personaId)) {
        this.academyStatus.currentPersonas.push({
          id: personaId,
          startTime: new Date().toISOString(),
          config: config || {}
        });
      }
    }
    
    return {
      success: true,
      data: {
        personaId,
        status: 'training_started',
        academyMode: this.academyStatus.academyMode
      }
    };
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
   * Get current Academy capabilities
   */
  public getCapabilities(): string[] {
    return [
      'academy-management',
      'persona-training',
      'progress-tracking',
      'academy-ui-integration'
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
      'stop_training'
    ];
  }
}

// Main execution when run directly
if (require.main === module) {
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