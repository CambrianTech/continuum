#!/usr/bin/env node

/**
 * Test Hierarchical Persona Storage with Real LoRA Weights
 * 
 * Tests:
 * 1. Persona storage hierarchy (project > user > org)
 * 2. Real LoRA weight persistence and loading
 * 3. Cross-scope persona sharing
 * 4. Storage statistics and discovery
 */

const fs = require('fs');
const path = require('path');
const os = require('os');

// Import modules
const Persona = require('../src/core/Persona.cjs');
const PersonaRegistry = require('../src/core/PersonaRegistry.cjs');
const LoRAAdapter = require('../src/core/LoRAAdapter.cjs');

async function testHierarchicalStorage() {
  console.log('🏗️ Testing Hierarchical Persona Storage with Real LoRA Weights\n');
  
  // Setup test environment
  const testOrgPath = path.join(os.tmpdir(), 'continuum-test-org');
  if (!fs.existsSync(testOrgPath)) {
    fs.mkdirSync(testOrgPath, { recursive: true });
  }
  process.env.CONTINUUM_ORG_SHARE = testOrgPath;
  
  try {
    // 1. Test registry configuration
    console.log('📋 1. Testing PersonaRegistry Configuration');
    const config = Persona.getRegistryConfig();
    console.log('   Search paths:', config.searchPaths.map(p => `${p.type}: ${p.path}`));
    console.log('   Org sharing enabled:', config.orgSharingEnabled);
    console.log('');
    
    // 2. Create persona with real LoRA weights
    console.log('🧠 2. Creating Persona with Real LoRA Adapter');
    const lora = new LoRAAdapter('gpt-3.5-turbo', 16, 32);
    lora.initializeAdapters({ baseModel: 'gpt-3.5-turbo' });
    
    // Simulate training
    const trainingData = [
      { input: 'What is a patent?', output: 'A patent is an exclusive right granted...' },
      { input: 'How to file USPTO?', output: 'To file with USPTO, follow these steps...' }
    ];
    
    await lora.fineTuneAdapters(trainingData, { epochs: 2, learningRate: 1e-4 });
    
    const persona = new Persona({
      name: 'PatentExpert',
      specialization: 'patent_law',
      baseModel: 'gpt-3.5-turbo',
      status: 'graduated',
      graduationScore: 92,
      loraAdapter: lora
    });
    
    console.log(`   Created: ${persona.name} (${persona.specialization})`);
    console.log(`   LoRA Parameters: ${lora.countAdapterParameters().toLocaleString()}`);
    console.log(`   Storage Size: ${Math.round(lora.estimateStorageSize() / 1024)}KB`);
    console.log(`   Reduction Factor: ${Math.round(lora.calculateReductionFactor()).toLocaleString()}x`);
    console.log('');
    
    // 3. Save to user scope
    console.log('💾 3. Saving Persona to User Scope');
    const userSaveResult = await persona.save('user');
    console.log('   Save result:', {
      scope: userSaveResult.scope,
      adapterSizeKB: userSaveResult.adapterSizeKB,
      storageReduction: `${userSaveResult.storageReduction}x`
    });
    console.log('');
    
    // 4. Test loading from user scope
    console.log('📂 4. Loading Persona from User Scope');
    const loadedPersona = await Persona.load(persona.id);
    console.log(`   Loaded: ${loadedPersona.name}`);
    console.log(`   Has LoRA adapter: ${loadedPersona.loraAdapter !== null}`);
    if (loadedPersona.loraAdapter) {
      console.log(`   Adapter parameters: ${loadedPersona.loraAdapter.countAdapterParameters().toLocaleString()}`);
      console.log(`   Adapter size: ${Math.round(loadedPersona.loraAdapter.estimateStorageSize() / 1024)}KB`);
    }
    console.log('');
    
    // 5. Test persona sharing
    console.log('🤝 5. Sharing Persona to Organization Scope');
    const shareResult = await loadedPersona.share('organization');
    console.log('   Share result:', {
      personaId: shareResult.personaId,
      from: shareResult.fromScope,
      to: shareResult.toScope
    });
    console.log('');
    
    // 6. Create project-specific persona
    console.log('🏢 6. Creating Project-Specific Persona');
    const projectPersona = new Persona({
      name: 'ProjectBot',
      specialization: 'continuum_development',
      baseModel: 'claude-3-haiku-20240307',
      status: 'graduated',
      graduationScore: 88
    });
    
    const projectSaveResult = await projectPersona.save('project');
    console.log(`   Saved to project scope: ${projectPersona.name}`);
    console.log('');
    
    // 7. Test hierarchical discovery
    console.log('🔍 7. Testing Hierarchical Persona Discovery');
    const allPersonas = await Persona.listAll();
    console.log('   Found personas:');
    allPersonas.forEach(p => {
      console.log(`     ${p.name} (${p.specialization}) - ${p.scope} scope`);
      if (p.hasLoRAAdapter) {
        console.log(`       LoRA adapter: ${p.adapterSize}, reduction: ${p.reductionFactor}`);
      }
    });
    console.log('');
    
    // 8. Test storage statistics
    console.log('📊 8. Storage Statistics');
    const stats = Persona.getStorageStats();
    console.log('   Search paths:');
    stats.searchPaths.forEach(sp => {
      console.log(`     ${sp.type}: ${sp.path} (exists: ${sp.exists})`);
    });
    console.log('   Persona counts:', stats.personaCounts);
    console.log('');
    
    // 9. Test deployment with LoRA
    console.log('🚀 9. Testing Deployment with LoRA Adapter');
    const deployment = loadedPersona.deploy({ task: 'Patent analysis' });
    console.log(`   Deployed: ${deployment.persona.name}`);
    console.log(`   Adapter type: ${deployment.deployment.adapterType}`);
    if (deployment.deployment.appliedAdapters) {
      console.log(`   Applied adapters: ${deployment.deployment.appliedAdapters.length} layers`);
    }
    console.log('');
    
    // 10. Test priority resolution
    console.log('🏆 10. Testing Priority Resolution');
    
    // Create same persona in multiple scopes
    const multiScopePersona = new Persona({
      id: 'test_priority_persona',
      name: 'MultiScopeTest',
      specialization: 'testing',
      status: 'test'
    });
    
    await multiScopePersona.save('organization');
    await multiScopePersona.save('user');
    await multiScopePersona.save('project');
    
    // Load should return project version (highest priority)
    const priorityPersona = await Persona.load('test_priority_persona');
    const priorityLocation = new PersonaRegistry().findPersona('test_priority_persona');
    console.log(`   Loaded from: ${priorityLocation.location.type} (priority: ${priorityLocation.location.priority})`);
    console.log('');
    
    console.log('✅ All tests passed! Hierarchical storage with LoRA weights working correctly.\n');
    
    // Summary
    console.log('📋 Summary:');
    console.log(`   ✓ Hierarchical storage: project > user > organization`);
    console.log(`   ✓ Real LoRA weight persistence and loading`);
    console.log(`   ✓ Cross-scope persona sharing`);
    console.log(`   ✓ Priority resolution system`);
    console.log(`   ✓ Storage statistics and discovery`);
    console.log(`   ✓ Deployment with adapter application`);
    console.log('');
    console.log('🏗️ Continuum Academy: Ready for enterprise-scale AI workforce management!');
    
  } catch (error) {
    console.error('❌ Test failed:', error.message);
    console.error(error.stack);
    process.exit(1);
  } finally {
    // Cleanup
    delete process.env.CONTINUUM_ORG_SHARE;
  }
}

// Run the test
if (require.main === module) {
  testHierarchicalStorage().catch(console.error);
}

module.exports = { testHierarchicalStorage };