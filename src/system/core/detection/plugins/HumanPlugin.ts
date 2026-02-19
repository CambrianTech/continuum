/**
 * Human User Detection Plugin
 * 
 * Detects human terminal users (fallback with lowest priority)
 */

import { AgentDetectionPlugin, type AgentDetectionResult, type AgentCapabilities, type ParticipantProfile } from '../AgentDetectionPlugin';

export class HumanPlugin extends AgentDetectionPlugin {
  readonly name = 'Human Terminal User';
  readonly priority = 1; // Lowest priority - fallback
  
  detect(): AgentDetectionResult | null {
    // Always returns a result as fallback
    const terminalFeatures = [
      process.stdout.isTTY,
      !!process.env.TERM,
      !!process.env.SHELL,
      !!process.env.USER || !!process.env.USERNAME
    ];
    
    const confidence = terminalFeatures.filter(Boolean).length / terminalFeatures.length;
    
    return {
      name: 'Human Terminal User',
      type: 'human',
      confidence: Math.max(0.3, confidence * 0.8), // Cap at 0.8 since this is fallback
      metadata: {
        terminal: process.env.TERM,
        shell: process.env.SHELL,
        user: process.env.USER || process.env.USERNAME,
        tty: process.stdout.isTTY
      }
    };
  }
  
  getCapabilities(): AgentCapabilities {
    const supportsColors = process.stdout.isTTY && (
      !!process.env.COLORTERM ||
      process.env.TERM?.includes('color') ||
      process.env.TERM === 'xterm-256color'
    );
    
    return {
      supportsColors,
      prefersStructuredData: false,
      supportsInteractivity: true,
      maxOutputLength: undefined,
      rateLimit: {
        requestsPerMinute: 1000 // Humans are fast but not that fast
      }
    };
  }
  
  getParticipantProfile(): ParticipantProfile {
    return {
      canSendMessages: true,
      canReceiveMessages: true,
      canCreateRooms: true,
      canInviteOthers: true,
      canModerate: true,
      autoResponds: false,
      providesContext: false,
      trustLevel: 'medium' // Humans can be trusted but make mistakes
    };
  }
  
  getOutputFormat(): 'human' | 'ai-friendly' | 'compact' | 'json' {
    return 'human';
  }
  
  getAdapterType(): 'browser-ui' | 'ai-api' | 'cli' | 'webhook' | 'automation' | 'bot' {
    return process.stdout.isTTY ? 'cli' : 'browser-ui';
  }
}