/**
 * CI/CD System Detection Plugin
 * 
 * Detects various CI/CD environments
 */

import { AgentDetectionPlugin, type AgentDetectionResult, type AgentCapabilities, type ParticipantProfile } from '../AgentDetectionPlugin';

export class CIPlugin extends AgentDetectionPlugin {
  readonly name = 'CI/CD System';
  readonly priority = 80;
  
  detect(): AgentDetectionResult | null {
    const ciSystems = [
      { env: 'GITHUB_ACTIONS', name: 'GitHub Actions', confidence: 0.95 },
      { env: 'GITLAB_CI', name: 'GitLab CI', confidence: 0.95 },
      { env: 'JENKINS_URL', name: 'Jenkins', confidence: 0.9 },
      { env: 'TRAVIS', name: 'Travis CI', confidence: 0.9 },
      { env: 'CIRCLECI', name: 'CircleCI', confidence: 0.9 },
      { env: 'BUILDKITE', name: 'Buildkite', confidence: 0.9 },
      { env: 'DRONE', name: 'Drone CI', confidence: 0.85 },
      { env: 'TEAMCITY_VERSION', name: 'TeamCity', confidence: 0.85 }
    ];
    
    for (const ci of ciSystems) {
      if (process.env[ci.env]) {
        return {
          name: ci.name,
          type: 'ci',
          confidence: ci.confidence,
          metadata: {
            platform: ci.name,
            buildId: process.env.BUILD_ID || process.env.GITHUB_RUN_ID || process.env.CI_PIPELINE_ID,
            branch: process.env.GITHUB_REF_NAME || process.env.CI_COMMIT_REF_NAME || process.env.BRANCH_NAME,
            repository: process.env.GITHUB_REPOSITORY || process.env.CI_PROJECT_PATH
          }
        };
      }
    }
    
    // Generic CI detection
    if (process.env.CI === 'true') {
      return {
        name: 'Generic CI System',
        type: 'ci',
        confidence: 0.7,
        metadata: {
          platform: 'unknown'
        }
      };
    }
    
    return null;
  }
  
  getCapabilities(): AgentCapabilities {
    return {
      supportsColors: false,
      prefersStructuredData: false,
      supportsInteractivity: false,
      maxOutputLength: 50000, // CI logs can be long
      rateLimit: {
        requestsPerMinute: 30 // Moderate rate for CI
      }
    };
  }
  
  getParticipantProfile(): ParticipantProfile {
    return {
      canSendMessages: true,
      canReceiveMessages: false, // CI typically just reports results
      canCreateRooms: false,
      canInviteOthers: false,
      canModerate: false,
      autoResponds: false,
      providesContext: true, // CI provides build context
      trustLevel: 'system' // CI systems are trusted
    };
  }
  
  getOutputFormat(): 'human' | 'ai-friendly' | 'compact' | 'json' {
    return 'compact';
  }
  
  getAdapterType(): 'browser-ui' | 'ai-api' | 'cli' | 'webhook' | 'automation' | 'bot' {
    return 'automation';
  }
  
  getConnectionContext(): Record<string, any> {
    return {
      ci: {
        platform: this.detect()?.name,
        buildContext: {
          id: process.env.BUILD_ID || process.env.GITHUB_RUN_ID,
          branch: process.env.GITHUB_REF_NAME || process.env.CI_COMMIT_REF_NAME,
          commit: process.env.GITHUB_SHA || process.env.CI_COMMIT_SHA
        }
      }
    };
  }
}