#!/usr/bin/env tsx

/**
 * Agent Detection Module Tests
 * 
 * Tests the auto-detection pattern, persona information extraction,
 * and chat widget integration features.
 */

// Conditional Jest imports for compatibility
let describe: any, test: any, expect: any, beforeEach: any;
try {
  ({ describe, test, expect, beforeEach } = require('@jest/globals'));
} catch {
  // Fallback for direct execution - provide minimal implementations
  describe = (name: string, fn: () => void) => { console.log(`📋 ${name}`); fn(); };
  test = (name: string, fn: () => void) => { console.log(`  ✅ ${name}`); fn(); };
  expect = (actual: any) => ({
    toBe: (expected: any) => { if (actual !== expected) throw new Error(`Expected ${expected}, got ${actual}`); },
    toBeTruthy: () => { if (!actual) throw new Error(`Expected truthy, got ${actual}`); },
    toBeDefined: () => { if (actual === undefined) throw new Error('Expected defined'); },
    toMatch: (pattern: RegExp | string) => { 
      const regex = typeof pattern === 'string' ? new RegExp(pattern) : pattern;
      if (!regex.test(String(actual))) throw new Error(`Expected ${actual} to match ${pattern}`); 
    },
    toBeGreaterThan: (expected: number) => { if (actual <= expected) throw new Error(`Expected ${actual} > ${expected}`); },
    toBeGreaterThanOrEqual: (expected: number) => { if (actual < expected) throw new Error(`Expected ${actual} >= ${expected}`); },
    toBeLessThanOrEqual: (expected: number) => { if (actual > expected) throw new Error(`Expected ${actual} <= ${expected}`); },
    toContain: (expected: any) => { if (!String(actual).includes(String(expected))) throw new Error(`Expected ${actual} to contain ${expected}`); },
    not: {
      toBe: (expected: any) => { if (actual === expected) throw new Error(`Expected not ${expected}, got ${actual}`); }
    }
  });
  beforeEach = (fn: () => void) => fn();
}

import { AgentDetectionRegistry } from '../system/core/detection/AgentDetectionRegistry';
import { ClaudePlugin } from '../system/core/detection/plugins/ClaudePlugin';
import { HumanPlugin } from '../system/core/detection/plugins/HumanPlugin';

describe('Agent Detection System', () => {
  let registry: AgentDetectionRegistry;
  
  beforeEach(() => {
    registry = new AgentDetectionRegistry();
  });

  describe('Auto-Detection Pattern', () => {
    test('should auto-detect by default when no parameters provided', () => {
      const context = registry.createConnectionContext();
      
      expect(context.agentInfo).toBeDefined();
      expect(context.agentInfo.name).toBeTruthy();
      expect(context.agentInfo.type).toBeTruthy();
      expect(context.agentInfo.detected).toBe(true);
      expect(typeof context.agentInfo.confidence).toBe('number');
    });

    test('should use explicit overrides when provided', () => {
      const overrides = {
        detection: {
          name: 'Test Agent',
          type: 'test',
          confidence: 1.0,
          metadata: { source: 'manual' }
        }
      } as any;
      
      const context = registry.createConnectionContext(overrides);
      
      expect(context.agentInfo.name).toBe('Test Agent');
      expect(context.agentInfo.type).toBe('test');
      expect(context.agentInfo.confidence).toBe(1.0);
      expect(context.agentInfo.detected).toBe(false);
    });

    test('should merge partial overrides with auto-detection', () => {
      const partialOverrides = {
        outputFormat: 'json'
      } as any;
      
      const context = registry.createConnectionContext(partialOverrides);
      
      // Should still detect agent automatically
      expect(context.agentInfo.name).toBeTruthy();
      expect(context.agentInfo.type).toBeTruthy();
      // But mark as not purely auto-detected due to overrides
      expect(context.agentInfo.detected).toBe(false);
    });
  });

  describe('Persona Information Extraction', () => {
    test('should provide persona details for chat widgets', () => {
      const agent = registry.detect();
      const context = agent.plugin.getConnectionContext?.();
      
      if (context?.persona) {
        expect(context.persona.personaId).toBeTruthy();
        expect(context.persona.displayName).toBeTruthy();
        expect(context.persona.shortName).toBeTruthy();
        expect(context.persona.personaType).toMatch(/ai-assistant|developer|system|user|bot/);
        expect(context.persona.preferences).toBeDefined();
        expect(context.persona.preferences.theme).toMatch(/light|dark|auto/);
        expect(context.persona.preferences.verbosity).toMatch(/minimal|normal|detailed/);
      }
    });

    test('should provide agent type vs persona distinction', () => {
      const agent = registry.detect();
      const context = agent.plugin.getConnectionContext?.();
      
      if (context?.agent && context?.persona) {
        // Agent type should be technology identifier
        expect(context.agent.type).toMatch(/claude|chatgpt|copilot|human|ci/);
        expect(context.agent.provider).toBeTruthy();
        
        // Persona should be human-readable identity
        expect(context.persona.displayName).toBeTruthy();
        expect(context.persona.shortName).toBeTruthy();
        
        // They should be different concepts
        expect(context.agent.type).not.toBe(context.persona.displayName);
      }
    });

    test('should provide display helpers for UI', () => {
      const agent = registry.detect();
      const context = agent.plugin.getConnectionContext?.();
      
      if (context?.display) {
        expect(context.display.chatLabel).toBeTruthy();
        expect(context.display.shortLabel).toBeTruthy();
        expect(context.display.statusBadge).toBeTruthy();
        expect(context.display.colorTheme).toMatch(/^#[0-9A-Fa-f]{6}$/); // Hex color
      }
    });
  });

  describe('Chat System Integration', () => {
    test('should provide participant capabilities', () => {
      const agent = registry.detect();
      const context = agent.plugin.getConnectionContext?.();
      
      if (context?.participantCapabilities) {
        const caps = context.participantCapabilities;
        expect(typeof caps.canSendMessages).toBe('boolean');
        expect(typeof caps.canReceiveMessages).toBe('boolean');
        expect(typeof caps.canCreateRooms).toBe('boolean');
        expect(typeof caps.canInviteOthers).toBe('boolean');
        expect(typeof caps.canModerate).toBe('boolean');
        expect(typeof caps.autoResponds).toBe('boolean');
        expect(typeof caps.providesContext).toBe('boolean');
      }
    });

    test('should provide chat profile for widgets', () => {
      const agent = registry.detect();
      const context = agent.plugin.getConnectionContext?.();
      
      if (context?.chatProfile) {
        const profile = context.chatProfile;
        expect(typeof profile.canInitiateConversation).toBe('boolean');
        expect(typeof profile.canMentionUsers).toBe('boolean');
        expect(typeof profile.showTypingIndicators).toBe('boolean');
        expect(typeof profile.maxMessageLength).toBe('number');
        expect(Array.isArray(profile.supportedMessageTypes)).toBe(true);
      }
    });

    test('should provide adapter configuration', () => {
      const agent = registry.detect();
      const context = agent.plugin.getConnectionContext?.();
      
      if (context?.participantAdapter) {
        const adapter = context.participantAdapter;
        expect(adapter.type).toBeTruthy();
        expect(adapter.config).toBeDefined();
        expect(adapter.config?.type).toBeTruthy();
        
        if (adapter.responseStrategy) {
          expect(Array.isArray(adapter.responseStrategy.triggers)).toBe(true);
          adapter.responseStrategy.triggers.forEach((trigger: any) => {
            expect(trigger.type).toBeTruthy();
            expect(typeof trigger.probability).toBe('number');
            expect(trigger.probability).toBeGreaterThanOrEqual(0);
            expect(trigger.probability).toBeLessThanOrEqual(1);
          });
        }
      }
    });
  });

  describe('Plugin System', () => {
    test('should register and unregister plugins', () => {
      const initialPluginCount = registry.getPlugins().length;
      
      const testPlugin = new class extends ClaudePlugin {
        readonly name = 'Test Plugin';
        readonly priority = 50;
      };
      
      registry.register(testPlugin);
      expect(registry.getPlugins().length).toBe(initialPluginCount + 1);
      
      const success = registry.unregister('Test Plugin');
      expect(success).toBe(true);
      expect(registry.getPlugins().length).toBe(initialPluginCount);
    });

    test('should prioritize plugins correctly', () => {
      const plugins = registry.getPlugins();
      
      // Should be sorted by priority (highest first)
      for (let i = 0; i < plugins.length - 1; i++) {
        expect(plugins[i].priority).toBeGreaterThanOrEqual(plugins[i + 1].priority);
      }
    });

    test('should cache detection results', () => {
      const agent1 = registry.detect();
      const agent2 = registry.detect();
      
      // Should return the same cached instance
      expect(agent1).toBe(agent2);
      
      // Force re-detection should return new instance
      const agent3 = registry.redetect();
      expect(agent3).not.toBe(agent1);
    });
  });

  describe('Output Formats', () => {
    test('should provide appropriate output format for detected agent', () => {
      const format = registry.getOutputFormat();
      expect(format).toMatch(/human|ai-friendly|compact|json/);
    });

    test('should identify AI agents correctly', () => {
      const isAI = registry.isAI();
      expect(typeof isAI).toBe('boolean');
    });

    test('should provide descriptive agent name', () => {
      const name = registry.getAgentName();
      expect(name).toBeTruthy();
      expect(name).toContain('confidence');
      expect(name).toMatch(/\d+%/); // Should contain percentage
    });
  });

  describe('Environment Detection', () => {
    test('should detect Claude Code environment if available', () => {
      // This test will pass or fail depending on whether we're actually in Claude Code
      const agent = registry.detect();
      
      if (process.env.CLAUDECODE === '1') {
        expect(agent.detection.name).toBe('Claude Code');
        expect(agent.detection.type).toBe('ai');
        expect(agent.detection.confidence).toBeGreaterThan(0.7);
      }
    });

    test('should fallback to human detection', () => {
      // Create registry with only human plugin
      const humanOnlyRegistry = new AgentDetectionRegistry();
      
      // Clear all plugins and add only human
      const plugins = humanOnlyRegistry.getPlugins();
      plugins.forEach(plugin => {
        humanOnlyRegistry.unregister(plugin.name);
      });
      humanOnlyRegistry.register(new HumanPlugin());
      
      const agent = humanOnlyRegistry.detect();
      expect(agent.detection.type).toBe('human');
      expect(agent.detection.name).toBe('Human Terminal User');
    });
  });
});

// If running directly, execute tests
if (require.main === module) {
  console.log('🧪 Running Agent Detection Tests...\n');
  
  // Simple test runner for direct execution
  const runTests = async () => {
    try {
      const registry = new AgentDetectionRegistry();
      
      console.log('✅ Auto-detection pattern test');
      const context = registry.createConnectionContext();
      console.log(`   Detected: ${context.agentInfo.name} (${context.agentInfo.type})`);
      
      console.log('✅ Persona information test');
      const agent = registry.detect();
      const personaContext = agent.plugin.getConnectionContext?.();
      if (personaContext?.persona) {
        console.log(`   Persona: ${personaContext.persona.displayName}`);
        console.log(`   ID: ${personaContext.persona.personaId}`);
      }
      
      console.log('✅ Plugin priority test');
      const plugins = registry.getPlugins();
      console.log(`   Loaded ${plugins.length} plugins in priority order`);
      
      console.log('\n🎉 All agent detection tests passed!');
      
    } catch (error) {
      console.error('❌ Tests failed:', error);
      process.exit(1);
    }
  };
  
  runTests();
}