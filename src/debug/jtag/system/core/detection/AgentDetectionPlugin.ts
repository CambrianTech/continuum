/**
 * Agent Detection Plugin Interface
 * 
 * Modular, extensible system for detecting different types of agents.
 * Each agent type gets its own plugin that can be registered dynamically.
 */

export interface AgentDetectionResult {
  name: string;
  type: 'ai' | 'human' | 'ci' | 'automation' | 'bot' | 'test' | 'unknown';
  version?: string;
  confidence: number; // 0-1
  metadata?: AgentMetadata;
}

export interface AgentMetadata {
  provider?: string;
  entrypoint?: string;
  sessionId?: string;
  model?: string;
  organization?: string;
  platform?: string;
  buildId?: string;
  branch?: string;
  repository?: string;
  terminal?: string;
  shell?: string;
  user?: string;
  tty?: boolean;
  source?: string;
  [key: string]: string | number | boolean | undefined;
}

export interface AgentCapabilities {
  supportsColors: boolean;
  prefersStructuredData: boolean;
  supportsInteractivity: boolean;
  maxOutputLength?: number;
  rateLimit?: {
    requestsPerMinute: number;
    tokensPerRequest?: number;
  };
}

export interface ParticipantProfile {
  canSendMessages: boolean;
  canReceiveMessages: boolean;
  canCreateRooms: boolean;
  canInviteOthers: boolean;
  canModerate: boolean;
  autoResponds: boolean;
  providesContext: boolean;
  trustLevel: 'unknown' | 'low' | 'medium' | 'high' | 'system';
}

/**
 * Base class for agent detection plugins
 */
export abstract class AgentDetectionPlugin {
  abstract readonly name: string;
  abstract readonly priority: number; // Higher = checked first
  
  /**
   * Attempt to detect this agent type
   * @returns Detection result or null if not this agent type
   */
  abstract detect(): AgentDetectionResult | null;
  
  /**
   * Get capabilities for this agent type
   */
  abstract getCapabilities(): AgentCapabilities;
  
  /**
   * Get participant profile for this agent type
   */
  abstract getParticipantProfile(): ParticipantProfile;
  
  /**
   * Get preferred output format
   */
  abstract getOutputFormat(): 'human' | 'ai-friendly' | 'compact' | 'json';
  
  /**
   * Get adapter type for existing chat system integration
   */
  abstract getAdapterType(): 'browser-ui' | 'ai-api' | 'cli' | 'webhook' | 'automation' | 'bot';
  
  /**
   * Optional: Custom connection context for this agent
   */
  getConnectionContext?(): AgentConnectionContext;
  
  /**
   * Optional: Environment-specific setup/teardown
   */
  onDetected?(): void;
  onConnectionEstablished?(): void;
}

/**
 * Strong types for agent connection context
 */
export interface AgentConnectionContext {
  // Enhanced agent information
  agent?: {
    type: 'claude' | 'chatgpt' | 'copilot' | 'codeium' | 'human' | 'ci' | 'system';
    provider: 'anthropic' | 'openai' | 'github' | 'codeium' | 'local' | 'unknown';
    model?: string;
    version?: string;
    confidence: number;
  };

  // Detailed persona information for chat widgets
  persona?: {
    personaId: string;
    displayName: string;
    shortName: string;
    avatar?: string;
    personaType: 'ai-assistant' | 'developer' | 'system' | 'user' | 'bot';
    status?: 'active' | 'thinking' | 'busy' | 'idle';
    preferences: {
      theme: 'light' | 'dark' | 'auto';
      verbosity: 'minimal' | 'normal' | 'detailed';
      responseStyle: 'formal' | 'casual' | 'technical';
    };
  };

  // Session and context
  session?: {
    sessionId: string;
    connectionId: string;
    startedAt: string;
    lastActivity: string;
  };

  // Environment context
  environment?: {
    platform: 'cli' | 'browser' | 'vscode' | 'api' | 'webhook';
    capabilities: string[];
    restrictions: string[];
  };

  // Chat widget specific info
  chatProfile?: {
    canInitiateConversation: boolean;
    canMentionUsers: boolean;
    showTypingIndicators: boolean;
    maxMessageLength: number;
    supportedMessageTypes: ('text' | 'code' | 'image' | 'file' | 'system')[];
  };

  // Chat system compatibility (matching existing ChatTypes)
  participantCapabilities?: {
    canSendMessages: boolean;
    canReceiveMessages: boolean;
    canCreateRooms: boolean;
    canInviteOthers: boolean;
    canModerate: boolean;
    autoResponds: boolean;
    providesContext: boolean;
  };

  // Adapter configuration for chat system
  participantAdapter?: {
    type: 'ai-api' | 'webhook' | 'lora-persona' | 'template' | 'browser-ui';
    config?: AdapterConfigUnion;
    responseStrategy?: {
      triggers: ResponseTrigger[];
      style?: {
        maxLength?: number;
        tone?: string;
        context?: 'message-history' | 'room-context' | 'global' | 'none';
      };
      frequency?: {
        maxPerMinute?: number;
        cooldownMs?: number;
      };
    };
  };

  // Display helpers for UI
  display?: {
    chatLabel: string;
    shortLabel: string;
    statusBadge: string;
    colorTheme: string; // Hex color
  };

  // Legacy contexts for backward compatibility
  anthropic?: {
    model: string;
    capabilities: string[];
    session?: string;
  };

  ci?: {
    platform?: string;
    buildContext?: {
      id?: string;
      branch?: string;
      commit?: string;
    };
  };
}

export interface ResponseTrigger {
  type: 'mention' | 'keyword' | 'question' | 'activity' | 'random' | 'always' | 'never';
  value?: string | string[];
  probability?: number; // 0-1
}

export type AdapterConfigUnion = 
  | AIAdapterConfig
  | WebhookAdapterConfig
  | LoRAAdapterConfig
  | TemplateAdapterConfig
  | BrowserAdapterConfig;

export interface AIAdapterConfig {
  type: 'ai-api';
  provider: 'anthropic' | 'openai' | 'local';
  model?: string;
  apiKey?: string;
  systemPrompt?: string;
  maxTokens?: number;
  temperature?: number;
}

export interface WebhookAdapterConfig {
  type: 'webhook';
  url: string;
  headers?: Record<string, string>;
  timeout?: number;
}

export interface LoRAAdapterConfig {
  type: 'lora-persona';
  personaName: string;
  modelPath: string;
  parameters?: Record<string, unknown>;
}

export interface TemplateAdapterConfig {
  type: 'template';
  template: string;
  variables?: Record<string, string>;
}

export interface BrowserAdapterConfig {
  type: 'browser-ui';
  widgetId?: string;
  theme?: string;
}