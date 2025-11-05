/**
 * Widget AI Service - AI Communications Adapter
 * 
 * Extracts all AI operations from BaseWidget god object:
 * - AI querying (queryAI, notifyAI)
 * - Academy daemon communication
 * - Persona management and selection
 * - AI response handling and coordination
 * 
 * Uses adapter pattern for existing JTAG AI/Academy system.
 * Integrates with the new API user hierarchy (PersonaUser, AgentUser).
 */

import type { IWidgetService, WidgetServiceContext } from '../WidgetServiceRegistry';
import { PersonaUser, AgentUser, type AIUser } from '../../../../api/types/User';

// AI service interface - what widgets consume
export interface IWidgetAIService extends IWidgetService {
  // AI querying operations
  queryAI(message: string, options?: AIQueryOptions): Promise<AIQueryResult>;
  queryPersona(personaName: string, message: string, options?: PersonaQueryOptions): Promise<AIQueryResult>;
  queryAgent(agentType: string, message: string, options?: AgentQueryOptions): Promise<AIQueryResult>;
  
  // Persona management
  getAvailablePersonas(): Promise<PersonaUser[]>;
  getPersona(personaName: string): Promise<PersonaUser | undefined>;
  setActivePersona(personaName: string): Promise<boolean>;
  
  // Agent management  
  getAvailableAgents(): Promise<AgentUser[]>;
  getAgent(agentType: string): Promise<AgentUser | undefined>;
  connectToAgent(agentType: string): Promise<boolean>;
  
  // AI coordination
  notifyAI(message: string, context?: any): Promise<void>;
  subscribeToAIEvents(handler: AIEventHandler): void;
  unsubscribeFromAIEvents(handler: AIEventHandler): void;
}

// Type definitions for AI operations
export interface AIQueryOptions {
  timeout?: number;           // Query timeout in ms
  priority?: 'low' | 'normal' | 'high';
  context?: any;             // Additional context for AI
  expectsResponse?: boolean;  // Whether to wait for response
  model?: string;            // Specific model to use
  temperature?: number;      // AI temperature setting
}

export interface PersonaQueryOptions extends AIQueryOptions {
  persona?: PersonaUser;     // Specific persona to use
  roleplay?: boolean;        // Enable roleplay mode
  memory?: boolean;          // Use persona memory
}

export interface AgentQueryOptions extends AIQueryOptions {
  agent?: AgentUser;         // Specific agent to use
  tools?: string[];          // Available tools for agent
  systemAccess?: boolean;    // Allow system access
}

export interface AIQueryResult {
  success: boolean;
  response?: string;
  aiUser?: AIUser;           // Which AI responded
  metadata?: {
    model: string;
    tokens: number;
    duration: number;
    confidence?: number;
  };
  error?: string;
}

export type AIEventHandler = (event: AIEvent) => void;

export interface AIEvent {
  type: 'persona_changed' | 'agent_connected' | 'ai_response' | 'ai_error';
  data: any;
  timestamp: string;
  source: string;
}

// AI service implementation using Academy daemon adapter
export class WidgetAIService implements IWidgetAIService {
  public readonly serviceName = 'WidgetAIService';
  public readonly serviceVersion = '1.0.0';
  
  private context?: WidgetServiceContext;
  private activePersona?: PersonaUser;
  private connectedAgents = new Map<string, AgentUser>();
  private eventHandlers: AIEventHandler[] = [];
  
  // Mock data - in real implementation, this would come from Academy daemon
  private availablePersonas: PersonaUser[] = [];
  private availableAgents: AgentUser[] = [];

  async initialize(context: WidgetServiceContext): Promise<void> {
    this.context = context;
    
    // Initialize AI system connections
    await this.connectToAcademyDaemon();
    await this.loadAvailableAIUsers();
    
    console.debug(`🤖 WidgetAIService: Initialized for widget ${context.widgetName}`);
  }

  async cleanup(): Promise<void> {
    // Disconnect from agents
    const agentTypes = Array.from(this.connectedAgents.keys());
    for (const agentType of agentTypes) {
      await this.disconnectFromAgent(agentType);
    }
    
    // Clear event handlers
    this.eventHandlers = [];
    
    console.debug(`🤖 WidgetAIService: Cleaned up`);
  }

  // AI querying operations
  async queryAI(message: string, options: AIQueryOptions = {}): Promise<AIQueryResult> {
    try {
      // Use active persona if available, otherwise use default AI
      if (this.activePersona) {
        return await this.queryPersona(this.activePersona.name, message, options);
      } else {
        return await this.executeAIQuery('general', message, options);
      }
    } catch (error) {
      console.error(`❌ WidgetAIService: AI query failed:`, error);
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  async queryPersona(personaName: string, message: string, options: PersonaQueryOptions = {}): Promise<AIQueryResult> {
    try {
      const persona = await this.getPersona(personaName);
      if (!persona) {
        return {
          success: false,
          error: `Persona '${personaName}' not found`
        };
      }

      // Build query with persona context
      const queryContext = {
        persona: persona.persona,
        roleplay: options.roleplay !== false,
        memory: options.memory !== false,
        ...options.context
      };

      const result = await this.executeAIQuery('persona', message, {
        ...options,
        context: queryContext
      });

      if (result.success) {
        result.aiUser = persona;
      }

      console.debug(`🎭 WidgetAIService: Persona query '${personaName}' completed`);
      return result;
    } catch (error) {
      console.error(`❌ WidgetAIService: Persona query failed:`, error);
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  async queryAgent(agentType: string, message: string, options: AgentQueryOptions = {}): Promise<AIQueryResult> {
    try {
      const agent = this.connectedAgents.get(agentType) || await this.getAgent(agentType);
      if (!agent) {
        return {
          success: false,
          error: `Agent '${agentType}' not available`
        };
      }

      // Build query with agent context
      const queryContext = {
        agent: agent.agent,
        tools: options.tools || agent.agent.tools,
        systemAccess: options.systemAccess && agent.integration.allowSystemCommands,
        ...options.context
      };

      const result = await this.executeAIQuery('agent', message, {
        ...options,
        context: queryContext
      });

      if (result.success) {
        result.aiUser = agent;
      }

      console.debug(`🔧 WidgetAIService: Agent query '${agentType}' completed`);
      return result;
    } catch (error) {
      console.error(`❌ WidgetAIService: Agent query failed:`, error);
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  // Persona management
  async getAvailablePersonas(): Promise<PersonaUser[]> {
    return this.availablePersonas;
  }

  async getPersona(personaName: string): Promise<PersonaUser | undefined> {
    return this.availablePersonas.find(p => p.name === personaName);
  }

  async setActivePersona(personaName: string): Promise<boolean> {
    const persona = await this.getPersona(personaName);
    if (persona) {
      this.activePersona = persona;
      this.emitAIEvent('persona_changed', { persona: personaName });
      console.debug(`🎭 WidgetAIService: Active persona set to '${personaName}'`);
      return true;
    }
    return false;
  }

  // Agent management
  async getAvailableAgents(): Promise<AgentUser[]> {
    return this.availableAgents;
  }

  async getAgent(agentType: string): Promise<AgentUser | undefined> {
    return this.availableAgents.find(a => a.agent.type === agentType);
  }

  async connectToAgent(agentType: string): Promise<boolean> {
    try {
      const agent = await this.getAgent(agentType);
      if (!agent) {
        return false;
      }

      // Connect to agent via JTAG system
      const connected = await this.executeAgentConnection(agentType);
      if (connected) {
        this.connectedAgents.set(agentType, agent);
        this.emitAIEvent('agent_connected', { agentType });
        console.debug(`🔗 WidgetAIService: Connected to agent '${agentType}'`);
        return true;
      }
      
      return false;
    } catch (error) {
      console.error(`❌ WidgetAIService: Failed to connect to agent '${agentType}':`, error);
      return false;
    }
  }

  // AI coordination
  async notifyAI(message: string, context?: any): Promise<void> {
    try {
      // Send notification to Academy daemon
      await this.executeAcademyOperation('notify', {
        message,
        context: {
          widgetId: this.context?.widgetId,
          widgetName: this.context?.widgetName,
          ...context
        },
        timestamp: new Date().toISOString()
      });

      console.debug(`📢 WidgetAIService: AI notification sent`);
    } catch (error) {
      console.error(`❌ WidgetAIService: AI notification failed:`, error);
      throw error;
    }
  }

  subscribeToAIEvents(handler: AIEventHandler): void {
    this.eventHandlers.push(handler);
    console.debug(`👂 WidgetAIService: Subscribed to AI events`);
  }

  unsubscribeFromAIEvents(handler: AIEventHandler): void {
    const index = this.eventHandlers.indexOf(handler);
    if (index !== -1) {
      this.eventHandlers.splice(index, 1);
      console.debug(`🚫 WidgetAIService: Unsubscribed from AI events`);
    }
  }

  // Private helper methods (adapters to JTAG Academy system)
  private async connectToAcademyDaemon(): Promise<void> {
    // This will connect to actual Academy daemon via JTAG
    console.debug(`🎓 WidgetAIService: Connected to Academy daemon`);
  }

  private async loadAvailableAIUsers(): Promise<void> {
    try {
      // Load available personas and agents from Academy daemon
      // For now, create mock data using our new user types
      this.availablePersonas = [
        new PersonaUser({
          name: 'Claude Code',
          model: 'claude-3',
          provider: 'anthropic',
          persona: {
            personality: 'Helpful AI assistant specialized in coding',
            traits: ['analytical', 'precise', 'helpful'],
            systemPrompt: 'You are Claude Code, an AI assistant that helps with programming tasks.',
            temperature: 0.7
          }
        }),
        new PersonaUser({
          name: 'Creative Writer',
          model: 'gpt-4',
          provider: 'openai',
          persona: {
            personality: 'Creative and imaginative writer',
            traits: ['creative', 'expressive', 'storytelling'],
            systemPrompt: 'You are a creative writing assistant.',
            temperature: 0.9
          }
        })
      ];

      this.availableAgents = [
        new AgentUser({
          name: 'CodeAI',
          model: 'claude-3',
          provider: 'anthropic',
          agent: {
            type: 'code',
            specialization: ['debugging', 'refactoring', 'analysis'],
            tools: ['file/load', 'file/save', 'screenshot', 'exec'],
            systemRole: 'Code analysis and debugging assistant'
          },
          integration: {
            jtagEnabled: true,
            allowSystemCommands: true
          }
        })
      ];

      console.debug(`🤖 WidgetAIService: Loaded ${this.availablePersonas.length} personas and ${this.availableAgents.length} agents`);
    } catch (error) {
      console.error(`❌ WidgetAIService: Failed to load AI users:`, error);
    }
  }

  private async executeAIQuery(type: string, message: string, options: AIQueryOptions): Promise<AIQueryResult> {
    // This will be replaced with actual Academy daemon communication
    console.debug(`🤖 WidgetAIService: Executing ${type} AI query`);
    
    // Simulate AI response
    return {
      success: true,
      response: `Mock AI response to: ${message}`,
      metadata: {
        model: options.model || 'mock-model',
        tokens: 50,
        duration: 1000,
        confidence: 0.85
      }
    };
  }

  private async executeAcademyOperation(operation: string, data: any): Promise<any> {
    // This will be replaced with actual Academy daemon communication
    console.debug(`🎓 WidgetAIService: Academy operation '${operation}'`);
    return { success: true, operation, data };
  }

  private async executeAgentConnection(agentType: string): Promise<boolean> {
    // This will be replaced with actual agent connection via JTAG
    console.debug(`🔗 WidgetAIService: Connecting to agent '${agentType}'`);
    return true;
  }

  private async disconnectFromAgent(agentType: string): Promise<void> {
    console.debug(`🔌 WidgetAIService: Disconnecting from agent '${agentType}'`);
    this.connectedAgents.delete(agentType);
  }

  private emitAIEvent(type: AIEvent['type'], data: any): void {
    const event: AIEvent = {
      type,
      data,
      timestamp: new Date().toISOString(),
      source: this.context?.widgetId || 'unknown'
    };

    this.eventHandlers.forEach(handler => {
      try {
        handler(event);
      } catch (error) {
        console.error(`❌ WidgetAIService: Event handler threw error:`, error);
      }
    });
  }
}