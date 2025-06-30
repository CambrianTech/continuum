/**
 * Integration tests for ModuleComplianceFramework
 * 
 * Tests the framework's ability to validate the entire system and interact with real modules
 */

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { ModuleComplianceFramework } from '../../ModuleComplianceFramework.js';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test('Integration: Can validate entire src directory structure', async () => {
  const srcPath = path.join(__dirname, '../../../../..');
  const results = await ModuleComplianceFramework.validateAllModules(srcPath);
  
  assert.equal(Array.isArray(results), true);
  assert.equal(results.length > 0, true);
  
  // Should find various types of modules
  const commands = results.filter(r => r.configType === 'command');
  const daemons = results.filter(r => r.configType === 'daemon');
  const modules = results.filter(r => r.configType === 'module');
  
  console.log(`Found: ${commands.length} commands, ${daemons.length} daemons, ${modules.length} modules`);
  
  // Validate that each module has the expected structure
  for (const result of results) {
    assert.equal(typeof result.moduleId, 'string');
    assert.equal(typeof result.configType, 'string');
    assert.equal(typeof result.isValid, 'boolean');
    assert.equal(Array.isArray(result.errors), true);
    assert.equal(Array.isArray(result.warnings), true);
  }
});

test('Integration: Self-validation - this module validates itself', async () => {
  const modulePath = path.join(__dirname, '../..');
  const result = await ModuleComplianceFramework.validateModule(modulePath);
  
  // This module should be compliant with its own rules
  if (!result.isValid) {
    console.error('Self-validation failed:', result.errors);
  }
  
  assert.equal(result.isValid, true, 'ModuleComplianceFramework should validate itself');
  assert.equal(result.moduleId, 'module-compliance-framework');
  assert.equal(result.configType, 'module');
});

test('Integration: Can generate comprehensive system report', async () => {
  const srcPath = path.join(__dirname, '../../../../..');
  const report = await ModuleComplianceFramework.generateComplianceReport(srcPath);
  
  // Report should contain comprehensive information
  assert.equal(report.includes('# Continuum Module Compliance Report'), true);
  assert.equal(report.includes('Total modules:'), true);
  assert.equal(report.includes('Valid modules:'), true);
  assert.equal(report.includes('Compliance rate:'), true);
  
  // Should list modules
  assert.equal(report.includes('module-compliance-framework'), true);
  
  console.log('Generated compliance report with', report.split('\n').length, 'lines');
});