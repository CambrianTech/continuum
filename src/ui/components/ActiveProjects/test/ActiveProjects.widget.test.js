/**
 * ActiveProjects Widget Module Test
 * Tests the ActiveProjects widget functionality
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import { BaseWidgetTest } from '../../shared/BaseWidget.test.js';

class ActiveProjectsTest extends BaseWidgetTest {
  constructor() {
    super('ActiveProjects', 'active-projects');
  }

  testSidebarWidgetExtension() {
    const fs = require('fs');
    const path = require('path');
    
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, `${this.widgetName}.js`), 'utf8');
    
    assert(widgetFile.includes('import(\'../shared/SidebarWidget.js\')'), 'Should import SidebarWidget');
    assert(widgetFile.includes('class ActiveProjects extends SidebarWidget'), 'Should extend SidebarWidget');
    assert(widgetFile.includes('this.renderSidebarStructure'), 'Should use renderSidebarStructure');
    
    console.log('✅ ActiveProjects properly extends SidebarWidget');
    return true;
  }

  testProjectFilteringFeatures() {
    const fs = require('fs');
    const path = require('path');
    
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, `${this.widgetName}.js`), 'utf8');
    
    assert(widgetFile.includes('filter-tabs'), 'Should have filter tabs');
    assert(widgetFile.includes('getFilteredProjects'), 'Should have filter method');
    assert(widgetFile.includes('setFilter'), 'Should have setFilter method');
    assert(widgetFile.includes('project-item'), 'Should have project item styling');
    
    console.log('✅ ActiveProjects has filtering functionality');
    return true;
  }

  testProjectSpecificFeatures() {
    const fs = require('fs');
    const path = require('path');
    
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, `${this.widgetName}.js`), 'utf8');
    
    assert(widgetFile.includes('getMockProjects'), 'Should have mock data method');
    assert(widgetFile.includes('Continuum Modular Widgets'), 'Should have mock project data');
    assert(widgetFile.includes('selectProject'), 'Should have project selection');
    assert(widgetFile.includes('createProject'), 'Should have project creation');
    
    console.log('✅ ActiveProjects has project-specific features');
    return true;
  }

  async testProjectScreenshot() {
    // Call base screenshot test with project-specific selectors
    const screenshotResult = await super.testWidgetScreenshot([
      '.project-item',
      '.filter-tabs',
      '[class*="project"]',
      '.project-list'
    ]);
    
    // Examine the screenshot result that the base class captured
    assert(screenshotResult.success, 'Screenshot should succeed');
    assert(screenshotResult.filename.includes('activeprojects'), 'Screenshot filename should include widget name');
    assert(screenshotResult.selectors.includes('active-projects'), 'Should include base widget selector');
    assert(screenshotResult.selectors.includes('.project-item'), 'Should include project-specific selectors');
    
    console.log(`✅ ActiveProjects screenshot captured at: ${screenshotResult.filename}`);
    console.log(`✅ Screenshot included selectors: ${screenshotResult.selectors}`);
    
    return screenshotResult;
  }

  async runAllTests() {
    console.log('\n🧪 Running ActiveProjects widget tests...');
    
    // Run base tests
    await this.runBaseTests();
    
    // Run ActiveProjects-specific tests
    this.testSidebarWidgetExtension();
    this.testProjectFilteringFeatures();
    this.testProjectSpecificFeatures();
    await this.testProjectScreenshot();
    
    console.log('✅ All ActiveProjects tests passed');
  }
}

describe('ActiveProjects Widget Tests', () => {
  const activeProjectsTest = new ActiveProjectsTest();

  test('should pass all base widget tests', async () => {
    await activeProjectsTest.runBaseTests();
  });

  test('should extend SidebarWidget properly', () => {
    activeProjectsTest.testSidebarWidgetExtension();
  });

  test('should have project filtering features', () => {
    activeProjectsTest.testProjectFilteringFeatures();
  });

  test('should have project-specific features', () => {
    activeProjectsTest.testProjectSpecificFeatures();
  });

  test('should capture project-specific screenshot', async () => {
    await activeProjectsTest.testProjectScreenshot();
  });
});