#!/usr/bin/env node
/**
 * Scan Real Command Dependencies
 * Reads actual command package.json files to build dependency graph
 */

const fs = require('fs');
const path = require('path');
const { topologicalSort } = require('./command-dependency-sort.cjs');

function scanCommandDirectories() {
  const commandsBase = 'src/commands/core';
  const storageBase = 'src/storage';
  const widgetsBase = 'src/ui/components';
  const commands = {};
  
  // First scan storage modules (core infrastructure)
  if (fs.existsSync(storageBase)) {
    const storageDirs = fs.readdirSync(storageBase, { withFileTypes: true });
    
    for (const dir of storageDirs) {
      if (!dir.isDirectory()) continue;
      
      const packagePath = path.join(storageBase, dir.name, 'package.json');
      if (!fs.existsSync(packagePath)) continue;
      
      try {
        const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
        const moduleName = packageData.continuum?.moduleName || packageData.name || dir.name;
        
        // Storage modules are core infrastructure with no dependencies
        const deps = packageData.continuum?.dependencies || [];
        commands[moduleName] = deps;
        
        console.log(`🗄️ Found storage: ${moduleName} (deps: ${deps.length > 0 ? deps.join(', ') : 'none'})`);
      } catch (error) {
        console.log(`❌ Error reading ${packagePath}:`, error.message);
      }
    }
  }
  
  // Scan widget modules (UI components)
  if (fs.existsSync(widgetsBase)) {
    const widgetDirs = fs.readdirSync(widgetsBase, { withFileTypes: true });
    
    for (const dir of widgetDirs) {
      if (!dir.isDirectory()) continue;
      
      const packagePath = path.join(widgetsBase, dir.name, 'package.json');
      if (!fs.existsSync(packagePath)) continue;
      
      try {
        const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
        const moduleName = packageData.continuum?.widgetName || packageData.name || dir.name;
        
        // Widget modules typically depend on storage and screenshot capabilities
        const deps = packageData.continuum?.dependencies || ['persistent', 'screenshot'];
        commands[moduleName] = deps;
        
        console.log(`🎨 Found widget: ${moduleName} (deps: ${deps.length > 0 ? deps.join(', ') : 'none'})`);
      } catch (error) {
        console.log(`❌ Error reading ${packagePath}:`, error.message);
      }
    }
  }
  
  // Then scan command modules
  if (!fs.existsSync(commandsBase)) {
    console.log('⚠️  Commands directory not found at:', commandsBase);
    return commands;
  }
  
  const dirs = fs.readdirSync(commandsBase, { withFileTypes: true });
  
  for (const dir of dirs) {
    if (!dir.isDirectory()) continue;
    
    const packagePath = path.join(commandsBase, dir.name, 'package.json');
    if (!fs.existsSync(packagePath)) continue;
    
    try {
      const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
      const commandName = packageData.continuum?.commandName || packageData.name || dir.name;
      
      // Look for dependencies in continuum config
      const deps = packageData.continuum?.dependencies || [];
      commands[commandName] = deps;
      
      console.log(`📦 Found command: ${commandName} (deps: ${deps.length > 0 ? deps.join(', ') : 'none'})`);
    } catch (error) {
      console.log(`❌ Error reading ${packagePath}:`, error.message);
    }
  }
  
  return commands;
}

function testDependencyOrder() {
  console.log('🔍 Scanning for real command dependencies...\n');
  
  const realCommands = scanCommandDirectories();
  
  if (Object.keys(realCommands).length === 0) {
    console.log('⚠️  No commands found, using mock data for testing...');
    // Use our test data
    const mockCommands = {
      'exec': [],
      'filesave': [],
      'screenshot': ['exec', 'filesave'],
      'share': ['screenshot']
    };
    return topologicalSort(mockCommands);
  }
  
  console.log(`\n📊 Found ${Object.keys(realCommands).length} commands`);
  console.log('🔄 Calculating test order...\n');
  
  const testOrder = topologicalSort(realCommands);
  
  console.log('🎯 Recommended test order:');
  testOrder.forEach((cmd, i) => {
    const deps = realCommands[cmd] || [];
    const depStr = deps.length > 0 ? ` (after: ${deps.join(', ')})` : ' (no deps)';
    console.log(`  ${i + 1}. ${cmd}${depStr}`);
  });
  
  return testOrder;
}

if (require.main === module) {
  testDependencyOrder();
}

module.exports = { scanCommandDirectories, testDependencyOrder };