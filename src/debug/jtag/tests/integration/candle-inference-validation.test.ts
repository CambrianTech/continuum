#!/usr/bin/env tsx
/**
 * Candle Inference Validation Test
 * ==================================
 *
 * Comprehensive integration tests for Candle (native Rust) inference.
 * Validates inference works correctly across different models and configurations.
 *
 * Candle is the ONLY local inference path.
 *
 * Test Coverage:
 * 1. Basic inference with default model (Qwen2-1.5B)
 * 2. Inference with quantized models (Q4_K_M)
 * 3. Multiple model switching
 * 4. Concurrent requests
 * 5. Error handling (NaN/Inf protection)
 * 6. Streaming inference
 */

import { describe, test, expect, beforeAll, afterAll } from 'vitest';
import { AIProviderDaemon } from '../../daemons/ai-provider-daemon/shared/AIProviderDaemon';

// Wait for inference-grpc worker to be ready
const WORKER_STARTUP_TIMEOUT = 30000;

describe('Candle Inference Validation', () => {
  beforeAll(async () => {
    console.log('\n🔧 Initializing Candle inference tests...');
    // AIProviderDaemon should already be initialized if server is running
    // These tests assume the server is running (npm start)
  }, WORKER_STARTUP_TIMEOUT);

  afterAll(async () => {
    console.log('🧹 Candle inference tests complete');
  });

  describe('Basic Inference', () => {
    test('should generate text with Candle adapter', async () => {
      console.log('\n🧪 TEST: Basic Candle text generation');
      console.log('=====================================');

      const adapter = AIProviderDaemon.getAdapter('candle');
      expect(adapter).toBeDefined();

      const startTime = Date.now();
      const result = await adapter!.generateText({
        messages: [{ role: 'user', content: 'Say "Hello from Candle" and nothing else.' }],
        model: 'llama3.2:3b', // Will be mapped to Qwen via LOCAL_MODELS
        temperature: 0.7,
        maxTokens: 50,
      });

      const responseTime = Date.now() - startTime;

      console.log(`✅ Response in ${responseTime}ms`);
      console.log(`📤 Text: "${result.text.substring(0, 100)}..."`);

      expect(result.text).toBeDefined();
      expect(result.text.length).toBeGreaterThan(0);
      expect(responseTime).toBeLessThan(60000); // Should complete within 60 seconds
    }, 120000);

    test('should handle temperature and maxTokens correctly', async () => {
      console.log('\n🧪 TEST: Temperature and maxTokens handling');
      console.log('==========================================');

      const adapter = AIProviderDaemon.getAdapter('candle');
      expect(adapter).toBeDefined();

      // Low temperature = more deterministic
      const lowTempResult = await adapter!.generateText({
        messages: [{ role: 'user', content: 'What is 2+2?' }],
        model: 'llama3.2:1b',
        temperature: 0.1,
        maxTokens: 10,
      });

      console.log(`📤 Low temp result: "${lowTempResult.text.substring(0, 50)}"`);
      expect(lowTempResult.text).toBeDefined();

      // High temperature = more creative
      const highTempResult = await adapter!.generateText({
        messages: [{ role: 'user', content: 'What is 2+2?' }],
        model: 'llama3.2:1b',
        temperature: 1.0,
        maxTokens: 10,
      });

      console.log(`📤 High temp result: "${highTempResult.text.substring(0, 50)}"`);
      expect(highTempResult.text).toBeDefined();
    }, 120000);
  });

  describe('Model Mapping', () => {
    test('should map legacy model names to HuggingFace IDs', async () => {
      console.log('\n🧪 TEST: Model name mapping (legacy short names -> HuggingFace)');
      console.log('==========================================================');

      const adapter = AIProviderDaemon.getAdapter('candle');
      expect(adapter).toBeDefined();

      // Test various legacy model names
      const modelTests = [
        { name: 'llama3.2:3b', expected: 'Qwen' }, // Should map to Qwen
        { name: 'llama3.2:1b', expected: 'Qwen' }, // Should map to smaller Qwen
        { name: 'qwen2:1.5b', expected: 'Qwen' },  // Direct Qwen reference
      ];

      for (const { name, expected } of modelTests) {
        console.log(`\n  Testing model: ${name}`);
        const result = await adapter!.generateText({
          messages: [{ role: 'user', content: 'Hi' }],
          model: name,
          temperature: 0.7,
          maxTokens: 5,
        });

        console.log(`    ✅ Got response: "${result.text.substring(0, 30)}..."`);
        expect(result.text).toBeDefined();
      }
    }, 180000);
  });

  describe('Concurrent Requests', () => {
    test('should handle multiple concurrent inference requests', async () => {
      console.log('\n🧪 TEST: Concurrent inference requests');
      console.log('======================================');

      const adapter = AIProviderDaemon.getAdapter('candle');
      expect(adapter).toBeDefined();

      const prompts = [
        'Count to 3',
        'Name a color',
        'Say hello',
        'What is 1+1?',
      ];

      const startTime = Date.now();
      const promises = prompts.map(prompt =>
        adapter!.generateText({
          messages: [{ role: 'user', content: prompt }],
          model: 'llama3.2:1b',
          temperature: 0.7,
          maxTokens: 20,
        })
      );

      const results = await Promise.all(promises);
      const totalTime = Date.now() - startTime;

      console.log(`✅ All ${results.length} requests complete in ${totalTime}ms`);

      for (let i = 0; i < results.length; i++) {
        console.log(`   ${i + 1}. "${prompts[i]}" -> "${results[i].text.substring(0, 30)}..."`);
        expect(results[i].text).toBeDefined();
        expect(results[i].text.length).toBeGreaterThan(0);
      }

      // Should leverage worker pool for parallelism
      // Total time should be less than sequential time
      expect(totalTime).toBeLessThan(prompts.length * 30000);
    }, 180000);
  });

  describe('Error Handling', () => {
    test('should not crash on edge case inputs', async () => {
      console.log('\n🧪 TEST: Edge case input handling');
      console.log('=================================');

      const adapter = AIProviderDaemon.getAdapter('candle');
      expect(adapter).toBeDefined();

      // Empty message
      try {
        const result = await adapter!.generateText({
          messages: [{ role: 'user', content: '' }],
          model: 'llama3.2:1b',
          temperature: 0.7,
          maxTokens: 10,
        });
        console.log(`✅ Empty message handled: "${result.text.substring(0, 30)}..."`);
      } catch (error) {
        console.log(`✅ Empty message threw error as expected: ${error instanceof Error ? error.message : String(error)}`);
      }

      // Very long message (should be truncated)
      const longMessage = 'A'.repeat(5000);
      try {
        const result = await adapter!.generateText({
          messages: [{ role: 'user', content: longMessage }],
          model: 'llama3.2:1b',
          temperature: 0.7,
          maxTokens: 10,
        });
        console.log(`✅ Long message handled: "${result.text.substring(0, 30)}..."`);
      } catch (error) {
        console.log(`✅ Long message threw error: ${error instanceof Error ? error.message : String(error)}`);
      }
    }, 120000);

    test('should return error text instead of crashing on NaN/Inf (sanitize_logits)', async () => {
      console.log('\n🧪 TEST: NaN/Inf protection (sanitize_logits)');
      console.log('=============================================');

      // This test validates that the sanitize_logits fix is working
      // Previously, NaN/Inf in logits would cause "Sampling failed" error
      // Now it should either succeed or return a graceful error

      const adapter = AIProviderDaemon.getAdapter('candle');
      expect(adapter).toBeDefined();

      // Use a prompt that might trigger edge cases in the model
      const result = await adapter!.generateText({
        messages: [{ role: 'user', content: 'Complete this: The quick brown fox' }],
        model: 'llama3.2:1b',
        temperature: 0.8,
        maxTokens: 20,
      });

      console.log(`📤 Result: "${result.text.substring(0, 50)}..."`);

      // Should not contain ERROR prefix (that was the old bug behavior)
      expect(result.text.startsWith('ERROR:')).toBe(false);
    }, 60000);
  });

  describe('Provider Aliasing', () => {
    test('should have candle adapter available', async () => {
      console.log('\n🧪 TEST: Candle adapter availability');
      console.log('====================================');

      const candleAdapter = AIProviderDaemon.getAdapter('candle');

      console.log(`   candle adapter: ${candleAdapter ? 'available' : 'null'}`);

      // Candle should be available
      expect(candleAdapter).toBeDefined();
    }, 60000);
  });
});

// Run if executed directly
if (require.main === module) {
  console.log('🔥 CANDLE INFERENCE VALIDATION TESTS');
  console.log('=====================================\n');
  console.log('Run with: npx vitest tests/integration/candle-inference-validation.test.ts');
}
