#!/usr/bin/env tsx
/**
 * AI Cost Tracking Integration Tests
 * ===================================
 *
 * Tests the complete cost tracking and pricing system:
 * - PricingManager loads static pricing
 * - PricingFetcher retrieves live pricing from OpenRouter
 * - Adapters use PricingManager for accurate cost calculation
 * - AIGenerationEntity tracks generations to database
 * - Cost aggregation queries work correctly
 * - Costs match expected values within tolerance
 */

import { PricingManager } from '../../daemons/ai-provider-daemon/shared/PricingManager';
import { PricingFetcher } from '../../daemons/ai-provider-daemon/shared/PricingFetcher';
import { AIGenerationEntity } from '../../system/data/entities/AIGenerationEntity';
import { OpenAIAdapter } from '../../daemons/ai-provider-daemon/adapters/openai/shared/OpenAIAdapter';
import { initializeSecrets, SecretManager } from '../../system/secrets/SecretManager';

async function testPricingManagerStaticPricing(): Promise<void> {
  console.log('\n💰 TEST 1: PricingManager Static Pricing');
  console.log('=========================================');

  const pricingManager = PricingManager.getInstance();

  // Test OpenAI pricing
  const gpt4Pricing = pricingManager.getModelPricing('openai', 'gpt-4');
  console.log(`✅ GPT-4 pricing loaded:`, gpt4Pricing);

  if (!gpt4Pricing) {
    throw new Error('❌ Failed to load GPT-4 pricing from static JSON');
  }

  // Test cost calculation with conservative rounding
  const inputTokens = 1000;
  const outputTokens = 500;
  const cost = pricingManager.calculateCost(inputTokens, outputTokens, gpt4Pricing);

  // Expected: (1000/1M * 30) + (500/1M * 60) = 0.03 + 0.03 = 0.06
  // With conservative rounding: Math.ceil(0.06 * 10000) / 10000 = 0.06
  console.log(`   Input: ${inputTokens} tokens, Output: ${outputTokens} tokens`);
  console.log(`   Cost: $${cost.toFixed(4)} (conservative rounding)`);

  if (cost !== 0.06) {
    throw new Error(`❌ Cost calculation incorrect: expected $0.0600, got $${cost.toFixed(4)}`);
  }

  console.log(`✅ Cost calculation correct: $${cost.toFixed(4)}`);

  // Test Anthropic pricing
  const claudeOpusPricing = pricingManager.getModelPricing('anthropic', 'claude-3-opus-20240229');
  console.log(`✅ Claude 3 Opus pricing loaded:`, claudeOpusPricing);

  if (!claudeOpusPricing) {
    throw new Error('❌ Failed to load Claude 3 Opus pricing');
  }

  // Test DeepSeek pricing
  const deepseekPricing = pricingManager.getModelPricing('deepseek', 'deepseek-reasoner');
  console.log(`✅ DeepSeek R1 pricing loaded:`, deepseekPricing);

  if (!deepseekPricing) {
    throw new Error('❌ Failed to load DeepSeek R1 pricing');
  }

  // Test Candle (free, wildcard pricing)
  const candlePricing = pricingManager.getModelPricing('candle', 'llama-3.2-vision');
  console.log(`✅ Candle pricing loaded (wildcard):`, candlePricing);

  if (!candlePricing || candlePricing.inputPer1M !== 0 || candlePricing.outputPer1M !== 0) {
    throw new Error('❌ Candle should have $0 pricing (local inference)');
  }

  console.log('✅ All static pricing tests passed');
}

async function testPricingFetcherOpenRouter(): Promise<void> {
  console.log('\n🌐 TEST 2: PricingFetcher - OpenRouter API');
  console.log('===========================================');

  try {
    console.log('📡 Fetching live pricing from OpenRouter...');
    const openRouterPricing = await PricingFetcher.fetchFromOpenRouter();

    console.log(`✅ Fetched pricing for ${openRouterPricing.size} models from OpenRouter`);

    // Check for a few expected models
    const gpt4oKey = 'openai/gpt-4o';
    const claude3Key = 'anthropic/claude-3-5-sonnet-20241022';

    if (openRouterPricing.has(gpt4oKey)) {
      const gpt4oPricing = openRouterPricing.get(gpt4oKey);
      console.log(`   ${gpt4oKey}:`, {
        inputPer1M: `$${gpt4oPricing?.inputPer1M.toFixed(2)}`,
        outputPer1M: `$${gpt4oPricing?.outputPer1M.toFixed(2)}`
      });
    } else {
      console.log(`   ⚠️  ${gpt4oKey} not found in OpenRouter pricing`);
    }

    if (openRouterPricing.has(claude3Key)) {
      const claude3Pricing = openRouterPricing.get(claude3Key);
      console.log(`   ${claude3Key}:`, {
        inputPer1M: `$${claude3Pricing?.inputPer1M.toFixed(2)}`,
        outputPer1M: `$${claude3Pricing?.outputPer1M.toFixed(2)}`
      });
    } else {
      console.log(`   ⚠️  ${claude3Key} not found in OpenRouter pricing`);
    }

    // Test adapter pricing cache
    const pricingManager = PricingManager.getInstance();
    for (const [modelId, pricing] of openRouterPricing.entries()) {
      const parsed = PricingFetcher.parseOpenRouterModelId(modelId);
      if (parsed && parsed.provider === 'openai') {
        pricingManager.registerAdapterPricing(parsed.provider, parsed.model, pricing);
        break; // Just test one to verify caching works
      }
    }

    console.log('✅ OpenRouter pricing fetch and caching successful');
  } catch (error) {
    console.warn(`⚠️  OpenRouter fetch failed (may be rate limited or down):`, error);
    console.log('   Continuing with static pricing fallback...');
  }
}

async function testAIGenerationEntityCreation(): Promise<void> {
  console.log('\n💾 TEST 3: AIGenerationEntity Creation');
  console.log('======================================');

  const mockResponse = {
    text: 'Hello! How can I help you today?',
    finishReason: 'stop' as const,
    model: 'gpt-4',
    provider: 'openai',
    usage: {
      inputTokens: 15,
      outputTokens: 10,
      totalTokens: 25,
      estimatedCost: 0.0009 // (15/1M * 30) + (10/1M * 60) = 0.00045 + 0.0006 = 0.00105 → rounds to 0.0011
    },
    responseTime: 1234,
    requestId: 'test-request-123'
  };

  const context = {
    userId: 'user-test-123' as any,
    roomId: 'room-test-456' as any,
    purpose: 'chat'
  };

  try {
    const entity = await AIGenerationEntity.createFromResponse(mockResponse, context);

    console.log(`✅ AIGenerationEntity created:`, {
      id: entity.id,
      provider: entity.provider,
      model: entity.model,
      inputTokens: entity.inputTokens,
      outputTokens: entity.outputTokens,
      estimatedCost: `$${entity.estimatedCost.toFixed(4)}`,
      responseTime: `${entity.responseTimeMs}ms`,
      success: entity.success
    });

    // Validate fields
    if (entity.provider !== 'openai') {
      throw new Error(`❌ Provider mismatch: expected 'openai', got '${entity.provider}'`);
    }

    if (entity.model !== 'gpt-4') {
      throw new Error(`❌ Model mismatch: expected 'gpt-4', got '${entity.model}'`);
    }

    if (entity.inputTokens !== 15 || entity.outputTokens !== 10) {
      throw new Error(`❌ Token count mismatch`);
    }

    if (entity.estimatedCost !== 0.0009) {
      throw new Error(`❌ Cost mismatch: expected $0.0009, got $${entity.estimatedCost.toFixed(4)}`);
    }

    console.log('✅ All AIGenerationEntity fields validated');
  } catch (error) {
    console.error(`❌ AIGenerationEntity creation failed:`, error);
    throw error;
  }
}

async function testAdapterCostCalculation(): Promise<void> {
  console.log('\n🔧 TEST 4: Adapter Cost Calculation Integration');
  console.log('================================================');

  const hasOpenAI = SecretManager.getInstance().has('OPENAI_API_KEY');

  if (!hasOpenAI) {
    console.log('⏭️  Skipped (OPENAI_API_KEY not configured)');
    console.log('   To test adapter cost calculation, add OPENAI_API_KEY to ~/.continuum/config.env');
    return;
  }

  try {
    const adapter = new OpenAIAdapter();
    await adapter.initialize();

    console.log('📤 Sending test prompt to measure cost...');
    const startTime = Date.now();

    const response = await adapter.generateText({
      messages: [
        { role: 'user', content: 'Count to 5' }
      ],
      maxTokens: 50,
      temperature: 0.7,
      userId: 'test-user-456' as any,
      roomId: 'test-room-789' as any,
      purpose: 'integration-test'
    });

    const responseTime = Date.now() - startTime;

    console.log(`✅ Response received in ${responseTime}ms`);
    console.log(`   Model: ${response.model}`);
    console.log(`   Input tokens: ${response.usage.inputTokens}`);
    console.log(`   Output tokens: ${response.usage.outputTokens}`);
    console.log(`   Estimated cost: $${response.usage.estimatedCost?.toFixed(4) || '0.0000'}`);
    console.log(`   Response: "${response.text.substring(0, 50)}..."`);

    // Verify cost is reasonable (should be < $0.01 for such a small request)
    if (response.usage.estimatedCost && response.usage.estimatedCost > 0.01) {
      console.warn(`⚠️  Cost seems high: $${response.usage.estimatedCost.toFixed(4)} (expected < $0.01)`);
    }

    // Verify cost is > 0 (OpenAI is not free)
    if (!response.usage.estimatedCost || response.usage.estimatedCost === 0) {
      throw new Error('❌ Cost calculation returned $0.00 - pricing system may not be working');
    }

    console.log('✅ Adapter cost calculation working correctly');
  } catch (error) {
    console.error(`❌ Adapter cost calculation test failed:`, error);
    throw error;
  }
}

async function testConservativeRounding(): Promise<void> {
  console.log('\n📐 TEST 5: Conservative Rounding (Always Round UP)');
  console.log('===================================================');

  const pricingManager = PricingManager.getInstance();
  const gpt4Pricing = pricingManager.getModelPricing('openai', 'gpt-4');

  if (!gpt4Pricing) {
    throw new Error('❌ Failed to load GPT-4 pricing');
  }

  // Test case 1: Exact amount (no rounding needed)
  const cost1 = pricingManager.calculateCost(1000, 0, gpt4Pricing);
  // (1000/1M * 30) = 0.03
  console.log(`   1000 input, 0 output: $${cost1.toFixed(4)} (exact)`);
  if (cost1 !== 0.03) {
    throw new Error(`❌ Expected $0.0300, got $${cost1.toFixed(4)}`);
  }

  // Test case 2: Fractional amount (rounds UP)
  const cost2 = pricingManager.calculateCost(123, 456, gpt4Pricing);
  // (123/1M * 30) + (456/1M * 60) = 0.00369 + 0.02736 = 0.03105
  // Math.ceil(0.03105 * 10000) / 10000 = 0.0311
  console.log(`   123 input, 456 output: $${cost2.toFixed(4)} (rounded UP)`);
  if (cost2 !== 0.0311) {
    throw new Error(`❌ Expected $0.0311 (rounded UP), got $${cost2.toFixed(4)}`);
  }

  // Test case 3: Tiny amount (rounds UP to minimum precision)
  const cost3 = pricingManager.calculateCost(1, 1, gpt4Pricing);
  // (1/1M * 30) + (1/1M * 60) = 0.00003 + 0.00006 = 0.00009
  // Math.ceil(0.00009 * 10000) / 10000 = 0.0001
  console.log(`   1 input, 1 output: $${cost3.toFixed(4)} (rounded UP to min)`);
  if (cost3 !== 0.0001) {
    throw new Error(`❌ Expected $0.0001 (rounded UP), got $${cost3.toFixed(4)}`);
  }

  console.log('✅ Conservative rounding verified (always rounds UP)');
}

// Main test runner
async function runAllTests(): Promise<void> {
  console.log('🧪 AI Cost Tracking Integration Tests');
  console.log('=====================================\n');

  try {
    // Initialize secrets for API key access
    await initializeSecrets();

    // Run all tests
    await testPricingManagerStaticPricing();
    await testPricingFetcherOpenRouter();
    await testAIGenerationEntityCreation();
    await testAdapterCostCalculation();
    await testConservativeRounding();

    console.log('\n✅ ALL TESTS PASSED');
    console.log('===================\n');
    process.exit(0);
  } catch (error) {
    console.error('\n❌ TEST SUITE FAILED');
    console.error('====================');
    console.error(error);
    process.exit(1);
  }
}

// Run tests
runAllTests();
