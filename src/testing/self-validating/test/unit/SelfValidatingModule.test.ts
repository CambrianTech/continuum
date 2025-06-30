/**
 * Unit tests for SelfValidatingModule
 * 
 * Tests the core functionality of self-validation against module's own configuration
 */

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { SelfValidatingModule } from '../../SelfValidatingModule.js';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test('SelfValidatingModule can validate itself', async () => {
  const modulePath = path.join(__dirname, '../..');
  const result = await SelfValidatingModule.validateSelf(modulePath);
  
  assert.equal(typeof result.modulePath, 'string');
  assert.equal(result.moduleId, 'self-validating-module');
  assert.equal(result.configType, 'module');
  assert.equal(typeof result.testsGenerated, 'number');
  assert.equal(typeof result.testsPassed, 'number');
  assert.equal(typeof result.testsFailed, 'number');
  assert.equal(typeof result.isCompliant, 'boolean');
  
  // Should have capability tests
  assert.equal(Array.isArray(result.capabilityTests), true);
  
  // Should have structure tests  
  assert.equal(Array.isArray(result.structureTests), true);
});

test('SelfValidatingModule detects missing capabilities', async () => {
  // This is a conceptual test - in practice we'd need a mock module
  // For now, test with invalid path
  const result = await SelfValidatingModule.validateSelf('/invalid/path');
  
  assert.equal(result.isCompliant, false);
  assert.equal(result.validationErrors.length > 0, true);
});

test('SelfValidatingModule can generate test content', async () => {
  const modulePath = path.join(__dirname, '../..');
  const testContent = await SelfValidatingModule.generateSelfTest(modulePath);
  
  assert.equal(typeof testContent, 'string');
  assert.equal(testContent.includes('self-validation test'), true);
  assert.equal(testContent.includes('SelfValidatingModule.validateSelf'), true);
  assert.equal(testContent.includes('assert.equal'), true);
});

test('SelfValidatingModule validates structure requirements', async () => {
  const modulePath = path.join(__dirname, '../..');
  const result = await SelfValidatingModule.validateSelf(modulePath);
  
  // Check that it found structure tests
  const packageJsonTest = result.structureTests.find(t => t.requirement.includes('package.json'));
  assert.notEqual(packageJsonTest, undefined);
  assert.equal(packageJsonTest.met, true);
  
  const testDirTest = result.structureTests.find(t => t.requirement.includes('Test directory'));
  assert.notEqual(testDirTest, undefined);
  assert.equal(testDirTest.met, true);
});