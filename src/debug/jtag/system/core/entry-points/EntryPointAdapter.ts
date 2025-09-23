/**
 * Entry Point Adapter - Intelligent CLI behavior based on agent detection
 * 
 * Provides different experiences for:
 * - AI Agents (Claude, ChatGPT): Structured output, minimal friction
 * - Human Developers: Helpful output, progress indicators  
 * - CI Systems: Compact output, machine-readable results
 */

import { agentDetection, type JTAGConnectionContext } from '../detection/AgentDetectionRegistry';

export interface EntryPointOptions {
  verbose?: boolean;
  quiet?: boolean;
  format?: 'auto' | 'human' | 'ai-friendly' | 'compact' | 'json';
  showAgentInfo?: boolean;
}

export interface EntryPointBehavior {
  shouldShowAgentInfo: boolean;
  shouldShowProgressIndicators: boolean;
  shouldSuppressVerboseLogs: boolean;
  outputFormat: 'human' | 'ai-friendly' | 'compact' | 'json';
  logLevel: 'minimal' | 'normal' | 'verbose';
  showTimestamps: boolean;
  showCorrelationIds: boolean;
}

export class EntryPointAdapter {
  private agentContext: JTAGConnectionContext;
  private behavior: EntryPointBehavior;

  constructor(options: EntryPointOptions = {}) {
    this.agentContext = agentDetection.createConnectionContext();
    this.behavior = this.determineBehavior(options);
  }

  /**
   * Determine intelligent behavior based on detected agent type
   */
  private determineBehavior(options: EntryPointOptions): EntryPointBehavior {
    const agent = this.agentContext;
    const agentType = agent.agentInfo.type;
    const outputFormat = options.format === 'auto' 
      ? agent.outputPreferences?.format || 'human'
      : options.format || agent.outputPreferences?.format || 'human';

    // Base behavior by agent type
    let behavior: EntryPointBehavior;

    switch (agentType) {
      case 'ai':
        behavior = {
          shouldShowAgentInfo: options.showAgentInfo ?? false, // AI agents know who they are
          shouldShowProgressIndicators: false, // AI doesn't need progress bars
          shouldSuppressVerboseLogs: true, // AI wants clean output
          outputFormat: outputFormat as any,
          logLevel: 'minimal',
          showTimestamps: false, // AI doesn't care about timestamps
          showCorrelationIds: false // AI doesn't need correlation debugging
        };
        break;

      case 'ci':
        behavior = {
          shouldShowAgentInfo: options.showAgentInfo ?? true, // CI should log what's running
          shouldShowProgressIndicators: false, // CI wants machine-readable
          shouldSuppressVerboseLogs: false, // CI wants full logs for debugging
          outputFormat: 'compact',
          logLevel: 'normal',
          showTimestamps: true, // CI needs timestamps for debugging
          showCorrelationIds: true // CI needs full traceability
        };
        break;

      case 'human':
      default:
        behavior = {
          shouldShowAgentInfo: options.showAgentInfo ?? false, // Clean output by default
          shouldShowProgressIndicators: false, // Clean output by default
          shouldSuppressVerboseLogs: true, // Quiet by default for everyone
          outputFormat: outputFormat as any,
          logLevel: 'minimal', // Quiet by default
          showTimestamps: false,
          showCorrelationIds: false
        };
        break;
    }

    // Apply explicit overrides
    if (options.verbose) {
      behavior.logLevel = 'verbose';
      behavior.shouldSuppressVerboseLogs = false;
    }
    if (options.quiet) {
      behavior.logLevel = 'minimal';
      behavior.shouldShowProgressIndicators = false;
      behavior.shouldShowAgentInfo = false;
    }

    return behavior;
  }

  /**
   * Get the agent context
   */
  getAgentContext(): JTAGConnectionContext {
    return this.agentContext;
  }

  /**
   * Get the determined behavior
   */
  getBehavior(): EntryPointBehavior {
    return this.behavior;
  }

  /**
   * Log agent detection info if appropriate
   */
  logAgentDetection(): void {
    if (!this.behavior.shouldShowAgentInfo) return;

    const agent = this.agentContext;
    const agentType = agent.agentInfo.type;

    if (agentType === 'ai') {
      // Minimal info for AI agents
      console.log(`🎭 AI Agent: ${agent.persona?.shortName || agent.agentInfo.name}`);
    } else if (agentType === 'ci') {
      // Full info for CI systems
      console.log(`🔄 CI System: ${agent.agentInfo.name} (${Math.round(agent.agentInfo.confidence * 100)}%)`);
      if (agent.ci?.platform) {
        console.log(`   Platform: ${agent.ci.platform}`);
      }
    } else {
      // Detailed info for humans
      console.log(`🎭 AGENT DETECTION: ${agent.agentInfo.name} (${agent.agentInfo.type})`);
      console.log(`   Confidence: ${Math.round(agent.agentInfo.confidence * 100)}%`);
      console.log(`   Plugin: ${agent.agentInfo.plugin}`);
      console.log(`   Output Format: ${agent.outputPreferences?.format}`);
      if (agent.persona) {
        console.log(`   Persona: ${agent.persona.displayName}`);
        console.log(`   Chat Label: ${agent.display?.chatLabel}`);
      }
    }
    console.log('');
  }

  /**
   * Format output based on agent preferences
   */
  formatOutput(data: any): string {
    switch (this.behavior.outputFormat) {
      case 'json':
        return JSON.stringify(data, null, 2);
      
      case 'compact':
        if (typeof data === 'object' && data.success !== undefined) {
          return `${data.success ? '✅' : '❌'} ${data.command || 'command'}${data.error ? `: ${data.error}` : ''}`;
        }
        return JSON.stringify(data);
      
      case 'ai-friendly':
        if (typeof data === 'object' && data.success !== undefined) {
          const result = [
            `Status: ${data.success ? 'SUCCESS' : 'FAILED'}`,
          ];
          if (data.filepath) result.push(`File: ${data.filepath}`);
          if (data.commands?.length) result.push(`Commands: ${data.commands.length}`);
          if (data.error) result.push(`Error: ${data.error}`);
          if (data.timestamp) result.push(`Time: ${data.timestamp}`);
          return result.join('\n');
        }
        return JSON.stringify(data, null, 2);
      
      case 'human':
      default:
        return JSON.stringify(data, null, 2);
    }
  }

  /**
   * Should show verbose logs based on behavior
   */
  shouldShowVerboseLogs(): boolean {
    return !this.behavior.shouldSuppressVerboseLogs;
  }

  /**
   * Should show progress indicators
   */
  shouldShowProgress(): boolean {
    return this.behavior.shouldShowProgressIndicators;
  }

  /**
   * Get log level
   */
  getLogLevel(): 'minimal' | 'normal' | 'verbose' {
    return this.behavior.logLevel;
  }
}