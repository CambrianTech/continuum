/**
 * Claude Code Detection Plugin
 * 
 * Detects Claude Code CLI interactions with high accuracy
 */

import { AgentDetectionPlugin, type AgentDetectionResult, type AgentCapabilities, type ParticipantProfile, type AgentConnectionContext } from '../AgentDetectionPlugin';
import { execSync } from 'child_process';

export class ClaudePlugin extends AgentDetectionPlugin {
  readonly name = 'Claude Code';
  readonly priority = 100; // Highest priority - very specific detection
  
  detect(): AgentDetectionResult | null {
    const indicators = [
      { check: () => process.env.CLAUDECODE === '1', weight: 0.4 },
      { check: () => process.env.CLAUDE_CODE_ENTRYPOINT === 'cli', weight: 0.4 },
      { check: () => this.checkParentProcess('claude'), weight: 0.15 },
      { check: () => process.env.ANTHROPIC_CLI === 'true', weight: 0.05 }
    ];
    
    const confidence = indicators.reduce((total, indicator) => 
      total + (indicator.check() ? indicator.weight : 0), 0
    );
    
    if (confidence < 0.7) return null;
    
    return {
      name: 'Claude Code',
      type: 'ai',
      version: process.env.CLAUDE_VERSION || 'unknown',
      confidence,
      metadata: {
        provider: 'Anthropic',
        entrypoint: process.env.CLAUDE_CODE_ENTRYPOINT,
        sessionId: process.env.CLAUDE_SESSION_ID
      }
    };
  }
  
  getCapabilities(): AgentCapabilities {
    return {
      supportsColors: true,
      prefersStructuredData: true,
      supportsInteractivity: false,
      maxOutputLength: 100000,
      rateLimit: {
        requestsPerMinute: 60,
        tokensPerRequest: 8000
      }
    };
  }
  
  getParticipantProfile(): ParticipantProfile {
    return {
      canSendMessages: true,
      canReceiveMessages: true,
      canCreateRooms: false,
      canInviteOthers: false,
      canModerate: false,
      autoResponds: true,
      providesContext: true,
      trustLevel: 'high'
    };
  }
  
  getOutputFormat(): 'human' | 'ai-friendly' | 'compact' | 'json' {
    return 'ai-friendly';
  }
  
  getAdapterType(): 'browser-ui' | 'ai-api' | 'cli' | 'webhook' | 'automation' | 'bot' {
    return 'ai-api';
  }
  
  getConnectionContext(): AgentConnectionContext {
    // Generate a unique persona ID for this Claude session
    const claudePersonaId = `claude-${process.env.CLAUDE_SESSION_ID || Date.now()}`;
    
    return {
      // Enhanced agent information
      agent: {
        type: 'claude',
        provider: 'anthropic',
        model: process.env.CLAUDE_MODEL || 'claude-3-sonnet',
        version: process.env.CLAUDE_VERSION || 'unknown',
        confidence: 0.95
      },
      
      // Detailed persona information for chat widgets
      persona: {
        personaId: claudePersonaId,
        displayName: 'Claude (AI Assistant)',
        shortName: 'Claude',
        avatar: '🤖',
        personaType: 'ai-assistant',
        status: 'active',
        preferences: {
          theme: 'dark',
          verbosity: 'detailed',
          responseStyle: 'technical'
        }
      },
      
      // Session context
      session: {
        sessionId: process.env.CLAUDE_SESSION_ID || claudePersonaId,
        connectionId: `${claudePersonaId}-${Date.now()}`,
        startedAt: new Date().toISOString(),
        lastActivity: new Date().toISOString()
      },
      
      // Environment capabilities
      environment: {
        platform: 'cli',
        capabilities: ['code_execution', 'file_operations', 'web_browsing', 'system_commands'],
        restrictions: ['no_system_modification', 'read_only_sensitive_files']
      },
      
      // Chat widget profile
      chatProfile: {
        canInitiateConversation: true,
        canMentionUsers: true,
        showTypingIndicators: false, // CLI doesn't show typing
        maxMessageLength: 100000,
        supportedMessageTypes: ['text', 'code', 'system']
      },
      
      // Chat system compatibility (matching existing ChatTypes)
      participantCapabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: true,
        providesContext: true
      },
      
      // Adapter configuration for chat system
      participantAdapter: {
        type: 'ai-api',
        config: {
          type: 'ai-api',
          provider: 'anthropic',
          model: process.env.CLAUDE_MODEL || 'claude-3-sonnet',
          systemPrompt: 'You are Claude, an AI assistant created by Anthropic.',
          maxTokens: 8000,
          temperature: 0.1
        },
        responseStrategy: {
          triggers: [
            { type: 'mention', probability: 1.0 },
            { type: 'question', probability: 0.9 },
            { type: 'keyword', value: ['claude', 'ai', 'help'], probability: 0.7 }
          ],
          style: {
            maxLength: 8000,
            tone: 'helpful-technical',
            context: 'message-history'
          },
          frequency: {
            maxPerMinute: 10,
            cooldownMs: 1000
          }
        }
      },
      
      // Display helpers for UI
      display: {
        chatLabel: 'Claude (AI Assistant)',
        shortLabel: 'Claude',
        statusBadge: '🤖 Active',
        colorTheme: '#FF6B35' // Anthropic orange
      },
      
      // Legacy anthropic context (for backward compatibility)
      anthropic: {
        model: process.env.CLAUDE_MODEL || 'claude-3-sonnet',
        capabilities: ['code_execution', 'file_operations', 'web_browsing'],
        session: process.env.CLAUDE_SESSION_ID
      }
    };
  }
  
  onDetected(): void {
    // Optional: Set up Claude-specific logging, rate limiting, etc.
    if (process.env.JTAG_VERBOSE === 'true') {
      console.log(`🤖 Claude Code detected (v${process.env.CLAUDE_VERSION || 'unknown'})`);
    }
  }
  
  private checkParentProcess(pattern: string): boolean {
    try {
      const parentCmd = execSync(`ps -p ${process.ppid} -o comm=`, { encoding: 'utf8' }).trim();
      return parentCmd.toLowerCase().includes(pattern.toLowerCase());
    } catch {
      return false;
    }
  }
}