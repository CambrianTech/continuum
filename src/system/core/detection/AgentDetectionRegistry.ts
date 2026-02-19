/**
 * Agent Detection Registry
 * 
 * Modular, extensible registry for agent detection plugins.
 * New agent types can be registered dynamically at runtime.
 */

import { AgentDetectionPlugin, type AgentDetectionResult, type AgentConnectionContext } from './AgentDetectionPlugin';

// Built-in plugins
import { ClaudePlugin } from './plugins/ClaudePlugin';
import { ChatGPTPlugin } from './plugins/ChatGPTPlugin';
import { CIPlugin } from './plugins/CIPlugin';
import { HumanPlugin } from './plugins/HumanPlugin';

export interface CompleteAgentInfo {
  detection: AgentDetectionResult;
  capabilities: ReturnType<AgentDetectionPlugin['getCapabilities']>;
  participantProfile: ReturnType<AgentDetectionPlugin['getParticipantProfile']>;
  outputFormat: ReturnType<AgentDetectionPlugin['getOutputFormat']>;
  adapterType: ReturnType<AgentDetectionPlugin['getAdapterType']>;
  connectionContext?: AgentConnectionContext;
  plugin: AgentDetectionPlugin;
}

export interface JTAGConnectionContext extends AgentConnectionContext {
  // JTAG-specific context fields
  agentInfo: {
    name: string;
    type: string;
    version?: string;
    confidence: number;
    metadata?: any;
    plugin: string;
    detected: boolean;
  };
  adapterType: string;
  capabilities: ReturnType<AgentDetectionPlugin['getParticipantProfile']>;
  outputPreferences: {
    format: string;
    supportsColors: boolean;
    maxLength?: number;
    rateLimit?: {
      requestsPerMinute: number;
      tokensPerRequest?: number;
    };
  };
}

export class AgentDetectionRegistry {
  private plugins: AgentDetectionPlugin[] = [];
  private cachedDetection: CompleteAgentInfo | null = null;
  
  constructor() {
    // Register built-in plugins
    this.registerDefaultPlugins();
  }
  
  /**
   * Register a new agent detection plugin
   */
  register(plugin: AgentDetectionPlugin): void {
    // Insert in priority order (highest priority first)
    const index = this.plugins.findIndex(p => p.priority < plugin.priority);
    if (index === -1) {
      this.plugins.push(plugin);
    } else {
      this.plugins.splice(index, 0, plugin);
    }
    
    // Clear cache when new plugin is added
    this.cachedDetection = null;
  }
  
  /**
   * Unregister a plugin by name
   */
  unregister(name: string): boolean {
    const index = this.plugins.findIndex(p => p.name === name);
    if (index !== -1) {
      this.plugins.splice(index, 1);
      this.cachedDetection = null;
      return true;
    }
    return false;
  }
  
  /**
   * Get all registered plugins
   */
  getPlugins(): readonly AgentDetectionPlugin[] {
    return [...this.plugins];
  }
  
  /**
   * Detect current agent using all registered plugins
   */
  detect(): CompleteAgentInfo {
    // Return cached result if available
    if (this.cachedDetection) {
      return this.cachedDetection;
    }
    
    // Try each plugin in priority order
    for (const plugin of this.plugins) {
      const detection = plugin.detect();
      if (detection && detection.confidence > 0.5) {
        const result: CompleteAgentInfo = {
          detection,
          capabilities: plugin.getCapabilities(),
          participantProfile: plugin.getParticipantProfile(),
          outputFormat: plugin.getOutputFormat(),
          adapterType: plugin.getAdapterType(),
          connectionContext: plugin.getConnectionContext?.(),
          plugin
        };
        
        // Trigger plugin lifecycle
        plugin.onDetected?.();
        
        // Cache result
        this.cachedDetection = result;
        return result;
      }
    }
    
    // Fallback to human plugin (should always have a result)
    const humanPlugin = this.plugins.find(p => p instanceof HumanPlugin);
    if (humanPlugin) {
      const detection = humanPlugin.detect()!;
      const result: CompleteAgentInfo = {
        detection,
        capabilities: humanPlugin.getCapabilities(),
        participantProfile: humanPlugin.getParticipantProfile(),
        outputFormat: humanPlugin.getOutputFormat(),
        adapterType: humanPlugin.getAdapterType(),
        connectionContext: humanPlugin.getConnectionContext?.(),
        plugin: humanPlugin
      };
      
      this.cachedDetection = result;
      return result;
    }
    
    throw new Error('No agent detection plugins available');
  }
  
  /**
   * Force re-detection (clears cache)
   */
  redetect(): CompleteAgentInfo {
    this.cachedDetection = null;
    return this.detect();
  }
  
  /**
   * Create connection context for JTAG system
   * 
   * AUTO-DETECTION PATTERN:
   * - No parameters: Automatically detects Claude, ChatGPT, Human, CI, etc.
   * - With overrides: Uses explicit values for testing/impersonation
   * - Partial overrides: Merges with auto-detection
   * 
   * @param overrides Optional explicit overrides for impersonation/testing
   * @returns Rich context with agent type, persona info, and capabilities
   */
  createConnectionContext(overrides?: Partial<CompleteAgentInfo>): JTAGConnectionContext {
    const agent = overrides ? { ...this.detect(), ...overrides } : this.detect();
    
    return {
      agentInfo: {
        name: agent.detection.name,
        type: agent.detection.type,
        version: agent.detection.version,
        confidence: agent.detection.confidence,
        metadata: agent.detection.metadata,
        plugin: agent.plugin.name,
        detected: !overrides
      },
      adapterType: agent.adapterType,
      capabilities: agent.participantProfile,
      outputPreferences: {
        format: agent.outputFormat,
        supportsColors: agent.capabilities.supportsColors,
        maxLength: agent.capabilities.maxOutputLength,
        rateLimit: agent.capabilities.rateLimit
      },
      ...agent.connectionContext
    };
  }
  
  /**
   * Register built-in plugins
   */
  private registerDefaultPlugins(): void {
    this.register(new ClaudePlugin());
    this.register(new ChatGPTPlugin());
    this.register(new CIPlugin());
    this.register(new HumanPlugin());
  }
  
  /**
   * Get agent display name
   */
  getAgentName(): string {
    const agent = this.detect();
    return `${agent.detection.name} (${Math.round(agent.detection.confidence * 100)}% confidence)`;
  }
  
  /**
   * Check if current agent is AI
   */
  isAI(): boolean {
    return this.detect().detection.type === 'ai';
  }
  
  /**
   * Get output format for current agent
   */
  getOutputFormat(): 'human' | 'ai-friendly' | 'compact' | 'json' {
    return this.detect().outputFormat;
  }
}

// Global registry instance
export const agentDetection = new AgentDetectionRegistry();

// Convenience exports
export const detectAgent = () => agentDetection.detect();
export const isAI = () => agentDetection.isAI();
export const getOutputFormat = () => agentDetection.getOutputFormat();
export const getAgentName = () => agentDetection.getAgentName();