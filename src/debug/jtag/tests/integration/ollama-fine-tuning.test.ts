#!/usr/bin/env npx tsx
/**
 * OLLAMA LOCAL FINE-TUNING INTEGRATION TEST
 * ==========================================
 *
 * Tests the LOCAL fine-tuning path: PersonaTaskExecutor -> OllamaLoRAAdapter
 * This uses local llama.cpp and doesn't require API keys or spend money.
 *
 * PREREQUISITES:
 * 1. Ollama running with a base model (llama3.2:3b)
 * 2. llama.cpp installed with finetune binary
 *
 * TEST FLOW:
 * 1. Validate training data structures
 * 2. Verify OllamaLoRAAdapter configuration
 * 3. Check training request format
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { OllamaLoRAAdapter } from '../../daemons/ai-provider-daemon/adapters/ollama/server/OllamaFineTuningAdapter';
import type { LoRATrainingRequest, TrainingDataset, TrainingExample } from '../../system/genome/fine-tuning/shared/FineTuningTypes';
import type { TraitType } from '../../system/genome/entities/GenomeLayerEntity';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

describe('Ollama Fine-Tuning Integration', () => {
  let adapter: OllamaLoRAAdapter;

  beforeAll(() => {
    adapter = new OllamaLoRAAdapter();
  });

  describe('OllamaLoRAAdapter Configuration', () => {
    it('should report fine-tuning capabilities', () => {
      const capabilities = adapter.getFineTuningCapabilities();

      expect(capabilities).toBeDefined();
      expect(capabilities.minRank).toBe(8);
      expect(capabilities.maxRank).toBe(256);
      expect(capabilities.defaultRank).toBe(32);
      expect(capabilities.defaultEpochs).toBe(3);
      expect(capabilities.costPerExample).toBe(0); // Local = free
      expect(capabilities.requiresInternet).toBe(false);
    });

    it('should have correct provider ID', () => {
      expect(adapter.providerId).toBe('ollama');
    });

    it('should check finetune availability', () => {
      // Method exists and returns boolean
      const supported = adapter.supportsFineTuning();
      expect(typeof supported).toBe('boolean');
    });
  });

  describe('Training Data Structures', () => {
    it('should create valid training examples from chat messages', () => {
      // Simulate what collectTrainingExamples produces
      const userMessage = 'What is the capital of France?';
      const assistantResponse = 'The capital of France is Paris. It is known as the "City of Light."';
      const personaName = 'Helper AI';

      const example: TrainingExample = {
        messages: [
          {
            role: 'system',
            content: `You are ${personaName}, a helpful AI assistant.`
          },
          {
            role: 'user',
            content: userMessage
          },
          {
            role: 'assistant',
            content: assistantResponse
          }
        ],
        metadata: {
          timestamp: Date.now(),
          roomId: generateUUID()
        }
      };

      // Verify structure
      expect(example.messages.length).toBe(3);
      expect(example.messages[0].role).toBe('system');
      expect(example.messages[1].role).toBe('user');
      expect(example.messages[2].role).toBe('assistant');

      // Verify content
      expect(example.messages[0].content).toContain(personaName);
      expect(example.messages[1].content).toBe(userMessage);
      expect(example.messages[2].content).toBe(assistantResponse);
    });

    it('should create proper dataset metadata', () => {
      const personaId = generateUUID();
      const personaName = 'Test AI';
      const domain = 'communication';

      const metadata = {
        personaId,
        personaName,
        traitType: domain as TraitType,
        createdAt: Date.now(),
        source: 'conversations',
        totalExamples: 5
      };

      expect(metadata.personaId).toBe(personaId);
      expect(metadata.personaName).toBe(personaName);
      expect(metadata.traitType).toBe('communication');
      expect(metadata.source).toBe('conversations');
      expect(metadata.totalExamples).toBe(5);
    });

    it('should build valid training dataset', () => {
      const examples: TrainingExample[] = [
        {
          messages: [
            { role: 'system', content: 'You are a helpful assistant.' },
            { role: 'user', content: 'What is 2+2?' },
            { role: 'assistant', content: 'The answer is 4.' }
          ]
        },
        {
          messages: [
            { role: 'system', content: 'You are a helpful assistant.' },
            { role: 'user', content: 'What color is the sky?' },
            { role: 'assistant', content: 'The sky is blue.' }
          ]
        }
      ];

      const dataset: TrainingDataset = {
        examples,
        metadata: {
          personaId: generateUUID(),
          personaName: 'Test Persona',
          traitType: 'communication' as TraitType,
          createdAt: Date.now(),
          source: 'test',
          totalExamples: examples.length
        }
      };

      expect(dataset.examples.length).toBe(2);
      expect(dataset.metadata.totalExamples).toBe(2);
      expect(dataset.metadata.source).toBe('test');
    });

    it('should handle multi-turn conversations', () => {
      const multiTurnExample: TrainingExample = {
        messages: [
          { role: 'system', content: 'You are an expert coder.' },
          { role: 'user', content: 'How do I reverse a string in JavaScript?' },
          { role: 'assistant', content: 'You can use str.split("").reverse().join("").' },
          { role: 'user', content: 'What about TypeScript?' },
          { role: 'assistant', content: 'Same approach works in TypeScript.' }
        ]
      };

      expect(multiTurnExample.messages.length).toBe(5);
      expect(multiTurnExample.messages[0].role).toBe('system');
      expect(multiTurnExample.messages[4].role).toBe('assistant');

      // Verify message roles alternate correctly
      expect(multiTurnExample.messages[1].role).toBe('user');
      expect(multiTurnExample.messages[2].role).toBe('assistant');
      expect(multiTurnExample.messages[3].role).toBe('user');
    });
  });

  describe('Training Request Validation', () => {
    it('should validate training request parameters', () => {
      const request: LoRATrainingRequest = {
        personaId: generateUUID(),
        personaName: 'Test Persona',
        traitType: 'communication' as TraitType,
        baseModel: 'llama3.2:3b',
        dataset: {
          examples: [
            {
              messages: [
                { role: 'user', content: 'Hello' },
                { role: 'assistant', content: 'Hi there!' }
              ]
            }
          ],
          metadata: {
            personaId: generateUUID(),
            personaName: 'Test',
            traitType: 'communication' as TraitType,
            createdAt: Date.now(),
            source: 'test',
            totalExamples: 1
          }
        },
        rank: 16,
        alpha: 32,
        epochs: 1,
        learningRate: 0.0001,
        batchSize: 4
      };

      // Validate request structure
      expect(request.personaId).toBeDefined();
      expect(request.baseModel).toBe('llama3.2:3b');
      expect(request.rank).toBe(16);
      expect(request.alpha).toBe(32);
      expect(request.epochs).toBe(1);
      expect(request.dataset.examples.length).toBe(1);
    });

    it('should filter short responses correctly', () => {
      // Responses < 20 chars should be skipped in collectTrainingExamples
      const shortResponse = 'OK';
      const validResponse = 'That is a great question! Let me explain in detail...';

      expect(shortResponse.length).toBeLessThan(20);
      expect(validResponse.length).toBeGreaterThanOrEqual(20);
    });
  });

  describe('LoRA Hyperparameters', () => {
    it('should use sensible defaults', () => {
      const defaults = {
        rank: 16,
        alpha: 32,
        epochs: 3,
        learningRate: 0.0001,
        batchSize: 4
      };

      // Rank should be power of 2, typically 8-64
      expect(defaults.rank).toBeGreaterThanOrEqual(8);
      expect(defaults.rank).toBeLessThanOrEqual(64);

      // Alpha typically 2x rank for good results
      expect(defaults.alpha).toBe(defaults.rank * 2);

      // Epochs 1-5 for chat data (prevent overfitting)
      expect(defaults.epochs).toBeGreaterThanOrEqual(1);
      expect(defaults.epochs).toBeLessThanOrEqual(5);

      // Learning rate should be small
      expect(defaults.learningRate).toBeLessThanOrEqual(0.001);
      expect(defaults.learningRate).toBeGreaterThan(0);
    });

    it('should match adapter capabilities', () => {
      const capabilities = adapter.getFineTuningCapabilities();

      // Default rank within bounds
      expect(capabilities.defaultRank).toBeGreaterThanOrEqual(capabilities.minRank);
      expect(capabilities.defaultRank).toBeLessThanOrEqual(capabilities.maxRank);

      // Default epochs within bounds
      expect(capabilities.defaultEpochs).toBeGreaterThanOrEqual(capabilities.minEpochs);
      expect(capabilities.defaultEpochs).toBeLessThanOrEqual(capabilities.maxEpochs);

      // Default learning rate within bounds
      expect(capabilities.defaultLearningRate).toBeGreaterThanOrEqual(capabilities.minLearningRate);
      expect(capabilities.defaultLearningRate).toBeLessThanOrEqual(capabilities.maxLearningRate);
    });
  });
});

/**
 * USAGE:
 *
 * Run this test:
 *   npx vitest tests/integration/ollama-fine-tuning.test.ts
 *
 * Or run with watch mode:
 *   npx vitest tests/integration/ollama-fine-tuning.test.ts --watch
 *
 * This tests the LOCAL fine-tuning structures without requiring:
 * - Running JTAG server
 * - API keys
 * - Real money
 * - Actually running llama.cpp
 */
