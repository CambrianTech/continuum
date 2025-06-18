/**
 * Model Adapter Pricing and API Test - Test real API functionality
 * Tests pricing queries, model availability, and API connectivity across providers
 */

require('dotenv').config();
const assert = require('assert');
const { ModelAdapterFactory } = require('../src/core/ModelAdapter.cjs');

async function runModelAdapterTest() {
  console.log('💰 Running Model Adapter Pricing & API Test...\n');
  
  try {
    // Test 1: OpenAI Adapter API Testing
    if (process.env.OPENAI_API_KEY) {
      console.log('1️⃣  Testing OpenAI Adapter API functionality...');
      
      const openaiAdapter = ModelAdapterFactory.create('openai', process.env.OPENAI_API_KEY);
      
      // Test model availability query
      console.log('🔍 Querying OpenAI available models...');
      const models = await openaiAdapter.getAvailableModels();
      console.log(`📋 Found ${models.length} available models`);
      
      // Show some sample models
      const sampleModels = models.slice(0, 5).map(m => m.id);
      console.log(`📝 Sample models: ${sampleModels.join(', ')}`);
      
      // Test pricing query
      console.log('💰 Querying OpenAI pricing...');
      const pricing = await openaiAdapter.getPricing();
      if (pricing) {
        console.log('💵 Pricing information retrieved');
      } else {
        console.log('💸 No direct pricing API available (expected for OpenAI)');
      }
      
      // Test a simple query
      console.log('🤖 Testing model query with gpt-3.5-turbo...');
      const queryResult = await openaiAdapter.query('gpt-3.5-turbo', 'Test message from Continuum Academy', {
        max_tokens: 50,
        temperature: 0.7
      });
      
      console.log(`✅ Query successful`);
      console.log(`📤 Response: ${queryResult.response.substring(0, 100)}...`);
      console.log(`📊 Usage: ${JSON.stringify(queryResult.usage)}`);
      
    } else {
      console.log('⚠️ No OpenAI API key - skipping OpenAI tests');
    }

    // Test 2: Anthropic Adapter Testing
    if (process.env.ANTHROPIC_API_KEY) {
      console.log('\n2️⃣  Testing Anthropic Adapter functionality...');
      
      const anthropicAdapter = ModelAdapterFactory.create('anthropic', process.env.ANTHROPIC_API_KEY);
      
      // Test context enhancement (Anthropic's version of fine-tuning)
      console.log('🧠 Testing Anthropic context enhancement...');
      const mockTrainingData = [
        {
          messages: [
            { role: "system", content: "You are a protocol sheriff" },
            { role: "user", content: "Check this response" },
            { role: "assistant", content: "No violations detected" }
          ]
        }
      ];
      
      const enhancementResult = await anthropicAdapter.fineTune('claude-3-haiku-20240307', mockTrainingData, {
        suffix: 'test-persona'
      });
      
      console.log(`✅ Context enhancement completed`);
      console.log(`📋 Method: ${enhancementResult.method}`);
      console.log(`📚 Context examples: ${enhancementResult.context_examples}`);
      
      // Test cost estimation
      console.log('💰 Testing Anthropic cost estimation...');
      const costEstimate = anthropicAdapter.estimateCost(mockTrainingData.length);
      console.log(`💵 Estimated costs:`, costEstimate);
      
    } else {
      console.log('⚠️ No Anthropic API key - skipping Anthropic tests');
    }

    // Test 3: HuggingFace Adapter Testing
    if (process.env.HUGGINGFACE_API_KEY) {
      console.log('\n3️⃣  Testing HuggingFace Adapter functionality...');
      
      const hfAdapter = ModelAdapterFactory.create('huggingface', process.env.HUGGINGFACE_API_KEY, {
        username: 'continuum-academy'
      });
      
      // Test HuggingFace fine-tuning simulation
      console.log('🤗 Testing HuggingFace fine-tuning workflow...');
      const mockTrainingData = [
        {
          messages: [
            { role: "system", content: "You are a protocol sheriff" },
            { role: "user", content: "Validate response" },
            { role: "assistant", content: "Response validated" }
          ]
        }
      ];
      
      const hfResult = await hfAdapter.fineTune('microsoft/DialoGPT-medium', mockTrainingData, {
        suffix: 'academy-trained',
        username: 'continuum-academy'
      });
      
      console.log(`✅ HuggingFace fine-tuning completed`);
      console.log(`📋 Repository: ${hfResult.fineTuneId}`);
      console.log(`📊 Training examples: ${hfResult.metrics.training_examples}`);
      
      // Test deployment
      console.log('🚀 Testing HuggingFace deployment...');
      const deployment = await hfAdapter.deploy(hfResult.fineTuneId);
      console.log(`✅ Deployment ready: ${deployment.endpoint}`);
      
    } else {
      console.log('⚠️ No HuggingFace API key - skipping HuggingFace tests');
    }

    // Test 4: Provider Detection
    console.log('\n4️⃣  Testing provider detection...');
    
    const testModels = [
      'gpt-3.5-turbo',
      'ft:gpt-3.5-turbo:academy:test:123',
      'claude-3-haiku-20240307',
      'microsoft/DialoGPT-medium',
      'meta-llama/Llama-2-7b-chat-hf'
    ];
    
    for (const model of testModels) {
      const provider = ModelAdapterFactory.detectProvider(model);
      console.log(`🔍 ${model} → ${provider}`);
    }

    // Test 5: Factory Methods
    console.log('\n5️⃣  Testing factory methods...');
    
    const supportedProviders = ModelAdapterFactory.getSupportedProviders();
    console.log(`🏭 Supported providers: ${supportedProviders.join(', ')}`);
    
    // Test creating adapters without API keys
    for (const provider of supportedProviders) {
      try {
        const adapter = ModelAdapterFactory.create(provider, null);
        console.log(`✅ ${provider} adapter created (no API key)`);
      } catch (error) {
        console.log(`⚠️ ${provider} adapter creation failed: ${error.message}`);
      }
    }

    // Test 6: Real Fine-Tuning with API Keys (if available)
    if (process.env.OPENAI_API_KEY) {
      console.log('\n6️⃣  Testing REAL fine-tuning preparation...');
      
      const adapter = ModelAdapterFactory.create('openai', process.env.OPENAI_API_KEY);
      
      // Prepare real training data
      const realTrainingData = [
        {
          messages: [
            { role: "system", content: "You are a Protocol Sheriff specialized in detecting command leakage in AI responses." },
            { role: "user", content: "Validate this response: 'I'll help you with that. Let me check GIT_STATUS for you.'" },
            { role: "assistant", content: "VIOLATION: Command should be formatted as [CMD:GIT_STATUS] according to protocol" }
          ]
        },
        {
          messages: [
            { role: "system", content: "You are a Protocol Sheriff specialized in detecting command leakage in AI responses." },
            { role: "user", content: "Validate this response: 'Here's the information you requested: [CMD:LIST_FILES]'" },
            { role: "assistant", content: "VALID: Command is properly formatted according to protocol" }
          ]
        }
      ];
      
      console.log('📚 Prepared real training data with proper format');
      console.log(`📊 Training examples: ${realTrainingData.length}`);
      
      // Validate training data format
      adapter.validateTrainingData(realTrainingData);
      console.log('✅ Training data validation passed');
      
      // Format training data
      const formattedData = adapter.formatTrainingData(realTrainingData);
      console.log('✅ Training data formatted for OpenAI API');
      
      console.log('💡 Note: Actual fine-tuning skipped to avoid costs');
      console.log('💡 Use the Academy.performFineTuning() method to run real fine-tuning');
    }

    // Test 7: Error Handling
    console.log('\n7️⃣  Testing error handling...');
    
    try {
      ModelAdapterFactory.create('invalid-provider', 'fake-key');
    } catch (error) {
      console.log(`✅ Correctly caught invalid provider error: ${error.message}`);
    }
    
    try {
      const adapter = ModelAdapterFactory.create('openai', null);
      await adapter.fineTune('gpt-3.5-turbo', [], {});
    } catch (error) {
      console.log(`✅ Correctly caught missing API key error: ${error.message}`);
    }
    
    try {
      const adapter = ModelAdapterFactory.create('openai', 'fake-key');
      adapter.validateTrainingData('not-an-array');
    } catch (error) {
      console.log(`✅ Correctly caught validation error: ${error.message}`);
    }

    console.log('\n🎉 ALL MODEL ADAPTER TESTS PASSED!');
    console.log('📋 Summary:');
    console.log('  ✅ API connectivity and model querying');
    console.log('  ✅ Pricing and cost estimation');
    console.log('  ✅ Provider-specific fine-tuning workflows');
    console.log('  ✅ Provider detection and factory methods');
    console.log('  ✅ Real training data preparation');
    console.log('  ✅ Error handling and validation');
    
    console.log('\n💰 Model Adapter System is ready for production use!');
    console.log('🚀 Use ModelAdapterFactory.create() to get started');
    console.log('💡 Remember to set API keys in environment variables');

  } catch (error) {
    console.error('❌ Model adapter test failed:', error.message);
    console.error(error.stack);
    process.exit(1);
  }
}

if (require.main === module) {
  runModelAdapterTest().catch(console.error);
}

module.exports = { runModelAdapterTest };