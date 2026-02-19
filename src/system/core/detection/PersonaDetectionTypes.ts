/**
 * Enhanced Agent and Persona Detection Types
 * 
 * For chat widgets, interaction labels, and persona-based system behavior.
 * Distinguishes between agent type (Claude, ChatGPT) vs persona (individual identity).
 */

export interface PersonaInfo {
  /** Unique identifier for this specific persona instance */
  personaId: string;
  
  /** Display name for chat widgets */
  displayName: string;
  
  /** Short identifier for compact displays */
  shortName: string;
  
  /** Avatar/icon identifier */
  avatar?: string;
  
  /** Persona type for UI styling */
  personaType: 'ai-assistant' | 'developer' | 'system' | 'user' | 'bot';
  
  /** Mood/status for dynamic UI */
  status?: 'active' | 'thinking' | 'busy' | 'idle';
  
  /** Persona preferences */
  preferences: {
    theme: 'light' | 'dark' | 'auto';
    verbosity: 'minimal' | 'normal' | 'detailed';
    responseStyle: 'formal' | 'casual' | 'technical';
  };
}

export interface EnhancedAgentInfo {
  /** Core agent detection info */
  agent: {
    type: 'claude' | 'chatgpt' | 'copilot' | 'codeium' | 'human' | 'ci' | 'system';
    provider: 'anthropic' | 'openai' | 'github' | 'codeium' | 'local' | 'unknown';
    model?: string;
    version?: string;
    confidence: number;
  };
  
  /** Individual persona within agent type */
  persona: PersonaInfo;
  
  /** Session and context */
  session: {
    sessionId: string;
    connectionId: string;
    startedAt: string;
    lastActivity: string;
  };
  
  /** Environment context */
  environment: {
    platform: 'cli' | 'browser' | 'vscode' | 'api' | 'webhook';
    capabilities: string[];
    restrictions: string[];
  };
  
  /** Chat widget specific info */
  chatProfile: {
    canInitiateConversation: boolean;
    canMentionUsers: boolean;
    showTypingIndicators: boolean;
    maxMessageLength: number;
    supportedMessageTypes: ('text' | 'code' | 'image' | 'file' | 'system')[];
  };
}

/**
 * Extended agent detection result with persona information
 */
export interface PersonaDetectionResult {
  /** Basic agent info */
  agentInfo: EnhancedAgentInfo;
  
  /** Connection context for JTAG */
  connectionContext: Record<string, any>;
  
  /** UI display helpers */
  display: {
    chatLabel: string;        // "Claude (AI Assistant)"
    shortLabel: string;       // "Claude"
    statusBadge: string;      // "🤖 Active"
    colorTheme: string;       // CSS color for UI theming
  };
}