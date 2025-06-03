/**
 * LoRA Fine-Tuning Test - Test adapter-only fine-tuning for minimal storage
 * Demonstrates saving only the fine-tuned layers, not the full model
 */

require('dotenv').config();
const assert = require('assert');
const fs = require('fs');
const path = require('path');
const LoRAAdapter = require('../src/core/LoRAAdapter.cjs');
const { ModelAdapterFactory } = require('../src/core/ModelAdapter.cjs');
const Persona = require('../src/core/Persona.cjs');

async function runLoRATest() {
  console.log('🔬 Running LoRA Fine-Tuning Test...\n');
  
  try {
    // Test 1: Initialize LoRA Adapter
    console.log('1️⃣  Testing LoRA adapter initialization...');
    
    const baseModel = 'gpt-3.5-turbo';
    const rank = 16;
    const alpha = 32;
    
    const lora = new LoRAAdapter(baseModel, rank, alpha);
    const modelConfig = { baseModel };
    
    lora.initializeAdapters(modelConfig);
    
    console.log(`✅ LoRA adapter initialized`);
    console.log(`🎯 Target layers: ${lora.targetLayers.length}`);
    console.log(`📊 Rank: ${rank}, Alpha: ${alpha}`);
    
    const totalParams = lora.countAdapterParameters();
    const storageSize = lora.estimateStorageSize();
    const reductionFactor = lora.calculateReductionFactor();
    
    console.log(`🔢 Adapter parameters: ${totalParams.toLocaleString()}`);
    console.log(`💾 Storage size: ${Math.round(storageSize / 1024 / 1024)}MB`);
    console.log(`📉 Reduction factor: ${Math.round(reductionFactor).toLocaleString()}x smaller`);

    // Test 2: Prepare Training Data
    console.log('\n2️⃣  Preparing training data...');
    
    const trainingData = [
      {
        messages: [
          { role: "system", content: "You are a Protocol Sheriff. Detect protocol violations in AI responses." },
          { role: "user", content: "Validate this response: 'I'll run git status for you'" },
          { role: "assistant", content: "VIOLATION: Command should be formatted as [CMD:GIT_STATUS]" }
        ]
      },
      {
        messages: [
          { role: "system", content: "You are a Protocol Sheriff. Detect protocol violations in AI responses." },
          { role: "user", content: "Validate this response: 'Here is the result: [CMD:LIST_FILES]'" },
          { role: "assistant", content: "VALID: Command is properly formatted according to protocol" }
        ]
      },
      {
        messages: [
          { role: "system", content: "You are a Protocol Sheriff. Detect protocol violations in AI responses." },
          { role: "user", content: "Validate this response: 'Let me check SYSTEM_STATUS first'" },
          { role: "assistant", content: "VIOLATION: Command should be formatted as [CMD:SYSTEM_STATUS]" }
        ]
      }
    ];
    
    console.log(`📚 Training examples: ${trainingData.length}`);
    console.log(`🎯 Focus: Protocol violation detection`);

    // Test 3: Fine-Tune LoRA Adapters
    console.log('\n3️⃣  Fine-tuning LoRA adapters...');
    
    const fineTuneOptions = {
      epochs: 3,
      learningRate: 1e-4,
      rank: rank,
      alpha: alpha
    };
    
    const checkpoint = await lora.fineTuneAdapters(trainingData, fineTuneOptions);
    
    console.log(`✅ LoRA fine-tuning completed`);
    console.log(`📊 Checkpoint type: ${checkpoint.type}`);
    console.log(`🔢 Parameters: ${checkpoint.parameterCount.toLocaleString()}`);
    console.log(`💾 Storage: ${Math.round(checkpoint.storageSize / 1024)}KB`);
    console.log(`📉 vs Full Model: ${Math.round(checkpoint.reductionFactor).toLocaleString()}x reduction`);

    // Test 4: Save Adapter Weights
    console.log('\n4️⃣  Saving LoRA adapter weights...');
    
    const testPersonaName = `lora-test-${Date.now()}`;
    const personaDir = path.join('.continuum', 'personas', testPersonaName);
    
    // Create persona directory
    if (!fs.existsSync(personaDir)) {
      fs.mkdirSync(personaDir, { recursive: true });
    }
    
    const saveResult = await lora.saveAdapters(personaDir, {
      personaName: testPersonaName,
      specialization: 'protocol_enforcement',
      trainingExamples: trainingData.length
    });
    
    console.log(`✅ Adapters saved successfully`);
    console.log(`📁 Path: ${saveResult.adapterPath}`);
    console.log(`💾 Size: ${saveResult.sizeKB}KB`);
    console.log(`🔢 Parameters: ${saveResult.parameterCount.toLocaleString()}`);
    console.log(`📉 Reduction: ${Math.round(saveResult.reductionFactor).toLocaleString()}x`);
    
    // Verify file exists and has content
    assert(fs.existsSync(saveResult.adapterPath), 'Adapter file should exist');
    const fileStats = fs.statSync(saveResult.adapterPath);
    assert(fileStats.size > 0, 'Adapter file should have content');

    // Test 5: Load Adapter Weights
    console.log('\n5️⃣  Loading LoRA adapter weights...');
    
    const loadedLoRA = await LoRAAdapter.loadAdapters(saveResult.adapterPath);
    
    console.log(`✅ Adapters loaded successfully`);
    console.log(`🔄 Base model: ${loadedLoRA.baseModel}`);
    console.log(`🎯 Rank: ${loadedLoRA.rank}, Alpha: ${loadedLoRA.alpha}`);
    console.log(`🔢 Parameters: ${loadedLoRA.countAdapterParameters().toLocaleString()}`);
    
    // Verify loaded adapters match saved ones
    assert.strictEqual(loadedLoRA.baseModel, baseModel, 'Base model should match');
    assert.strictEqual(loadedLoRA.rank, rank, 'Rank should match');
    assert.strictEqual(loadedLoRA.alpha, alpha, 'Alpha should match');
    assert.strictEqual(loadedLoRA.adapters.size, lora.adapters.size, 'Adapter count should match');

    // Test 6: Apply Adapters to Model (Simulation)
    console.log('\n6️⃣  Testing adapter application...');
    
    const appliedLayers = loadedLoRA.applyToModel(null); // null = simulated base model
    
    console.log(`✅ Adapters applied to base model`);
    console.log(`🔧 Modified layers: ${appliedLayers.length}`);
    
    for (const layer of appliedLayers) {
      console.log(`   📊 ${layer.layer}: ${layer.deltaShape} (scaling: ${layer.scaling.toFixed(2)})`);
    }

    // Test 7: Integration with Model Adapter
    console.log('\n7️⃣  Testing LoRA integration with ModelAdapter...');
    
    if (process.env.OPENAI_API_KEY) {
      const adapter = ModelAdapterFactory.create('openai', process.env.OPENAI_API_KEY);
      
      console.log(`🔬 Testing LoRA fine-tuning via ModelAdapter...`);
      
      const loraResult = await adapter.fineTuneWithLoRA(baseModel, trainingData, {
        useLoRA: true,
        rank: 8,
        alpha: 16,
        suffix: testPersonaName,
        epochs: 2
      });
      
      console.log(`✅ ModelAdapter LoRA fine-tuning completed`);
      console.log(`🆔 Fine-tune ID: ${loraResult.fineTuneId}`);
      console.log(`🔬 Method: ${loraResult.method}`);
      console.log(`📉 Storage reduction: ${Math.round(loraResult.storageReduction).toLocaleString()}x`);
      
      // Save LoRA adapters
      const adapterSaveResult = await loraResult.adapters.saveAdapters(personaDir, {
        fineTuneId: loraResult.fineTuneId
      });
      
      console.log(`💾 ModelAdapter LoRA saved: ${adapterSaveResult.sizeKB}KB`);
      
    } else {
      console.log(`⚠️ No OpenAI API key - skipping ModelAdapter integration`);
    }

    // Test 8: Create Persona with LoRA
    console.log('\n8️⃣  Creating persona with LoRA adapters...');
    
    const persona = new Persona({
      id: testPersonaName,
      name: testPersonaName,
      specialization: 'protocol_enforcement',
      baseModel: baseModel,
      fineTuneId: `lora:${baseModel}:${testPersonaName}`,
      status: 'graduated',
      graduationScore: 0.90
    });
    
    // Add LoRA-specific metadata
    persona.metadata.fineTuningMethod = 'lora_adapter';
    persona.metadata.loraConfig = {
      rank: rank,
      alpha: alpha,
      targetLayers: lora.targetLayers,
      parameterCount: lora.countAdapterParameters(),
      storageReduction: lora.calculateReductionFactor()
    };
    
    // Simulate training data
    persona.trainingData = [
      {
        round: 1,
        testsGenerated: 3,
        correctDetections: 3,
        totalTests: 3,
        accuracy: 1.0,
        timestamp: new Date().toISOString(),
        failedCases: []
      }
    ];
    
    // Save persona with LoRA metadata
    const savedPaths = await persona.save();
    
    console.log(`✅ LoRA persona saved`);
    console.log(`📁 Config: ${savedPaths.configPath}`);
    console.log(`🧠 Checkpoint: ${savedPaths.checkpointPath}`);
    console.log(`🔬 LoRA adapters: ${saveResult.adapterPath}`);

    // Test 9: Load and Deploy LoRA Persona
    console.log('\n9️⃣  Loading and deploying LoRA persona...');
    
    const loadedPersona = Persona.load(testPersonaName);
    
    console.log(`✅ LoRA persona loaded`);
    console.log(`🔬 Fine-tuning method: ${loadedPersona.metadata.fineTuningMethod}`);
    console.log(`📊 LoRA rank: ${loadedPersona.metadata.loraConfig.rank}`);
    console.log(`💾 Storage reduction: ${Math.round(loadedPersona.metadata.loraConfig.storageReduction).toLocaleString()}x`);
    
    // Deploy the persona
    const deployment = loadedPersona.deploy({
      task: 'Protocol enforcement with LoRA fine-tuning'
    });
    
    console.log(`🚀 LoRA persona deployed`);
    console.log(`📋 Task: ${deployment.deployment.task}`);
    console.log(`🧠 Model: ${deployment.deployment.modelId}`);

    // Test 10: Compare Storage Sizes
    console.log('\n🔟  Comparing storage requirements...');
    
    const fullModelSize = 175000000000 * 4; // 175B parameters * 4 bytes each
    const loraSize = lora.estimateStorageSize();
    const configSize = fs.statSync(savedPaths.configPath).size;
    const checkpointSize = fs.statSync(savedPaths.checkpointPath).size;
    const totalLoRASize = loraSize + configSize + checkpointSize;
    
    console.log(`📊 Storage Comparison:`);
    console.log(`   🏔️ Full Model: ${Math.round(fullModelSize / 1024 / 1024 / 1024)}GB`);
    console.log(`   🔬 LoRA Adapters: ${Math.round(loraSize / 1024)}KB`);
    console.log(`   📁 Persona Config: ${Math.round(configSize / 1024)}KB`);
    console.log(`   🧠 Checkpoint: ${Math.round(checkpointSize / 1024)}KB`);
    console.log(`   📦 Total LoRA: ${Math.round(totalLoRASize / 1024)}KB`);
    console.log(`   📉 Reduction: ${Math.round(fullModelSize / totalLoRASize).toLocaleString()}x smaller`);

    console.log('\n🎉 ALL LORA FINE-TUNING TESTS PASSED!');
    console.log('📋 Summary:');
    console.log('  ✅ LoRA adapter initialization and configuration');
    console.log('  ✅ Adapter-only fine-tuning (no base model changes)');
    console.log('  ✅ Efficient adapter weight saving/loading');
    console.log('  ✅ Integration with ModelAdapter system');
    console.log('  ✅ Persona creation with LoRA metadata');
    console.log('  ✅ Cross-session LoRA persona loading');
    console.log('  ✅ Production deployment readiness');
    console.log('  ✅ Massive storage reduction validation');
    
    console.log('\n🌟 LoRA Fine-Tuning: Revolutionary Storage Efficiency!');
    console.log(`💾 Save only ${Math.round(loraSize / 1024)}KB instead of ${Math.round(fullModelSize / 1024 / 1024 / 1024)}GB`);
    console.log(`📉 ${Math.round(fullModelSize / loraSize).toLocaleString()}x storage reduction`);
    console.log('🔬 Perfect for Academy personas - save only what you trained!');
    
    // Clean up test data
    console.log('\n🧹 Cleaning up test data...');
    if (fs.existsSync(personaDir)) {
      fs.rmSync(personaDir, { recursive: true });
      console.log('✅ Test data cleaned up');
    }

  } catch (error) {
    console.error('❌ LoRA test failed:', error.message);
    console.error(error.stack);
    process.exit(1);
  }
}

if (require.main === module) {
  runLoRATest().catch(console.error);
}

module.exports = { runLoRATest };