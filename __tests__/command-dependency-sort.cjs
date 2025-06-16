#!/usr/bin/env node
/**
 * Command Dependency Sorter
 * Sorts commands by dependencies for proper test order
 */

// Example command dependencies
const commandDeps = {
  'exec': [],                           // No dependencies - test first
  'filesave': [],                       // No dependencies - test first  
  'clear': [],                          // No dependencies - test first
  'screenshot': ['exec', 'filesave'],   // Depends on exec and filesave - test after
  'share': ['screenshot'],              // Depends on screenshot - test last
  'diagnostics': ['exec', 'screenshot'] // Depends on exec and screenshot
};

function topologicalSort(deps) {
  const visited = new Set();
  const result = [];
  
  function visit(command) {
    if (visited.has(command)) return;
    visited.add(command);
    
    // Visit dependencies first
    const dependencies = deps[command] || [];
    dependencies.forEach(dep => visit(dep));
    
    // Add current command after its dependencies
    result.push(command);
  }
  
  Object.keys(deps).forEach(command => visit(command));
  return result;
}

const testOrder = topologicalSort(commandDeps);
console.log('📋 Command Test Order:');
testOrder.forEach((cmd, i) => {
  const deps = commandDeps[cmd];
  const depStr = deps.length > 0 ? ` (needs: ${deps.join(', ')})` : ' (no deps)';
  console.log(`  ${i + 1}. ${cmd}${depStr}`);
});

module.exports = { topologicalSort, commandDeps };