/**
 * Universal Agent Detection System
 * 
 * Like navigator.userAgent for AI systems - detects who is interacting
 * and provides appropriate output formatting and capabilities.
 */

export interface AgentInfo {
  name: string;
  type: 'ai' | 'human' | 'ci' | 'automation' | 'unknown';
  version?: string;
  capabilities: {
    supportsColors: boolean;
    prefersStructuredData: boolean;
    supportsInteractivity: boolean;
    maxOutputLength?: number;
  };
  outputFormat: 'human' | 'ai-friendly' | 'compact' | 'json';
  confidence: number; // 0-1 how confident we are in detection
  // Compatibility with existing ParticipantAdapter system
  adapterType: 'browser-ui' | 'ai-api' | 'cli' | 'webhook' | 'automation';
  participantCapabilities: {
    canSendMessages: boolean;
    canReceiveMessages: boolean;
    canCreateRooms: boolean;
    canInviteOthers: boolean;
    canModerate: boolean;
    autoResponds: boolean;
    providesContext: boolean;
  };
}

export class AgentDetector {
  
  /**
   * Main detection method - like navigator.userAgent but for AI systems
   */
  static detect(): AgentInfo {
    // Try different detection methods in order of confidence
    const detectors = [
      this.detectClaude.bind(this),
      this.detectChatGPT.bind(this),
      this.detectGitHubCopilot.bind(this),
      this.detectCI.bind(this),
      this.detectAutomation.bind(this),
      this.detectHuman.bind(this)
    ];

    for (const detector of detectors) {
      const result = detector();
      if (result.confidence > 0.7) {
        return result;
      }
    }

    // Fallback to best guess
    return this.detectHuman();
  }

  /**
   * Claude Code detection
   */
  private static detectClaude(): AgentInfo {
    const indicators = [
      process.env.CLAUDECODE === '1',
      process.env.CLAUDE_CODE_ENTRYPOINT === 'cli',
      process.env.ANTHROPIC_CLI === 'true',
      AgentDetector.checkParentProcess('claude')
    ];

    const matches = indicators.filter(Boolean).length;
    let confidence = matches / indicators.length;

    // CLAUDECODE=1 alone is sufficient for detection
    if (process.env.CLAUDECODE === '1') {
      // If we have 2+ indicators, confidence is high
      confidence = matches >= 2 ? 0.95 : 0.75;
    }

    if (confidence < 0.5) {
      return this.createUnknownAgent();
    }
    
    return {
      name: 'Claude Code',
      type: 'ai',
      version: process.env.CLAUDE_VERSION || 'unknown',
      capabilities: {
        supportsColors: true,
        prefersStructuredData: true,
        supportsInteractivity: false,
        maxOutputLength: 100000
      },
      outputFormat: 'ai-friendly',
      confidence,
      adapterType: 'ai-api',
      participantCapabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: true,
        providesContext: true
      }
    };
  }

  /**
   * ChatGPT / OpenAI detection
   */
  private static detectChatGPT(): AgentInfo {
    const indicators = [
      process.env.OPENAI_API_KEY !== undefined,
      process.env.CHATGPT_CLI === 'true',
      process.env.OPENAI_CLI === 'true',
      AgentDetector.checkParentProcess('gpt'),
      AgentDetector.checkParentProcess('openai')
    ];
    
    const matches = indicators.filter(Boolean).length;
    const confidence = matches / indicators.length;
    
    if (confidence < 0.5) {
      return this.createUnknownAgent();
    }
    
    return {
      name: 'ChatGPT CLI',
      type: 'ai',
      capabilities: {
        supportsColors: false,
        prefersStructuredData: true,
        supportsInteractivity: false,
        maxOutputLength: 8000
      },
      outputFormat: 'ai-friendly',
      confidence,
      adapterType: 'ai-api',
      participantCapabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: true,
        providesContext: true
      }
    };
  }

  /**
   * GitHub Copilot detection
   */
  private static detectGitHubCopilot(): AgentInfo {
    const indicators = [
      process.env.GITHUB_COPILOT === 'true',
      process.env.VSCODE_PID !== undefined && process.env.TERM_PROGRAM === 'vscode',
      AgentDetector.checkParentProcess('copilot')
    ];
    
    const matches = indicators.filter(Boolean).length;
    const confidence = matches / indicators.length;
    
    if (confidence < 0.3) {
      return this.createUnknownAgent();
    }
    
    return {
      name: 'GitHub Copilot',
      type: 'ai',
      capabilities: {
        supportsColors: false,
        prefersStructuredData: true,
        supportsInteractivity: false,
        maxOutputLength: 2000
      },
      outputFormat: 'compact',
      confidence,
      adapterType: 'ai-api',
      participantCapabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: true,
        providesContext: true
      }
    };
  }

  /**
   * CI/CD system detection
   */
  private static detectCI(): AgentInfo {
    const ciSystems = [
      { env: 'GITHUB_ACTIONS', name: 'GitHub Actions' },
      { env: 'GITLAB_CI', name: 'GitLab CI' },
      { env: 'JENKINS_URL', name: 'Jenkins' },
      { env: 'TRAVIS', name: 'Travis CI' },
      { env: 'CIRCLECI', name: 'CircleCI' },
      { env: 'BUILDKITE', name: 'Buildkite' }
    ];
    
    for (const ci of ciSystems) {
      if (process.env[ci.env]) {
        return {
          name: ci.name,
          type: 'ci',
          capabilities: {
            supportsColors: false,
            prefersStructuredData: false,
            supportsInteractivity: false,
            maxOutputLength: 50000
          },
          outputFormat: 'compact',
          confidence: 0.9,
          adapterType: 'automation',
          participantCapabilities: {
            canSendMessages: false,
            canReceiveMessages: false,
            canCreateRooms: false,
            canInviteOthers: false,
            canModerate: false,
            autoResponds: false,
            providesContext: false
          }
        };
      }
    }
    
    if (process.env.CI === 'true') {
      return {
        name: 'Generic CI',
        type: 'ci',
        capabilities: {
          supportsColors: false,
          prefersStructuredData: false,
          supportsInteractivity: false
        },
        outputFormat: 'compact',
        confidence: 0.8,
        adapterType: 'automation',
        participantCapabilities: {
          canSendMessages: false,
          canReceiveMessages: false,
          canCreateRooms: false,
          canInviteOthers: false,
          canModerate: false,
          autoResponds: false,
          providesContext: false
        }
      };
    }
    
    return this.createUnknownAgent();
  }

  /**
   * Automation/scripting detection
   */
  private static detectAutomation(): AgentInfo {
    const indicators = [
      !process.stdout.isTTY,
      process.env.AUTOMATED === 'true',
      process.env.HEADLESS === 'true',
      process.argv.some(arg => arg.includes('--json') || arg.includes('--quiet'))
    ];
    
    const matches = indicators.filter(Boolean).length;
    
    if (matches >= 2) {
      return {
        name: 'Automation Script',
        type: 'automation',
        capabilities: {
          supportsColors: false,
          prefersStructuredData: true,
          supportsInteractivity: false
        },
        outputFormat: 'json',
        confidence: 0.7,
        adapterType: 'automation',
        participantCapabilities: {
          canSendMessages: false,
          canReceiveMessages: false,
          canCreateRooms: false,
          canInviteOthers: false,
          canModerate: false,
          autoResponds: false,
          providesContext: false
        }
      };
    }
    
    return this.createUnknownAgent();
  }

  /**
   * Human user detection (fallback)
   */
  private static detectHuman(): AgentInfo {
    const terminalFeatures = [
      process.stdout.isTTY,
      process.env.TERM !== undefined,
      process.env.SHELL !== undefined
    ];
    
    const supportsColors = process.stdout.isTTY && (
      process.env.COLORTERM !== undefined ||
      process.env.TERM?.includes('color') ||
      process.env.TERM === 'xterm-256color'
    );
    
    return {
      name: 'Human Terminal User',
      type: 'human',
      capabilities: {
        supportsColors,
        prefersStructuredData: false,
        supportsInteractivity: true
      },
      outputFormat: 'human',
      confidence: 0.6,
      adapterType: 'cli',
      participantCapabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: true,
        canInviteOthers: true,
        canModerate: true,
        autoResponds: false,
        providesContext: false
      }
    };
  }

  /**
   * Check if parent process matches a pattern
   */
  private static checkParentProcess(pattern: string): boolean {
    try {
      const { execSync } = require('child_process');
      const parentCmd = execSync(`ps -p ${process.ppid} -o comm=`, { encoding: 'utf8' }).trim();
      return parentCmd.toLowerCase().includes(pattern.toLowerCase());
    } catch {
      return false;
    }
  }

  /**
   * Get agent-appropriate output format
   */
  static getOutputFormat(): 'human' | 'ai-friendly' | 'compact' | 'json' {
    return this.detect().outputFormat;
  }

  /**
   * Check if current agent is AI
   */
  static isAI(): boolean {
    return this.detect().type === 'ai';
  }

  /**
   * Get agent display name for logging
   */
  static getAgentName(): string {
    const agent = this.detect();
    return `${agent.name} (${Math.round(agent.confidence * 100)}% confidence)`;
  }

  /**
   * Get capabilities for feature detection
   */
  static getCapabilities() {
    return this.detect().capabilities;
  }

  /**
   * Helper to create unknown agent info
   */
  private static createUnknownAgent(): AgentInfo {
    return { 
      name: 'unknown', 
      type: 'unknown', 
      capabilities: { supportsColors: false, prefersStructuredData: false, supportsInteractivity: false }, 
      outputFormat: 'human', 
      confidence: 0,
      adapterType: 'browser-ui',
      participantCapabilities: {
        canSendMessages: false,
        canReceiveMessages: false,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: false,
        providesContext: false
      }
    };
  }

  /**
   * Create agent info for JTAG connection context
   * @param overrides - Optional explicit overrides for impersonation/testing
   */
  static createConnectionContext(overrides?: Partial<AgentInfo>): Record<string, any> {
    const detected = this.detect();
    const agent = overrides ? { ...detected, ...overrides } : detected;
    
    return {
      agentInfo: {
        name: agent.name,
        type: agent.type,
        version: agent.version,
        confidence: overrides ? 1.0 : agent.confidence, // Full confidence if manually set
        detected: !overrides, // Flag whether this was auto-detected or manual
        overrides: overrides ? Object.keys(overrides) : []
      },
      adapterType: agent.adapterType,
      capabilities: agent.participantCapabilities,
      outputPreferences: {
        format: agent.outputFormat,
        supportsColors: agent.capabilities.supportsColors,
        maxLength: agent.capabilities.maxOutputLength
      }
    };
  }

  /**
   * Create human impersonation context (AI pretending to be human)
   */
  static createHumanImpersonation(): Record<string, any> {
    return this.createConnectionContext({
      name: 'Human User (AI Impersonation)',
      type: 'human',
      outputFormat: 'human',
      capabilities: {
        supportsColors: true,
        prefersStructuredData: false,
        supportsInteractivity: true,
        maxOutputLength: undefined
      },
      adapterType: 'browser-ui',
      participantCapabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: true,
        canInviteOthers: true,
        canModerate: true,
        autoResponds: false,
        providesContext: false
      }
    });
  }

  /**
   * Create AI impersonation context (Human pretending to be AI)
   */
  static createAIImpersonation(): Record<string, any> {
    return this.createConnectionContext({
      name: 'AI Agent (Human Impersonation)',
      type: 'ai',
      outputFormat: 'ai-friendly',
      capabilities: {
        supportsColors: false,
        prefersStructuredData: true,
        supportsInteractivity: false,
        maxOutputLength: 50000
      },
      adapterType: 'ai-api',
      participantCapabilities: {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: true,
        providesContext: true
      }
    });
  }
}

// Export convenience functions
export const detectAgent = () => AgentDetector.detect();
export const isAI = () => AgentDetector.isAI();
export const getOutputFormat = () => AgentDetector.getOutputFormat();
export const getAgentName = () => AgentDetector.getAgentName();