/**
 * Milestone Configuration
 * 
 * Configurable milestone definitions for system readiness tracking.
 * Easily extensible for future projects and requirements.
 */

import { MilestoneConfig, SystemReadySignal, SignalConfig } from './SystemSignalingTypes';
import { getActivePorts } from '../../../system/shared/ExampleConfig';

// Main configuration factory - easily customizable per project
export function getDefaultMilestoneConfig(): MilestoneConfig {
  const activePorts = getActivePorts();
  
  return {
    core: [
      {
        id: 'bootstrap',
        name: 'Bootstrap',
        description: 'System bootstrap and command discovery complete',
        required: true,
        emoji: '🚀',
        checkFn: (signal: SystemReadySignal) => signal.bootstrapComplete
      },
      {
        id: 'commands',
        name: 'Commands',
        description: 'Command registry populated with available commands',
        required: true,
        emoji: '⚡',
        checkFn: (signal: SystemReadySignal) => signal.commandCount > 0
      },
      {
        id: 'websocket',
        name: 'WebSocket',
        description: `WebSocket server active on port ${activePorts.websocket_server}`,
        required: true,
        emoji: '🔌',
        checkFn: (signal: SystemReadySignal) => 
          (signal.portsActive?.length || 0) >= 1 && 
          (signal.portsActive?.includes(activePorts.websocket_server) || false)
      },
      {
        id: 'http',
        name: 'HTTP',
        description: `HTTP server active on port ${activePorts.http_server}`,
        required: true,
        emoji: '🌐',
        checkFn: (signal: SystemReadySignal) => 
          (signal.portsActive?.length || 0) >= 2 && 
          (signal.portsActive?.includes(activePorts.http_server) || false)
      },
      {
        id: 'browser',
        name: 'Browser',
        description: 'Browser interface ready and responding',
        required: false, // Nice-to-have for degraded mode acceptance
        emoji: '🖥️',
        checkFn: (signal: SystemReadySignal) => signal.browserReady
      }
    ],
    
    // Future expansion examples (not currently used but demonstrates flexibility)
    performance: [
      {
        id: 'build_speed',
        name: 'Build Speed',
        description: 'TypeScript compilation completed within performance targets',
        required: false,
        emoji: '⚡',
        checkFn: (signal: SystemReadySignal) => signal.buildStatus === 'success'
      }
    ],
    
    integration: [
      {
        id: 'external_apis',
        name: 'External APIs',
        description: 'External service integrations are healthy',
        required: false,
        emoji: '🔗',
        checkFn: (signal: SystemReadySignal) => signal.errors.length === 0
      }
    ]
  };
}

// Signal configuration factory
export function getSignalConfig(): SignalConfig {
  // Use ExampleConfig ports instead of SystemConfiguration for consistency
  const activePorts = getActivePorts();
  return {
    VERSION: '1.0.0',
    EXPECTED_PORTS: [
      activePorts.websocket_server,
      activePorts.http_server
    ].filter(port => port > 0) as readonly number[], // Filter out disabled ports
    MIN_COMMAND_COUNT: 1, // Reduced - we now use process registry for readiness
    DEFAULT_TIMEOUT_MS: 30000 // Reduced timeout since registry-based detection is faster
  } as const;
}

// Custom milestone builder for project-specific requirements
export function createCustomMilestone(
  id: string,
  name: string,
  description: string,
  checkFn: (signal: SystemReadySignal) => boolean,
  options?: {
    required?: boolean;
    emoji?: string;
  }
) {
  return {
    id,
    name,
    description,
    required: options?.required ?? false,
    emoji: options?.emoji ?? '📋',
    checkFn
  };
}

// Milestone config merger for combining configs
export function mergeMilestoneConfigs(...configs: MilestoneConfig[]): MilestoneConfig {
  return configs.reduce((merged, config) => ({
    core: [...(merged.core || []), ...(config.core || [])],
    performance: [...(merged.performance || []), ...(config.performance || [])],
    integration: [...(merged.integration || []), ...(config.integration || [])],
    custom: [...(merged.custom || []), ...(config.custom || [])]
  }), {} as MilestoneConfig);
}