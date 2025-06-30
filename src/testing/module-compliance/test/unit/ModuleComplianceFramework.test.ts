/**
 * Unit tests for ModuleComplianceFramework
 * 
 * Tests the core functionality of module validation against configuration contracts
 */

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { ModuleComplianceFramework } from '../../ModuleComplianceFramework.js';
import * as fs from 'fs/promises';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test('ModuleComplianceFramework can validate a well-formed module', async () => {
  // Test with this module itself
  const modulePath = path.join(__dirname, '../..');
  const result = await ModuleComplianceFramework.validateModule(modulePath);
  
  assert.equal(typeof result.modulePath, 'string');
  assert.equal(result.moduleId, 'module-compliance-framework');
  assert.equal(result.configType, 'module');
  assert.equal(Array.isArray(result.errors), true);
  assert.equal(Array.isArray(result.warnings), true);
  
  // Should have found the package.json
  assert.notEqual(result.config, undefined);
});

test('ModuleComplianceFramework detects missing package.json', async () => {
  // Test with non-existent directory
  const result = await ModuleComplianceFramework.validateModule('/non/existent/path');
  
  assert.equal(result.isValid, false);
  assert.equal(result.errors.length > 0, true);
  assert.equal(result.errors[0].includes('Failed to validate module'), true);
});

test('ModuleComplianceFramework can discover modules', async () => {
  const srcPath = path.join(__dirname, '../../../..');
  const results = await ModuleComplianceFramework.validateAllModules(srcPath);
  
  assert.equal(Array.isArray(results), true);
  assert.equal(results.length > 0, true);
  
  // Should find this module
  const thisModule = results.find(r => r.moduleId === 'module-compliance-framework');
  assert.notEqual(thisModule, undefined);
});

test('ModuleComplianceFramework generates valid compliance report', async () => {
  const srcPath = path.join(__dirname, '../../../..');
  const report = await ModuleComplianceFramework.generateComplianceReport(srcPath);
  
  assert.equal(typeof report, 'string');
  assert.equal(report.includes('# Continuum Module Compliance Report'), true);
  assert.equal(report.includes('## Summary'), true);
  assert.equal(report.includes('Total modules:'), true);
});