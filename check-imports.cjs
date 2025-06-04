#!/usr/bin/env node
/**
 * Import Checker - Verifies all module imports work correctly
 * This catches broken imports that unit tests might miss
 */

const fs = require('fs');
const path = require('path');

const failures = [];
const successes = [];

// Core modules to test
const modules = [
  'src/core/continuum-core.cjs',
  'src/core/CommandProcessor.cjs', 
  'src/core/Academy.cjs',
  'src/core/Persona.cjs',
  'src/core/PersonaFactory.cjs',
  'src/core/PersonaBootcamp.cjs',
  
  'src/integrations/HttpServer.cjs',
  'src/integrations/WebSocketServer.cjs',
  
  'src/adapters/ModelAdapter.cjs',
  'src/adapters/LoRAAdapter.cjs',
  'src/adapters/HierarchicalAdapter.cjs',
  'src/adapters/AdapterRegistry.cjs',
  
  'src/storage/PersistentStorage.cjs',
  'src/storage/ModelCheckpoint.cjs',
  
  'src/services/TabManager.cjs',
  'src/services/RemoteAgentManager.cjs',
  'src/services/GameManager.cjs',
  'src/services/VisualGameManager.cjs',
  'src/services/WebVisualManager.cjs',
  
  'src/ui/UIGenerator.cjs',
  'src/ui/AcademyWebInterface.cjs'
];

console.log('🔍 Testing module imports...\n');

for (const modulePath of modules) {
  try {
    if (!fs.existsSync(modulePath)) {
      failures.push(`❌ ${modulePath} - File does not exist`);
      continue;
    }

    // Try to require the module
    delete require.cache[require.resolve('./' + modulePath)];
    require('./' + modulePath);
    successes.push(`✅ ${modulePath}`);
    
  } catch (error) {
    failures.push(`❌ ${modulePath} - ${error.message}`);
  }
}

console.log('📊 Import Test Results:\n');

if (successes.length > 0) {
  console.log('🎉 Successful imports:');
  successes.forEach(success => console.log('  ' + success));
  console.log('');
}

if (failures.length > 0) {
  console.log('🚨 Failed imports:');
  failures.forEach(failure => console.log('  ' + failure));
  console.log('');
  console.log(`❌ ${failures.length} modules failed to import`);
  process.exit(1);
} else {
  console.log(`✅ All ${successes.length} modules imported successfully!`);
  process.exit(0);
}