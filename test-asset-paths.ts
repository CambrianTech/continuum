/**
 * Simple Asset Path Test  
 * Test the asset URLs that widgets declare in their arrays
 */

// Mock HTMLElement for Node.js environment
global.HTMLElement = class HTMLElement {} as any;
global.ShadowRoot = class ShadowRoot {} as any;

import { SidebarWidget } from './src/ui/components/Sidebar/SidebarWidget.js';
import { ChatWidget } from './src/ui/components/Chat/ChatWidget.js';

async function testAssetPaths() {
  console.log('🌐 Testing widget asset paths against http://localhost:9000');
  
  // Get asset paths from widget declarations
  const widgets = [SidebarWidget, ChatWidget];
  const assetPaths: string[] = [];
  
  for (const WidgetClass of widgets) {
    const basePath = WidgetClass.getBasePath();
    const cssAssets = WidgetClass.getCSSAssets();
    
    console.log(`📋 ${WidgetClass.name} declares ${cssAssets.length} CSS assets from ${basePath}:`);
    
    for (const asset of cssAssets) {
      const fullPath = `${basePath}/${asset}`;
      console.log(`   ${fullPath}`);
      assetPaths.push(fullPath);
    }
  }
  
  let passed = 0;
  let failed = 0;
  
  for (const path of assetPaths) {
    try {
      console.log(`🔍 Testing: ${path}`);
      const response = await fetch(`http://localhost:9000${path}`);
      
      if (response.status === 200) {
        console.log(`✅ ${path} - OK`);
        passed++;
      } else {
        console.error(`❌ ${path} - HTTP ${response.status}`);
        failed++;
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ ${path} - ${errorMessage}`);
      failed++;
    }
  }
  
  console.log(`\n📊 Results: ${passed} passed, ${failed} failed`);
  
  if (failed > 0) {
    console.log('\n🔧 Failed assets need to be fixed for widgets to load properly');
    process.exit(1);
  } else {
    console.log('\n✅ All widget assets are accessible!');
  }
}

testAssetPaths().catch(console.error);