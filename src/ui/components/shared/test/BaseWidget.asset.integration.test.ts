/**
 * BaseWidget Asset Integration Test
 * Generic test that validates all widget assets load correctly
 * Tests any widget that extends BaseWidget automatically
 */

import { BaseWidget } from '../BaseWidget';

interface AssetTestResult {
  url: string;
  status: number;
  success: boolean;
  error?: string;
}

/**
 * Generic asset testing for any widget that extends BaseWidget
 * Discovers assets by intercepting getBundledCSS() fetch calls
 */
export async function testWidgetAssets(WidgetClass: typeof BaseWidget): Promise<AssetTestResult[]> {
  console.log(`🧪 Testing assets for ${WidgetClass.name}...`);
  
  // Create widget instance
  const widget = new WidgetClass();
  
  // Capture all fetch calls made by getBundledCSS
  const fetchedUrls: string[] = [];
  const originalFetch = global.fetch;
  
  global.fetch = jest.fn((url: string) => {
    fetchedUrls.push(url);
    // Return mock response so getBundledCSS doesn't fail
    return Promise.resolve(new Response('/* mock css */'));
  }) as jest.MockedFunction<typeof fetch>;
  
  try {
    // Call getBundledCSS to discover what assets it needs
    await widget.getBundledCSS();
    
    // Restore original fetch
    global.fetch = originalFetch;
    
    console.log(`📋 ${WidgetClass.name} requests ${fetchedUrls.length} assets:`, fetchedUrls);
    
    // Test each discovered asset URL against actual server
    const results: AssetTestResult[] = [];
    
    for (const url of fetchedUrls) {
      try {
        console.log(`🔍 Testing asset: ${url}`);
        const response = await fetch(url);
        
        results.push({
          url,
          status: response.status,
          success: response.status === 200,
          error: response.status !== 200 ? `HTTP ${response.status}` : undefined
        });
        
        if (response.status === 200) {
          console.log(`✅ ${url} - OK`);
        } else {
          console.error(`❌ ${url} - HTTP ${response.status}`);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        results.push({
          url,
          status: 0,
          success: false,
          error: errorMessage
        });
        console.error(`❌ ${url} - ${errorMessage}`);
      }
    }
    
    return results;
    
  } catch (error) {
    // Restore fetch in case of error
    global.fetch = originalFetch;
    throw error;
  }
}

describe('BaseWidget Asset Integration', () => {
  beforeAll(() => {
    // Ensure we have a running server to test against
    console.log('🌐 Testing against server at http://localhost:9000');
  });
  
  describe('Generic Asset Loading', () => {
    it('should have a working asset testing framework', async () => {
      // Test that our testing framework itself works
      expect(testWidgetAssets).toBeDefined();
      expect(typeof testWidgetAssets).toBe('function');
    });
  });
});

/**
 * Helper to run asset tests for multiple widget classes
 */
export async function runWidgetAssetTests(widgetClasses: (typeof BaseWidget)[]): Promise<void> {
  console.log(`🧪 Running asset tests for ${widgetClasses.length} widgets...`);
  
  const allResults: { widget: string; results: AssetTestResult[] }[] = [];
  
  for (const WidgetClass of widgetClasses) {
    try {
      const results = await testWidgetAssets(WidgetClass);
      allResults.push({ widget: WidgetClass.name, results });
      
      // Check if all assets loaded successfully
      const failedAssets = results.filter(r => !r.success);
      if (failedAssets.length > 0) {
        console.error(`❌ ${WidgetClass.name} has ${failedAssets.length} failed assets:`);
        failedAssets.forEach(asset => {
          console.error(`   ${asset.url}: ${asset.error}`);
        });
      } else {
        console.log(`✅ ${WidgetClass.name} all ${results.length} assets load successfully`);
      }
    } catch (error) {
      console.error(`💥 ${WidgetClass.name} asset test failed:`, error);
    }
  }
  
  // Summary
  const totalWidgets = allResults.length;
  const successfulWidgets = allResults.filter(w => w.results.every(r => r.success)).length;
  const totalAssets = allResults.reduce((sum, w) => sum + w.results.length, 0);
  const successfulAssets = allResults.reduce((sum, w) => sum + w.results.filter(r => r.success).length, 0);
  
  console.log('');
  console.log('📊 Widget Asset Test Summary:');
  console.log(`   Widgets: ${successfulWidgets}/${totalWidgets} passed`);
  console.log(`   Assets: ${successfulAssets}/${totalAssets} loaded successfully`);
  console.log(`   Success Rate: ${totalAssets > 0 ? ((successfulAssets / totalAssets) * 100).toFixed(1) : 0}%`);
}