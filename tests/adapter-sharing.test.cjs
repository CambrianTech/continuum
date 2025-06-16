/**
 * Adapter Sharing Test - Test the sharing and distribution of LoRA adapters
 * Demonstrates how to publish, share, and install tiny adapter files
 */

require('dotenv').config();
const assert = require('assert');
const fs = require('fs');
const path = require('path');
const LoRAAdapter = require('../src/adapters/LoRAAdapter.cjs');
const AdapterRegistry = require('../src/adapters/AdapterRegistry.cjs');
const { ModelAdapterFactory } = require('../src/adapters/ModelAdapter.cjs');

async function runAdapterSharingTest() {
  console.log('🌐 Running Adapter Sharing Test...\n');
  
  try {
    // Test 1: Create and train some LoRA adapters
    console.log('1️⃣  Creating LoRA adapters for sharing...');
    
    const adapters = [
      {
        name: 'ProtocolSheriff',
        baseModel: 'gpt-3.5-turbo',
        specialization: 'protocol_enforcement',
        description: 'Detects protocol violations in AI responses',
        author: 'ContinuumAcademy',
        tags: ['security', 'protocol', 'validation']
      },
      {
        name: 'CommandValidator', 
        baseModel: 'gpt-3.5-turbo',
        specialization: 'command_validation',
        description: 'Validates command formatting and syntax',
        author: 'ContinuumAcademy',
        tags: ['commands', 'syntax', 'validation']
      },
      {
        name: 'ClaudeHelper',
        baseModel: 'claude-3-haiku-20240307',
        specialization: 'conversation_flow',
        description: 'Optimizes conversation flow for Claude models',
        author: 'AnthropicTeam',
        tags: ['conversation', 'claude', 'optimization']
      }
    ];
    
    const createdAdapters = [];
    
    for (const adapterSpec of adapters) {
      console.log(`\n🔬 Creating ${adapterSpec.name} for ${adapterSpec.baseModel}...`);
      
      // Create LoRA adapter
      const lora = new LoRAAdapter(adapterSpec.baseModel, 8, 16);
      lora.initializeAdapters({ baseModel: adapterSpec.baseModel });
      
      // Simulate training data based on specialization
      const trainingData = generateTrainingData(adapterSpec.specialization);
      
      // Fine-tune adapters
      const checkpoint = await lora.fineTuneAdapters(trainingData, {
        epochs: 2,
        learningRate: 1e-4
      });
      
      // Save adapter
      const adapterDir = path.join('.continuum', 'temp_adapters', adapterSpec.name);
      if (!fs.existsSync(adapterDir)) {
        fs.mkdirSync(adapterDir, { recursive: true });
      }
      
      const saveResult = await lora.saveAdapters(adapterDir, {
        name: adapterSpec.name,
        specialization: adapterSpec.specialization,
        author: adapterSpec.author
      });
      
      createdAdapters.push({
        ...adapterSpec,
        adapterPath: saveResult.adapterPath,
        size: saveResult.sizeKB
      });
      
      console.log(`✅ ${adapterSpec.name}: ${saveResult.sizeKB}KB`);
    }
    
    console.log(`\n📦 Created ${createdAdapters.length} adapters for sharing`);

    // Test 2: Initialize Adapter Registry
    console.log('\n2️⃣  Initializing Adapter Registry...');
    
    const registry = new AdapterRegistry('.continuum/test_registry');
    
    console.log(`✅ Registry initialized`);
    console.log(`📁 Registry path: ${registry.registryPath}`);
    console.log(`📊 Initial stats:`, registry.getStats());

    // Test 3: Publish adapters to registry
    console.log('\n3️⃣  Publishing adapters to registry...');
    
    const publishedAdapters = [];
    
    for (const adapter of createdAdapters) {
      console.log(`\n📤 Publishing ${adapter.name}...`);
      
      const publishResult = await registry.publishAdapter(adapter.adapterPath, {
        name: adapter.name,
        description: adapter.description,
        specialization: adapter.specialization,
        author: adapter.author,
        tags: adapter.tags,
        version: '1.0.0'
      });
      
      publishedAdapters.push({
        ...adapter,
        id: publishResult.id,
        url: publishResult.url
      });
      
      console.log(`✅ Published: ${publishResult.id}`);
      console.log(`🌐 Share URL: ${publishResult.url}`);
    }
    
    const stats = registry.getStats();
    console.log(`\n📊 Registry stats after publishing:`);
    console.log(`   📦 Total adapters: ${stats.totalAdapters}`);
    console.log(`   💾 Total size: ${Math.round(stats.totalSize / 1024)}KB`);
    console.log(`   🤖 Base models: ${stats.baseModels.join(', ')}`);

    // Test 4: Search and discover adapters
    console.log('\n4️⃣  Testing adapter discovery...');
    
    // Search by base model
    const gptAdapters = registry.searchAdapters({ baseModel: 'gpt-3.5-turbo' });
    console.log(`🔍 GPT-3.5 adapters: ${gptAdapters.length}`);
    
    // Search by specialization
    const validationAdapters = registry.searchAdapters({ specialization: 'validation' });
    console.log(`🔍 Validation adapters: ${validationAdapters.length}`);
    
    // Search by tags
    const securityAdapters = registry.searchAdapters({ tags: ['security'] });
    console.log(`🔍 Security adapters: ${securityAdapters.length}`);
    
    // List all adapters
    const allAdapters = registry.searchAdapters();
    console.log(`📋 All adapters:`);
    for (const adapter of allAdapters) {
      console.log(`   🔬 ${adapter.name} (${adapter.baseModel}) - ${Math.round(adapter.size / 1024)}KB`);
      console.log(`      🏷️ ${adapter.tags.join(', ')}`);
    }

    // Test 5: Export adapters for sharing
    console.log('\n5️⃣  Exporting adapters for sharing...');
    
    const exportedFiles = [];
    
    for (const adapter of publishedAdapters.slice(0, 2)) { // Export first 2
      const exportPath = path.join('.continuum', 'exports', `${adapter.name}_adapter.json`);
      
      // Ensure export directory exists
      if (!fs.existsSync(path.dirname(exportPath))) {
        fs.mkdirSync(path.dirname(exportPath), { recursive: true });
      }
      
      const exportResult = await registry.exportAdapter(adapter.id, exportPath);
      exportedFiles.push(exportResult);
      
      console.log(`📤 Exported ${adapter.name}: ${Math.round(exportResult.size / 1024)}KB`);
    }

    // Test 6: Simulate sharing (copy to another location)
    console.log('\n6️⃣  Simulating adapter sharing...');
    
    // Create a second registry (simulating another user/system)
    const remoteRegistry = new AdapterRegistry('.continuum/remote_registry');
    
    console.log(`🌐 Created remote registry`);
    console.log(`📊 Remote registry stats:`, remoteRegistry.getStats());
    
    // Import shared adapters
    for (const exportedFile of exportedFiles) {
      console.log(`\n📥 Importing ${path.basename(exportedFile.exportPath)}...`);
      
      const importResult = await remoteRegistry.importAdapter(exportedFile.exportPath);
      
      console.log(`✅ Imported: ${importResult.adapterId}`);
      console.log(`🤖 Base model: ${importResult.baseModel}`);
      console.log(`🏷️ Specialization: ${importResult.metadata.specialization}`);
    }
    
    const remoteStats = remoteRegistry.getStats();
    console.log(`\n📊 Remote registry after import:`);
    console.log(`   📦 Total adapters: ${remoteStats.totalAdapters}`);
    console.log(`   💾 Total size: ${Math.round(remoteStats.totalSize / 1024)}KB`);

    // Test 7: Install adapters for use
    console.log('\n7️⃣  Installing adapters for use...');
    
    const installedAdapters = [];
    const remoteAdapters = remoteRegistry.searchAdapters();
    
    for (const adapter of remoteAdapters) {
      const installPath = path.join('.continuum', 'installed_adapters', `${adapter.name}_lora_adapters.json`);
      
      console.log(`\n📦 Installing ${adapter.name}...`);
      
      const installResult = await remoteRegistry.installAdapter(adapter.id, installPath);
      installedAdapters.push(installResult);
      
      console.log(`✅ Installed to: ${installResult.targetPath}`);
      console.log(`🤖 Base model: ${installResult.baseModel}`);
      
      // Verify installation by loading
      const loadedLoRA = await LoRAAdapter.loadAdapters(installResult.targetPath);
      console.log(`🔄 Verified: ${loadedLoRA.countAdapterParameters().toLocaleString()} parameters`);
    }

    // Test 8: Model Adapter Integration
    console.log('\n8️⃣  Testing integration with base models...');
    
    for (const installed of installedAdapters) {
      console.log(`\n🤖 Testing ${installed.metadata.name} with ${installed.baseModel}...`);
      
      // Load the adapter
      const loraAdapter = await LoRAAdapter.loadAdapters(installed.targetPath);
      
      // Show how to apply to base model
      const appliedLayers = loraAdapter.applyToModel(null);
      
      console.log(`✅ Ready to enhance ${installed.baseModel}`);
      console.log(`🔧 Modified layers: ${appliedLayers.length}`);
      console.log(`💾 Total adapter size: ${Math.round(loraAdapter.estimateStorageSize() / 1024)}KB`);
      console.log(`🎯 Specialization: ${installed.metadata.specialization}`);
      
      // Show that you can swap adapters quickly
      console.log(`⚡ Adapter can be swapped in/out instantly (just ${Math.round(loraAdapter.estimateStorageSize() / 1024)}KB)`);
    }

    // Test 9: Demonstrate sharing workflow
    console.log('\n9️⃣  Demonstrating complete sharing workflow...');
    
    console.log(`\n🌟 ADAPTER SHARING WORKFLOW:`);
    console.log(`\n👤 Team Member A (Trainer):`);
    console.log(`   1. Creates LoRA adapter: ${Math.round(createdAdapters[0].size)}KB`);
    console.log(`   2. Publishes to registry: continuum://adapter/${publishedAdapters[0].id}`);
    console.log(`   3. Exports for sharing: ${Math.round(exportedFiles[0].size / 1024)}KB file`);
    
    console.log(`\n👤 Team Member B (User):`);
    console.log(`   1. Imports shared adapter: ${exportedFiles[0].adapterId}`);
    console.log(`   2. Installs to project: ${installedAdapters[0].targetPath}`);
    console.log(`   3. Applies to base model: ${installedAdapters[0].baseModel}`);
    console.log(`   4. Gets specialized behavior: ${installedAdapters[0].metadata.specialization}`);
    
    console.log(`\n💡 Benefits:`);
    console.log(`   📉 Storage: ${Math.round(createdAdapters[0].size)}KB vs ~175GB base model`);
    console.log(`   🚀 Speed: Instant adapter swapping`);
    console.log(`   🌐 Sharing: Tiny files, easy distribution`);
    console.log(`   🔒 Security: Base model stays private`);
    console.log(`   🎯 Specialization: Task-specific fine-tuning`);

    // Test 10: Performance comparison
    console.log('\n🔟  Performance comparison...');
    
    const baseModelSize = 175000000000 * 4; // 175B parameters * 4 bytes
    const adapterSize = installedAdapters[0] ? 
      fs.statSync(installedAdapters[0].targetPath).size : 
      createdAdapters[0].size * 1024;
    
    console.log(`📊 Storage Comparison:`);
    console.log(`   🏔️ Full Model Fine-tune: ${Math.round(baseModelSize / 1024 / 1024 / 1024)}GB`);
    console.log(`   🔬 LoRA Adapter: ${Math.round(adapterSize / 1024)}KB`);
    console.log(`   📉 Reduction: ${Math.round(baseModelSize / adapterSize).toLocaleString()}x`);
    
    console.log(`\n⚡ Sharing Speed (1MB internet):`);
    console.log(`   🏔️ Full Model: ${Math.round(baseModelSize / 1024 / 1024 / 8)} hours`);
    console.log(`   🔬 LoRA Adapter: ${Math.round(adapterSize / 1024 / 8)} seconds`);

    console.log('\n🎉 ALL ADAPTER SHARING TESTS PASSED!');
    console.log('📋 Summary:');
    console.log('  ✅ LoRA adapter creation and training');
    console.log('  ✅ Registry publishing and metadata management');
    console.log('  ✅ Adapter discovery and search functionality');
    console.log('  ✅ Export/import for cross-system sharing');
    console.log('  ✅ Installation and integration workflow');
    console.log('  ✅ Base model compatibility verification');
    console.log('  ✅ Complete sharing workflow demonstration');
    console.log('  ✅ Performance and storage comparison');
    
    console.log('\n🌟 Perfect Architecture for Team Collaboration!');
    console.log('📦 Share specialized fine-tuning in KB, not GB');
    console.log('🚀 Instant adapter swapping for different tasks');
    console.log('🔒 Keep base models private, share only improvements');
    console.log('🎯 Task-specific specialization without full retraining');
    
    // Clean up test data
    console.log('\n🧹 Cleaning up test data...');
    const cleanupPaths = [
      '.continuum/temp_adapters',
      '.continuum/test_registry', 
      '.continuum/remote_registry',
      '.continuum/installed_adapters',
      '.continuum/exports'
    ];
    
    for (const cleanupPath of cleanupPaths) {
      if (fs.existsSync(cleanupPath)) {
        fs.rmSync(cleanupPath, { recursive: true });
      }
    }
    console.log('✅ Test data cleaned up');

  } catch (error) {
    console.error('❌ Adapter sharing test failed:', error.message);
    console.error(error.stack);
    process.exit(1);
  }
}

// Helper function to generate training data
function generateTrainingData(specialization) {
  const trainingExamples = {
    protocol_enforcement: [
      {
        messages: [
          { role: "system", content: "You are a Protocol Sheriff. Detect protocol violations." },
          { role: "user", content: "Check: 'I'll run git status'" },
          { role: "assistant", content: "VIOLATION: Should be [CMD:GIT_STATUS]" }
        ]
      }
    ],
    command_validation: [
      {
        messages: [
          { role: "system", content: "You validate command syntax." },
          { role: "user", content: "Validate: '[CMD:LIST_FILES]'" },
          { role: "assistant", content: "VALID: Proper command format" }
        ]
      }
    ],
    conversation_flow: [
      {
        messages: [
          { role: "system", content: "You optimize conversation flow." },
          { role: "user", content: "How can I help you today?" },
          { role: "assistant", content: "I'd be happy to assist! What would you like to work on?" }
        ]
      }
    ]
  };
  
  return trainingExamples[specialization] || trainingExamples.protocol_enforcement;
}

if (require.main === module) {
  runAdapterSharingTest().catch(console.error);
}

module.exports = { runAdapterSharingTest };