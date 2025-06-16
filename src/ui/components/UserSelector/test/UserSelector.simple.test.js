/**
 * Simple UserSelector Widget Test
 * Just verify the module loads and has basic functionality
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import fs from 'fs';
import path from 'path';

describe('UserSelector Widget Simple Tests', () => {

  test('should have valid package.json with continuum metadata', () => {
    const __dirname = path.dirname(new URL(import.meta.url).pathname);
    const packagePath = path.resolve(__dirname, '..', 'package.json');
    
    assert(fs.existsSync(packagePath), 'Package.json should exist');
    
    const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
    
    assert.strictEqual(packageData.continuum.widgetName, 'UserSelector');
    assert.strictEqual(packageData.continuum.icon, '👥');
    assert.strictEqual(packageData.continuum.category, 'User Interface');
    assert(Array.isArray(packageData.continuum.selectors));
    assert(packageData.continuum.selectors.length > 0);
    
    console.log('✅ UserSelector widget package.json is valid');
  });

  test('should have UserSelector component file', () => {
    const __dirname = path.dirname(new URL(import.meta.url).pathname);
    const componentPath = path.resolve(__dirname, '..', 'UserSelector.js');
    
    assert(fs.existsSync(componentPath), 'UserSelector.js should exist');
    
    const componentContent = fs.readFileSync(componentPath, 'utf8');
    assert(componentContent.includes('UserSelector'), 'Component should contain UserSelector class');
    
    console.log('✅ UserSelector component file exists and contains expected content');
  });

  test('should have proper module structure', () => {
    const __dirname = path.dirname(new URL(import.meta.url).pathname);
    const moduleDir = path.resolve(__dirname, '..');
    
    // Check required files
    const requiredFiles = ['package.json', 'index.js', 'UserSelector.js'];
    for (const file of requiredFiles) {
      const filePath = path.join(moduleDir, file);
      assert(fs.existsSync(filePath), `${file} should exist`);
    }
    
    // Check test directory
    const testDir = path.join(moduleDir, 'test');
    assert(fs.existsSync(testDir), 'test directory should exist');
    
    console.log('✅ UserSelector module structure is correct');
  });
});