/**
 * CommandWidget - Layer 1.5 Intermediate Base Class
 * 
 * Adds enhanced command execution patterns with error handling,
 * command sequences, and workflow management.
 * 
 * Used by widgets that need robust command execution like:
 * - AcademyWidget (academy-spawn, academy-train commands)
 * - ChatWidget (chat, createroom commands)  
 * - PersonaWidget (persona management commands)
 */

import { BaseWidget } from '../../shared/BaseWidget.js';

export interface CommandResult {
  success: boolean;
  data?: any;
  error?: string;
  message?: string;
}

export interface CommandSequenceStep {
  command: string;
  params: any;
  description?: string;
  onSuccess?: (data: any) => void;
  onError?: (error: string) => void;
  required?: boolean; // If false, continue sequence even if this fails
}

export abstract class CommandWidget extends BaseWidget {
  protected commandHistory: CommandHistoryEntry[] = [];
  protected activeCommands: Set<string> = new Set();
  protected lastCommandResult: CommandResult | null = null;

  /**
   * Simple logging method for CommandWidget subclasses
   */
  protected log(message: string, level: 'info' | 'error' | 'warn' = 'info'): void {
    const prefix = level === 'error' ? '❌' : level === 'warn' ? '⚠️' : 'ℹ️';
    console.log(`${prefix} ${this.widgetName}: ${message}`);
  }

  /**
   * Enhanced command execution with comprehensive error handling
   */
  protected async executeCommandSafely(
    command: string, 
    params: any,
    options: CommandOptions = {}
  ): Promise<CommandResult> {
    const startTime = Date.now();
    const commandId = `${command}_${startTime}`;
    
    try {
      // Track active command
      this.activeCommands.add(commandId);
      
      // Add to history
      this.commandHistory.push({
        command,
        params: this.sanitizeParamsForLogging(params),
        timestamp: new Date(),
        status: 'executing'
      });

      // Show loading state if requested
      if (options.showLoading) {
        this.setCommandLoadingState(command, true);
      }

      // Execute the command
      const result = await this.executeCommand(command, params);
      const executionTime = Date.now() - startTime;
      
      // Update history with result
      const historyEntry = this.commandHistory[this.commandHistory.length - 1];
      historyEntry.status = result.success ? 'success' : 'error';
      historyEntry.executionTime = executionTime;
      historyEntry.result = result;

      this.lastCommandResult = result;
      
      if (result.success) {
        this.onCommandSuccess(command, result.data, executionTime);
        this.log(`✅ Command ${command} succeeded in ${executionTime}ms`, 'info');
        
        if (options.successMessage) {
          this.showCommandFeedback(options.successMessage, 'success');
        }
      } else {
        this.onCommandError(command, result.error || 'Unknown error', executionTime);
        this.log(`❌ Command ${command} failed: ${result.error}`, 'error');
        
        if (options.errorMessage) {
          this.showCommandFeedback(options.errorMessage, 'error');
        }
      }

      return result;

    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMsg = error instanceof Error ? error.message : String(error);
      
      // Update history with error
      const historyEntry = this.commandHistory[this.commandHistory.length - 1];
      historyEntry.status = 'error';
      historyEntry.executionTime = executionTime;
      historyEntry.error = errorMsg;

      this.onCommandError(command, errorMsg, executionTime);
      this.log(`💥 Command ${command} threw exception: ${errorMsg}`, 'error');

      const result: CommandResult = { success: false, error: errorMsg };
      this.lastCommandResult = result;
      return result;

    } finally {
      // Clean up
      this.activeCommands.delete(commandId);
      
      if (options.showLoading) {
        this.setCommandLoadingState(command, false);
      }
      
      // Keep command history manageable
      if (this.commandHistory.length > 100) {
        this.commandHistory = this.commandHistory.slice(-50);
      }
    }
  }

  /**
   * Execute a sequence of commands with dependency handling
   * Perfect for complex workflows like Academy training setup
   */
  protected async executeCommandSequence(
    steps: CommandSequenceStep[],
    options: SequenceOptions = {}
  ): Promise<SequenceResult> {
    const results: SequenceStepResult[] = [];
    let allSuccessful = true;

    this.log(`🔄 Starting command sequence: ${steps.length} steps`, 'info');

    for (let i = 0; i < steps.length; i++) {
      const step = steps[i];
      const stepNumber = i + 1;
      
      this.log(`📍 Step ${stepNumber}/${steps.length}: ${step.description || step.command}`, 'info');

      try {
        const result = await this.executeCommandSafely(step.command, step.params, {
          showLoading: true,
          successMessage: `Step ${stepNumber} completed`,
          errorMessage: `Step ${stepNumber} failed`
        });

        const stepResult: SequenceStepResult = {
          step: stepNumber,
          command: step.command,
          success: result.success,
          data: result.data
        };
        
        if (result.error !== undefined) {
          stepResult.error = result.error;
        }

        results.push(stepResult);

        if (result.success) {
          step.onSuccess?.(result.data);
        } else {
          step.onError?.(result.error || 'Unknown error');
          
          if (step.required !== false) {
            allSuccessful = false;
            this.log(`❌ Required step ${stepNumber} failed, stopping sequence`, 'error');
            break;
          } else {
            this.log(`⚠️ Optional step ${stepNumber} failed, continuing sequence`, 'warn');
          }
        }

      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        
        results.push({
          step: stepNumber,
          command: step.command,
          success: false,
          error: errorMsg
        });

        step.onError?.(errorMsg);
        
        if (step.required !== false) {
          allSuccessful = false;
          this.log(`💥 Required step ${stepNumber} threw exception, stopping sequence`, 'error');
          break;
        }
      }

      // Optional delay between steps
      if (options.stepDelay && i < steps.length - 1) {
        await new Promise(resolve => setTimeout(resolve, options.stepDelay));
      }
    }

    const finalResult: SequenceResult = {
      success: allSuccessful,
      completedSteps: results.filter(r => r.success).length,
      totalSteps: steps.length,
      results
    };

    if (allSuccessful) {
      this.log(`✅ Command sequence completed successfully`, 'info');
      options.onSuccess?.(finalResult);
    } else {
      this.log(`❌ Command sequence failed`, 'error');
      options.onError?.(finalResult);
    }

    return finalResult;
  }

  /**
   * Academy-specific command patterns
   */
  protected async spawnPersona(personaName: string, options: PersonaSpawnOptions = {}): Promise<CommandResult> {
    return this.executeCommandSafely('academy-spawn', {
      persona_name: personaName,
      specialization: options.specialization || 'auto-discover',
      p2p_seed: options.p2pSeed !== false,
      evolution_mode: options.evolutionMode || 'adversarial',
      base_model: options.baseModel
    }, {
      showLoading: true,
      successMessage: `🎓 Persona ${personaName} spawned successfully`,
      errorMessage: `❌ Failed to spawn persona ${personaName}`
    });
  }

  protected async startAcademyTraining(studentPersona: string, options: TrainingOptions = {}): Promise<CommandResult> {
    return this.executeCommandSafely('academy-train', {
      student_persona: studentPersona,
      trainer_mode: options.trainerMode || 'adversarial',
      evolution_target: options.evolutionTarget || 'auto-discover',
      vector_exploration: options.vectorExploration !== false,
      domain: options.domain
    }, {
      showLoading: true,
      successMessage: `🏃‍♂️ Training started for ${studentPersona}`,
      errorMessage: `❌ Failed to start training for ${studentPersona}`
    });
  }

  protected async getAcademyStatus(options: StatusOptions = {}): Promise<CommandResult> {
    return this.executeCommandSafely('academy-status', {
      detail_level: options.detailLevel || 'summary',
      include_p2p: options.includeP2P !== false,
      include_vector_space: options.includeVectorSpace !== false,
      include_adversarial: options.includeAdversarial !== false,
      persona_id: options.personaId
    });
  }

  /**
   * Chat-specific command patterns
   */
  protected async sendChatMessage(message: string, room: string = 'general'): Promise<CommandResult> {
    return this.executeCommandSafely('chat', {
      message,
      room
    }, {
      showLoading: false, // Chat should feel instant
      errorMessage: `❌ Failed to send message`
    });
  }

  protected async createChatRoom(roomName: string, description?: string): Promise<CommandResult> {
    return this.executeCommandSafely('createroom', {
      name: roomName,
      description: description || `Chat room: ${roomName}`
    }, {
      showLoading: true,
      successMessage: `💬 Room ${roomName} created`,
      errorMessage: `❌ Failed to create room ${roomName}`
    });
  }

  /**
   * Command workflow for Academy training (like in the screenshot)
   */
  protected async executeAcademyTrainingWorkflow(
    personaName: string, 
    trainingConfig: AcademyTrainingConfig
  ): Promise<SequenceResult> {
    const steps: CommandSequenceStep[] = [
      {
        command: 'academy-status',
        params: { detail_level: 'summary' },
        description: 'Check Academy system status',
        required: true
      },
      {
        command: 'academy-spawn',
        params: {
          persona_name: personaName,
          specialization: trainingConfig.specialization,
          p2p_seed: true
        },
        description: `Spawn persona ${personaName}`,
        required: true,
        onSuccess: (data) => {
          this.log(`🎓 Persona ${personaName} spawned with ID: ${data.persona_id}`, 'info');
        }
      },
      {
        command: 'academy-train',
        params: {
          student_persona: personaName,
          trainer_mode: trainingConfig.trainerMode,
          evolution_target: trainingConfig.evolutionTarget,
          vector_exploration: true
        },
        description: `Start ${trainingConfig.trainerMode} training`,
        required: true,
        onSuccess: (data) => {
          this.log(`🏃‍♂️ Training session ${data.session_id} started`, 'info');
        }
      }
    ];

    return this.executeCommandSequence(steps, {
      stepDelay: 1000, // 1 second between steps
      onSuccess: (_result) => {
        this.showCommandFeedback(`🎉 Academy training workflow completed for ${personaName}`, 'success');
      },
      onError: (_result) => {
        this.showCommandFeedback(`❌ Academy training workflow failed for ${personaName}`, 'error');
      }
    });
  }

  // Abstract methods for subclasses to implement
  protected abstract onCommandSuccess(command: string, data: any, executionTime: number): void;
  protected abstract onCommandError(command: string, error: string, executionTime: number): void;
  protected abstract setCommandLoadingState(command: string, loading: boolean): void;
  protected abstract showCommandFeedback(message: string, type: 'success' | 'error' | 'info'): void;

  // Utility methods
  private sanitizeParamsForLogging(params: any): any {
    // Remove sensitive data from logging
    const sanitized = { ...params };
    if (sanitized.password) sanitized.password = '[REDACTED]';
    if (sanitized.token) sanitized.token = '[REDACTED]';
    return sanitized;
  }

  // Public API methods
  getCommandHistory(): CommandHistoryEntry[] {
    return [...this.commandHistory];
  }

  getActiveCommands(): string[] {
    return Array.from(this.activeCommands);
  }

  getLastCommandResult(): CommandResult | null {
    return this.lastCommandResult;
  }

  isCommandActive(command: string): boolean {
    return Array.from(this.activeCommands).some(cmd => cmd.startsWith(command));
  }
}

// Supporting interfaces and types

interface CommandOptions {
  showLoading?: boolean;
  successMessage?: string;
  errorMessage?: string;
}

interface SequenceOptions {
  stepDelay?: number;
  onSuccess?: (result: SequenceResult) => void;
  onError?: (result: SequenceResult) => void;
}

interface CommandHistoryEntry {
  command: string;
  params: any;
  timestamp: Date;
  status: 'executing' | 'success' | 'error';
  executionTime?: number;
  result?: CommandResult;
  error?: string;
}

interface SequenceStepResult {
  step: number;
  command: string;
  success: boolean;
  data?: any;
  error?: string;
}

interface SequenceResult {
  success: boolean;
  completedSteps: number;
  totalSteps: number;
  results: SequenceStepResult[];
}

interface PersonaSpawnOptions {
  specialization?: string;
  p2pSeed?: boolean;
  evolutionMode?: 'adversarial' | 'collaborative' | 'autonomous';
  baseModel?: string;
}

interface TrainingOptions {
  trainerMode?: 'adversarial' | 'collaborative' | 'discovery';
  evolutionTarget?: string;
  vectorExploration?: boolean;
  domain?: string;
}

interface StatusOptions {
  detailLevel?: 'summary' | 'detailed' | 'deep_metrics';
  includeP2P?: boolean;
  includeVectorSpace?: boolean;
  includeAdversarial?: boolean;
  personaId?: string;
}

interface AcademyTrainingConfig {
  specialization: string;
  trainerMode: 'adversarial' | 'collaborative' | 'discovery';
  evolutionTarget: string;
}