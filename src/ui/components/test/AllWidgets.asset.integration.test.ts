/**
 * All Widgets Asset Integration Test
 * Runs the generic asset test against all discovered widgets
 * Middle-out approach: test the pattern, catch everything
 */

import { runWidgetAssetTests } from '../shared/test/BaseWidget.asset.integration.test.js';
import { SidebarWidget } from '../Sidebar/SidebarWidget.js';
import { ChatWidget } from '../Chat/ChatWidget.js';

describe('All Widgets Asset Integration', () => {
  beforeAll(() => {
    console.log('🧪 Starting comprehensive widget asset testing...');
    console.log('🎯 Middle-out methodology: test the pattern, catch all widgets');
  });

  it('should load all assets for all widgets successfully', async () => {
    // All widget classes to test
    const widgetClasses = [
      SidebarWidget,
      ChatWidget
      // Add more widgets here as they're created
    ];

    // Run the generic asset test against all widgets
    await runWidgetAssetTests(widgetClasses);

    // Individual assertions for Jest
    for (const WidgetClass of widgetClasses) {
      console.log(`\n🔍 Individual assertion for ${WidgetClass.name}:`);
      
      // Import dynamically to avoid circular dependencies
      const { testWidgetAssets } = await import('../shared/test/BaseWidget.asset.integration.test.js');
      const results = await testWidgetAssets(WidgetClass);
      
      // Assert all assets loaded successfully
      const failedAssets = results.filter(r => !r.success);
      
      expect(failedAssets).toHaveLength(0);
      
      if (failedAssets.length > 0) {
        console.error(`❌ ${WidgetClass.name} failed assets:`, failedAssets);
        throw new Error(`${WidgetClass.name} has ${failedAssets.length} failed assets`);
      }
      
      console.log(`✅ ${WidgetClass.name}: ${results.length} assets tested, all successful`);
    }
  }, 30000); // 30 second timeout for comprehensive testing

  describe('Individual Widget Asset Tests', () => {
    it('should load SidebarWidget assets', async () => {
      const { testWidgetAssets } = await import('../shared/test/BaseWidget.asset.integration.test.js');
      const results = await testWidgetAssets(SidebarWidget);
      
      expect(results.every(r => r.success)).toBe(true);
      expect(results.length).toBeGreaterThan(0); // Should have at least some assets
    });

    it('should load ChatWidget assets', async () => {
      const { testWidgetAssets } = await import('../shared/test/BaseWidget.asset.integration.test.js');
      const results = await testWidgetAssets(ChatWidget);
      
      expect(results.every(r => r.success)).toBe(true);
      expect(results.length).toBeGreaterThan(0); // Should have at least some assets
    });
  });
});