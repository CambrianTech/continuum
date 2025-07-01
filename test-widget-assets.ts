/**
 * Simple Widget Asset Test - No Jest Dependencies
 * Test widget asset loading against running server
 */

import { SidebarWidget } from './src/ui/components/Sidebar/SidebarWidget.js';
import { ChatWidget } from './src/ui/components/Chat/ChatWidget.js';

async function testWidgetAssets(WidgetClass: any) {
  console.log(`\n🧪 Testing assets for ${WidgetClass.name}...`);
  
  // Create widget instance
  const widget = new WidgetClass();
  
  // Capture all fetch calls made by getBundledCSS
  const fetchedUrls: string[] = [];
  const originalFetch = global.fetch;
  
  global.fetch = ((url: string) => {
    fetchedUrls.push(url);
    // Return mock response so getBundledCSS doesn't fail
    return Promise.resolve(new Response('/* mock css */'));
  }) as any;
  
  try {
    // Call getBundledCSS to discover what assets it needs
    await widget.getBundledCSS();
    
    // Restore original fetch
    global.fetch = originalFetch;
    
    console.log(`📋 ${WidgetClass.name} requests ${fetchedUrls.length} assets:`, fetchedUrls);
    
    // Test each discovered asset URL against actual server
    for (const url of fetchedUrls) {
      try {
        console.log(`🔍 Testing asset: ${url}`);
        const response = await fetch(url);
        
        if (response.status === 200) {
          console.log(`✅ ${url} - OK`);
        } else {
          console.error(`❌ ${url} - HTTP ${response.status}`);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.error(`❌ ${url} - ${errorMessage}`);
      }
    }
    
  } catch (error) {
    // Restore fetch in case of error
    global.fetch = originalFetch;
    console.error(`💥 ${WidgetClass.name} getBundledCSS failed:`, error);
  }
}

async function main() {
  console.log('🌐 Testing widget assets against http://localhost:9000');
  
  // Test all widgets
  const widgets = [SidebarWidget, ChatWidget];
  
  for (const widget of widgets) {
    await testWidgetAssets(widget);
  }
  
  console.log('\n✅ Widget asset testing complete');
}

main().catch(console.error);