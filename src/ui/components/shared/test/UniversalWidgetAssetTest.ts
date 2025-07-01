/**
 * Universal Widget Asset Test Framework
 * Tests any widget's declared assets automatically
 * Middle-out methodology: test the pattern, catch all widgets
 */

import { BaseWidget } from '../BaseWidget.js';

export interface AssetTestResult {
  widget: string;
  assetType: 'css' | 'html' | 'js';
  assetPath: string;
  status: number;
  success: boolean;
  error?: string;
  size?: number;
}

export class UniversalWidgetAssetTester {
  private baseUrl: string;
  
  constructor(baseUrl = 'http://localhost:9000') {
    this.baseUrl = baseUrl;
  }
  
  /**
   * Test all assets for a widget class
   */
  async testWidgetAssets(WidgetClass: typeof BaseWidget): Promise<AssetTestResult[]> {
    console.log(`🧪 Testing assets for ${WidgetClass.name}...`);
    
    const results: AssetTestResult[] = [];
    
    // Test CSS assets
    const allAssets = await WidgetClass.getWidgetAssets();
    const cssAssets = allAssets.filter(asset => asset.endsWith('.css'));
    for (const cssPath of cssAssets) {
      const result = await this.testAsset(WidgetClass.name, 'css', cssPath);
      results.push(result);
    }
    
    // Test HTML assets
    const allFiles = await WidgetClass.getWidgetFiles();
    const htmlAssets = allFiles.filter(file => file.endsWith('.html'));
    const basePath = WidgetClass.getBasePath();
    for (const htmlFile of htmlAssets) {
      const htmlPath = `${basePath}/${htmlFile}`;
      const result = await this.testAsset(WidgetClass.name, 'html', htmlPath);
      results.push(result);
    }
    
    // Test the widget's TypeScript file itself
    const tsPath = `${basePath}/${WidgetClass.name}.ts`;
    const jsResult = await this.testAsset(WidgetClass.name, 'js', tsPath);
    results.push(jsResult);
    
    const failedAssets = results.filter(r => !r.success);
    const totalAssets = results.length;
    
    console.log(`📊 ${WidgetClass.name}: ${totalAssets - failedAssets.length}/${totalAssets} assets successful`);
    
    if (failedAssets.length > 0) {
      console.error(`❌ ${WidgetClass.name} failed assets:`, failedAssets.map(a => a.assetPath));
    } else {
      console.log(`✅ ${WidgetClass.name}: All assets load successfully`);
    }
    
    return results;
  }
  
  /**
   * Test a single asset URL
   */
  private async testAsset(widgetName: string, assetType: 'css' | 'html' | 'js', assetPath: string): Promise<AssetTestResult> {
    try {
      const url = assetPath.startsWith('http') ? assetPath : `${this.baseUrl}${assetPath}`;
      console.log(`🔍 Testing ${assetType}: ${assetPath}`);
      
      const response = await fetch(url);
      const contentLength = response.headers.get('content-length');
      const size = contentLength ? parseInt(contentLength, 10) : undefined;
      
      const success = response.status === 200;
      
      if (success) {
        console.log(`✅ ${assetPath} - OK (${size ? `${size} bytes` : 'size unknown'})`);
      } else {
        console.error(`❌ ${assetPath} - HTTP ${response.status}`);
      }
      
      return {
        widget: widgetName,
        assetType,
        assetPath,
        status: response.status,
        success,
        size,
        error: success ? undefined : `HTTP ${response.status}`
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ ${assetPath} - ${errorMessage}`);
      
      return {
        widget: widgetName,
        assetType,
        assetPath,
        status: 0,
        success: false,
        error: errorMessage
      };
    }
  }
  
  /**
   * Test multiple widget classes
   */
  async testMultipleWidgets(widgetClasses: (typeof BaseWidget)[]): Promise<{
    results: AssetTestResult[];
    summary: {
      totalWidgets: number;
      successfulWidgets: number;
      totalAssets: number;
      successfulAssets: number;
      successRate: number;
    };
  }> {
    console.log(`🧪 Testing assets for ${widgetClasses.length} widgets...`);
    
    const allResults: AssetTestResult[] = [];
    
    for (const WidgetClass of widgetClasses) {
      try {
        const results = await this.testWidgetAssets(WidgetClass);
        allResults.push(...results);
      } catch (error) {
        console.error(`💥 Failed to test ${WidgetClass.name}:`, error);
        allResults.push({
          widget: WidgetClass.name,
          assetType: 'js',
          assetPath: 'unknown',
          status: 0,
          success: false,
          error: `Widget test failed: ${error}`
        });
      }
    }
    
    // Calculate summary
    const totalWidgets = widgetClasses.length;
    const widgetResults = new Map<string, boolean>();
    
    allResults.forEach(result => {
      const currentSuccess = widgetResults.get(result.widget) ?? true;
      widgetResults.set(result.widget, currentSuccess && result.success);
    });
    
    const successfulWidgets = Array.from(widgetResults.values()).filter(Boolean).length;
    const totalAssets = allResults.length;
    const successfulAssets = allResults.filter(r => r.success).length;
    const successRate = totalAssets > 0 ? (successfulAssets / totalAssets) * 100 : 0;
    
    console.log('\n📊 Universal Widget Asset Test Summary:');
    console.log(`   Widgets: ${successfulWidgets}/${totalWidgets} passed`);
    console.log(`   Assets: ${successfulAssets}/${totalAssets} loaded successfully`);
    console.log(`   Success Rate: ${successRate.toFixed(1)}%`);
    
    return {
      results: allResults,
      summary: {
        totalWidgets,
        successfulWidgets,
        totalAssets,
        successfulAssets,
        successRate
      }
    };
  }
  
  /**
   * Generate asset test report
   */
  generateReport(results: AssetTestResult[]): string {
    const report = [`# Widget Asset Test Report\n`];
    
    const byWidget = new Map<string, AssetTestResult[]>();
    results.forEach(result => {
      if (!byWidget.has(result.widget)) {
        byWidget.set(result.widget, []);
      }
      byWidget.get(result.widget)!.push(result);
    });
    
    byWidget.forEach((widgetResults, widgetName) => {
      const failed = widgetResults.filter(r => !r.success);
      const status = failed.length === 0 ? '✅' : '❌';
      
      report.push(`## ${status} ${widgetName}`);
      report.push(`Assets: ${widgetResults.length - failed.length}/${widgetResults.length} successful\n`);
      
      if (failed.length > 0) {
        report.push('### Failed Assets:');
        failed.forEach(asset => {
          report.push(`- ${asset.assetPath}: ${asset.error}`);
        });
        report.push('');
      }
    });
    
    return report.join('\n');
  }
}

/**
 * Jest-compatible test function
 */
export async function expectWidgetAssetsToLoad(WidgetClass: typeof BaseWidget): Promise<void> {
  const tester = new UniversalWidgetAssetTester();
  const results = await tester.testWidgetAssets(WidgetClass);
  
  const failedAssets = results.filter(r => !r.success);
  
  if (failedAssets.length > 0) {
    const errorMsg = `${WidgetClass.name} has ${failedAssets.length} failed assets:\n` +
      failedAssets.map(a => `  - ${a.assetPath}: ${a.error}`).join('\n');
    throw new Error(errorMsg);
  }
}