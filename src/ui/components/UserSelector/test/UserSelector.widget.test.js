/**
 * UserSelector Widget Module Test
 * Tests the UserSelector widget functionality and structure
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

describe('UserSelector Widget Module Tests', () => {

  test('should extend SidebarWidget properly', () => {
    const widgetPath = '../UserSelector.js';
    
    const widgetFile = fs.readFileSync(path.resolve(__dirname, widgetPath), 'utf8');
    
    assert(widgetFile.includes('import(\'../shared/SidebarWidget.js\')'), 'Should import SidebarWidget');
    assert(widgetFile.includes('class UserSelector extends SidebarWidget'), 'Should extend SidebarWidget');
    assert(widgetFile.includes('this.widgetName = \'UserSelector\''), 'Should set widget name');
    assert(widgetFile.includes('this.widgetIcon = \'👥\''), 'Should set widget icon');
    
    console.log('✅ UserSelector properly extends SidebarWidget');
  });

  test('should have collapsible header structure', () => {
    const widgetPath = '../UserSelector.js';
    
    const widgetFile = fs.readFileSync(path.resolve(__dirname, widgetPath), 'utf8');
    
    assert(widgetFile.includes('this.renderSidebarStructure'), 'Should use renderSidebarStructure');
    assert(widgetFile.includes('this.getHeaderStyle()'), 'Should use header styling');
    
    console.log('✅ UserSelector has proper collapsible structure');
  });

  test('should have search functionality', () => {
    const widgetPath = '../UserSelector.js';
    
    const widgetFile = fs.readFileSync(path.resolve(__dirname, widgetPath), 'utf8');
    
    assert(widgetFile.includes('search-input'), 'Should have search input');
    assert(widgetFile.includes('searchQuery'), 'Should have search query state');
    assert(widgetFile.includes('filterAndRender'), 'Should have filtering method');
    
    console.log('✅ UserSelector has search functionality');
  });

  test('should have user and agent management', () => {
    const widgetPath = '../UserSelector.js';
    
    const widgetFile = fs.readFileSync(path.resolve(__dirname, widgetPath), 'utf8');
    
    assert(widgetFile.includes('getDefaultAgents'), 'Should have default agents');
    assert(widgetFile.includes('getDefaultUsers'), 'Should have default users');
    assert(widgetFile.includes('selectAgent'), 'Should have agent selection');
    assert(widgetFile.includes('favoriteAgents'), 'Should have favorites functionality');
    
    console.log('✅ UserSelector has user and agent management');
  });

  test('should export properly for modules', () => {
    const widgetPath = '../UserSelector.js';
    
    const widgetFile = fs.readFileSync(path.resolve(__dirname, widgetPath), 'utf8');
    
    assert(widgetFile.includes('customElements.define(\'user-selector\''), 'Should register custom element');
    assert(widgetFile.includes('module.exports = UserSelector'), 'Should export for CommonJS');
    assert(!widgetFile.includes('window.AgentSelector'), 'Should not have old AgentSelector references');
    
    console.log('✅ UserSelector exports properly');
  });
});