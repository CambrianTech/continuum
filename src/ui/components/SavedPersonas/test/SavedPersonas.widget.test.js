/**
 * SavedPersonas Widget Module Test
 * Tests the SavedPersonas widget functionality
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import { BaseWidgetTest } from '../../shared/BaseWidget.test.js';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

class SavedPersonasTest extends BaseWidgetTest {
  constructor() {
    super('SavedPersonas', 'saved-personas');
  }

  testSidebarWidgetExtension() {
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, `${this.widgetName}.js`), 'utf8');
    
    assert(widgetFile.includes('import(\'../shared/SidebarWidget.js\')'), 'Should import SidebarWidget');
    assert(widgetFile.includes('class SavedPersonas extends SidebarWidget'), 'Should extend SidebarWidget');
    assert(widgetFile.includes('this.renderSidebarStructure'), 'Should use renderSidebarStructure');
    
    console.log('✅ SavedPersonas properly extends SidebarWidget');
    return true;
  }

  testWebSocketIntegration() {
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, `${this.widgetName}.js`), 'utf8');
    
    assert(widgetFile.includes('setupWebSocketListeners'), 'Should have WebSocket listener setup');
    assert(widgetFile.includes('personas_updated'), 'Should listen for persona updates');
    assert(widgetFile.includes('persona_added'), 'Should listen for persona additions');
    assert(!widgetFile.includes('refresh-btn'), 'Should not have manual refresh button');
    
    console.log('✅ SavedPersonas has proper WebSocket integration');
    return true;
  }

  testPersonaSpecificFeatures() {
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, `${this.widgetName}.js`), 'utf8');
    
    assert(widgetFile.includes('getMockPersonas'), 'Should have mock data method');
    assert(widgetFile.includes('getMockPersonas'), 'Should have persona data method');
    assert(widgetFile.includes('persona-item'), 'Should have persona item styling');
    assert(widgetFile.includes('selectPersona'), 'Should have persona selection');
    
    console.log('✅ SavedPersonas has persona-specific features');
    return true;
  }

  async testPersonaScreenshot() {
    // Call base screenshot test with persona-specific selectors
    const screenshotResult = await super.testWidgetScreenshot([
      '.persona-item',
      '[class*="persona"]',
      '.persona-list'
    ]);
    
    // Now we can examine the screenshot result that the base class captured
    assert(screenshotResult.success, 'Screenshot should succeed');
    assert(screenshotResult.filename.includes('savedpersonas'), 'Screenshot filename should include widget name');
    assert(screenshotResult.selectors.includes('saved-personas'), 'Should include base widget selector');
    assert(screenshotResult.selectors.includes('.persona-item'), 'Should include persona-specific selectors');
    
    console.log(`✅ SavedPersonas screenshot captured at: ${screenshotResult.filename}`);
    console.log(`✅ Screenshot included selectors: ${screenshotResult.selectors}`);
    
    return screenshotResult;
  }

  async runAllTests() {
    console.log('\n🧪 Running SavedPersonas widget tests...');
    
    // Run base tests
    await this.runBaseTests();
    
    // Run SavedPersonas-specific tests
    this.testSidebarWidgetExtension();
    this.testWebSocketIntegration();
    this.testPersonaSpecificFeatures();
    await this.testPersonaScreenshot();
    
    console.log('✅ All SavedPersonas tests passed');
  }
}

describe('SavedPersonas Widget Tests', () => {
  const savedPersonasTest = new SavedPersonasTest();

  test('should pass all base widget tests', async () => {
    await savedPersonasTest.runBaseTests();
  });

  test('should extend SidebarWidget properly', () => {
    savedPersonasTest.testSidebarWidgetExtension();
  });

  test('should have WebSocket integration', () => {
    savedPersonasTest.testWebSocketIntegration();
  });

  test('should have persona-specific features', () => {
    savedPersonasTest.testPersonaSpecificFeatures();
  });

  test('should capture persona-specific screenshot', async () => {
    await savedPersonasTest.testPersonaScreenshot();
  });
});