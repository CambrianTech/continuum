/**
 * Simple SavedPersonas Widget Test
 * Just verify the module loads and has basic functionality
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import fs from 'fs';
import path from 'path';

describe('SavedPersonas Widget Simple Tests', () => {

  test('should have valid package.json with continuum metadata', () => {
    const __dirname = path.dirname(new URL(import.meta.url).pathname);
    const packagePath = path.resolve(__dirname, '..', 'package.json');
    
    assert(fs.existsSync(packagePath), 'Package.json should exist');
    
    const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
    
    assert.strictEqual(packageData.continuum.widgetName, 'SavedPersonas');
    assert.strictEqual(packageData.continuum.icon, '👤');
    assert.strictEqual(packageData.continuum.category, 'User Interface');
    assert(Array.isArray(packageData.continuum.selectors));
    assert(packageData.continuum.selectors.length > 0);
    assert(packageData.continuum.capabilities.includes('api-integration'));
    
    console.log('✅ SavedPersonas widget package.json is valid');
  });

  test('should have SavedPersonas component file', () => {
    const __dirname = path.dirname(new URL(import.meta.url).pathname);
    const componentPath = path.resolve(__dirname, '..', 'SavedPersonas.js');
    
    assert(fs.existsSync(componentPath), 'SavedPersonas.js should exist');
    
    const componentContent = fs.readFileSync(componentPath, 'utf8');
    assert(componentContent.includes('SavedPersonas'), 'Component should contain SavedPersonas class');
    assert(componentContent.includes('BaseWidget'), 'Component should extend BaseWidget');
    
    console.log('✅ SavedPersonas component file exists and extends BaseWidget');
  });

  test('should have proper module structure', () => {
    const __dirname = path.dirname(new URL(import.meta.url).pathname);
    const moduleDir = path.resolve(__dirname, '..');
    
    // Check required files
    const requiredFiles = ['package.json', 'index.js', 'SavedPersonas.js'];
    for (const file of requiredFiles) {
      const filePath = path.join(moduleDir, file);
      assert(fs.existsSync(filePath), `${file} should exist`);
    }
    
    // Check test directory
    const testDir = path.join(moduleDir, 'test');
    assert(fs.existsSync(testDir), 'test directory should exist');
    
    console.log('✅ SavedPersonas module structure is correct');
  });

  test('should have shared BaseWidget dependency', () => {
    const __dirname = path.dirname(new URL(import.meta.url).pathname);
    const sharedDir = path.resolve(__dirname, '..', '..', 'shared');
    const baseWidgetPath = path.join(sharedDir, 'BaseWidget.js');
    
    assert(fs.existsSync(baseWidgetPath), 'BaseWidget.js should exist in shared directory');
    
    const baseWidgetContent = fs.readFileSync(baseWidgetPath, 'utf8');
    assert(baseWidgetContent.includes('class BaseWidget'), 'BaseWidget should contain BaseWidget class');
    assert(baseWidgetContent.includes('getHeaderStyle'), 'BaseWidget should have shared header styling');
    
    console.log('✅ SavedPersonas has access to shared BaseWidget');
  });
});