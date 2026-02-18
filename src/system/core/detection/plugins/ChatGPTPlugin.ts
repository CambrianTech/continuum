/**
 * ChatGPT / OpenAI Detection Plugin
 */

import { AgentDetectionPlugin, type AgentDetectionResult, type AgentCapabilities, type ParticipantProfile } from '../AgentDetectionPlugin';

export class ChatGPTPlugin extends AgentDetectionPlugin {
  readonly name = 'ChatGPT CLI';
  readonly priority = 90;
  
  detect(): AgentDetectionResult | null {
    const indicators = [
      { check: () => !!process.env.OPENAI_API_KEY, weight: 0.3 },
      { check: () => process.env.CHATGPT_CLI === 'true', weight: 0.4 },
      { check: () => process.env.OPENAI_CLI === 'true', weight: 0.4 },
      { check: () => process.argv.some(arg => arg.includes('gpt')), weight: 0.1 }
    ];
    
    const confidence = indicators.reduce((total, indicator) => 
      total + (indicator.check() ? indicator.weight : 0), 0
    );
    
    if (confidence < 0.6) return null;
    
    return {
      name: 'ChatGPT CLI',
      type: 'ai',
      version: process.env.OPENAI_MODEL || 'gpt-4',
      confidence,
      metadata: {
        provider: 'OpenAI',
        model: process.env.OPENAI_MODEL,
        organization: process.env.OPENAI_ORG_ID
      }
    };
  }
  
  getCapabilities(): AgentCapabilities {
    return {
      supportsColors: false,
      prefersStructuredData: true,
      supportsInteractivity: false,
      maxOutputLength: 8000,
      rateLimit: {
        requestsPerMinute: 20,
        tokensPerRequest: 4000
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
}