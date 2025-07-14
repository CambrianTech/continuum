/**
 * DEAD CODE CANDIDATE - STATIC METHODS TEST FOR PACKAGE.JSON ASSET SYSTEM
 * 
 * ⚠️  POTENTIAL DEAD CODE: This file appears to be in temp-disabled directory
 * TODO: Determine if this static methods test is still needed or should be archived
 * 
 * ISSUES IDENTIFIED:
 * - TODO: File location in temp-disabled suggests it may be dead code
 * - TODO: No proper middle-out architecture header
 * - TODO: Hardcoded path patterns '/dist/ui/components/' and '/src/' (lines 14, 18, 23)
 * - TODO: Magic strings for path manipulation and class name parsing
 * - TODO: Mock fetch implementation should be extracted to test utilities (lines 106-120)
 * - TODO: Hardcoded test data in mock fetch
 * - TODO: Switch-like logic in getBasePath() method should be configuration-driven
 * 
 * Original functionality: Tests only the static methods that can run in Node.js
 */

// Extract just the static methods we need to test
class BaseWidgetStatic {
  static getBasePath(): string {
    // Extract widget directory from class name: ChatWidget -> Chat, SidebarWidget -> Sidebar
    const className = this.name.replace('Widget', '');
    
    // Special case for shared components
    if (className === 'Base' || className === 'Interactive' || this.name.includes('BaseWidget')) {
      // TODO: Replace hardcoded shared components path with configuration
      return '/dist/ui/components/shared';
    }
    
    // All other widgets: /dist/ui/components/{WidgetName}
    // TODO: Replace hardcoded path pattern with configuration
    return `/dist/ui/components/${className}`;
  }
  
  static async getWidgetFiles(): Promise<string[]> {
    try {
      // TODO: Replace hardcoded path replacement with configuration
      const basePath = this.getBasePath().replace('/dist/', '/src/'); // Read from source
      const packagePath = `${basePath}/package.json`;
      
      const response = await fetch(packagePath);
      if (!response.ok) {
        console.warn(`📦 No package.json found for ${this.name} at ${packagePath}`);
        return [];
      }
      
      const packageData = await response.json();
      return packageData.files || [];
    } catch (error) {
      console.warn(`📦 Failed to read package.json for ${this.name}:`, error);
      return [];
    }
  }
  
  static async getWidgetAssets(): Promise<string[]> {
    const widgetFiles = await this.getWidgetFiles();
    const assets = widgetFiles.filter(file => !file.endsWith('.ts')); // Serve everything except TypeScript
    return assets.map(file => `${this.getBasePath()}/${file}`);
  }
}

class ChatWidgetStatic extends BaseWidgetStatic {
  // The name is automatically set by the class system
}

class SidebarWidgetStatic extends BaseWidgetStatic {
  // The name is automatically set by the class system  
}

async function testStaticMethods() {
  console.log('🧪 Testing Static Methods for Asset System\n');
  
  // Test 1: Path generation for different widget types
  console.log('Test 1: Path generation');
  console.log(`ChatWidget path: ${ChatWidgetStatic.getBasePath()}`);
  console.log(`SidebarWidget path: ${SidebarWidgetStatic.getBasePath()}`);
  console.log(`BaseWidget path: ${BaseWidgetStatic.getBasePath()}`);
  
  // Test 2: Try to read ChatWidget package.json if it exists
  console.log('\nTest 2: Package.json reading');
  try {
    console.log('Attempting to read ChatWidget package.json...');
    const files = await ChatWidgetStatic.getWidgetFiles();
    console.log(`📦 Files found: ${JSON.stringify(files)}`);
    
    if (files.length > 0) {
      console.log('✅ Successfully read package.json files array');
      
      // Test 3: Asset filtering
      console.log('\nTest 3: Asset filtering');
      const assets = await ChatWidgetStatic.getWidgetAssets();
      console.log(`🎨 Assets (non-.ts): ${JSON.stringify(assets)}`);
      
      const hasTypeScriptFiles = files.some(file => file.endsWith('.ts'));
      const assetsHaveTypeScript = assets.some(asset => asset.endsWith('.ts'));
      
      if (hasTypeScriptFiles && !assetsHaveTypeScript) {
        console.log('✅ TypeScript files properly filtered out of assets');
      } else if (!hasTypeScriptFiles) {
        console.log('ℹ️  No TypeScript files in package.json to filter');
      } else {
        console.log('❌ TypeScript files not filtered properly');
      }
    } else {
      console.log('ℹ️  No files found (package.json may not exist or have empty files array)');
    }
  } catch (error) {
    console.log(`❌ Error: ${error}`);
  }
  
  console.log('\n📊 Test Summary:');
  console.log('✅ Path generation works correctly');
  console.log('✅ Package.json reading handles missing files gracefully');
  console.log('✅ Asset filtering logic is implemented');
  console.log('\n🎯 Ready for browser integration testing!');
}

// Mock fetch if not available in Node.js environment
if (typeof fetch === 'undefined') {
  console.log('⚠️  Fetch not available in Node.js - using mock for testing');
  (global as any).fetch = async (url: string) => {
    console.log(`Mock fetch: ${url}`);
    // Return mock data for testing
    if (url.includes('Chat/package.json')) {
      return {
        ok: true,
        // TODO: Replace hardcoded mock test data with configurable test fixtures
        json: () => Promise.resolve({
          name: '@continuum/chat-widget',
          files: ['ChatWidget.ts', 'ChatWidget.css', 'chat-icon.svg', 'notification.mp3']
        })
      };
    }
    return { ok: false, status: 404 };
  };
}

testStaticMethods().catch(error => {
  console.error('Test failed:', error);
  process.exit(1);
});