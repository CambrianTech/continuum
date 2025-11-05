/**
 * Unit tests for ModuleComplianceFramework using object-oriented validation
 */

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { ModuleComplianceFramework } from '../../ModuleComplianceFramework';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test('ModuleComplianceFramework validates module using module.validate()', async () => {
  // Test with this module itself
  const modulePath = path.join(__dirname, '../..');
  const result = await ModuleComplianceFramework.validateModule(modulePath);
  
  assert.equal(typeof result.modulePath, 'string');
  assert.equal(result.moduleId, 'module-compliance-framework');
  assert.equal(result.configType, 'module');
  assert.equal(Array.isArray(result.errors), true);
  assert.equal(Array.isArray(result.warnings), true);
  assert.notEqual(result.config, undefined);
  
  console.log(`✅ Module validation: ${result.isValid ? 'VALID' : 'INVALID'}`);
  if (!result.isValid) {
    console.log(`   Errors: ${result.errors.join(', ')}`);
  }
});

test('ModuleComplianceFramework discovers modules correctly', async () => {
  const srcPath = path.join(__dirname, '../../../..');
  const results = await ModuleComplianceFramework.validateAllModules(srcPath);
  
  assert.equal(Array.isArray(results), true);
  assert.equal(results.length > 0, true);
  
  // Should find this module
  const thisModule = results.find(r => r.moduleId === 'module-compliance-framework');
  assert.notEqual(thisModule, undefined);
  
  console.log(`✅ Discovered ${results.length} modules`);
});

test('ModuleComplianceFramework generates meaningful reports', async () => {
  const srcPath = path.join(__dirname, '../../../..');
  const report = await ModuleComplianceFramework.generateComplianceReport(srcPath);
  
  assert.equal(typeof report, 'string');
  assert.equal(report.includes('# Continuum Module Compliance Report'), true);
  assert.equal(report.includes('## Summary'), true);
  assert.equal(report.includes('Total modules:'), true);
  assert.equal(report.includes('module-compliance-framework'), true);
  
  console.log('✅ Generated compliance report');
});

test('ModuleComplianceFramework uses object-oriented validation internally', async () => {
  // The key insight: compliance testing is just calling module.validate()
  const modulePath = path.join(__dirname, '../..');
  
  // Test that the framework can validate a module
  const result = await ModuleComplianceFramework.validateModule(modulePath);
  
  // Should succeed because the module validates itself
  assert.equal(result.isValid, true);
  assert.equal(result.errors.length, 0);
  
  console.log('✅ Object-oriented validation: Modules validate themselves');
});