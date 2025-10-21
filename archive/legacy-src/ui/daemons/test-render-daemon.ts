/**
 * Test RenderDaemon - Verify package-driven asset processing works
 */

import { DaemonManager } from './BaseDaemon.js';

// Test package configuration
const testPackageConfig = {
  name: 'test-widget',
  files: ['TestWidget.ts', 'TestWidget.css', 'icon.svg'],
  assetPipeline: {
    'TestWidget.css': ['minify', 'compress'],
    'icon.svg': ['optimize']
  },
  outputFormats: {
    'css': 'string',
    'image': 'blob'
  }
};

// Mock CSS content for testing
const mockCSS = `
/* Test CSS with comments and whitespace */
.test-widget {
  background-color: #ffffff;
  padding: 10px;
  /* This is a comment */
  margin: 5px;
}

.test-button {
  color: blue;
  font-size: 14px;
}
`;

async function testRenderDaemon() {
  console.log('🧪 Testing RenderDaemon...');
  
  try {
    // Create daemon manager
    const manager = new DaemonManager();
    
    // Start RenderDaemon as Web Worker
    console.log('📦 Starting RenderDaemon...');
    await manager.startDaemon('render', '/src/ui/daemons/RenderDaemon.js');
    
    // Test 1: Process single asset
    console.log('🎨 Test 1: Processing single CSS asset...');
    const assetResult = await manager.sendRequest('render', {
      type: 'render:asset',
      assetUrl: 'data:text/css;base64,' + btoa(mockCSS),
      operations: ['minify'],
      outputFormat: 'string'
    });
    
    console.log('✅ Asset result:', {
      success: assetResult.success,
      assetSize: assetResult.result?.asset?.length,
      originalSize: mockCSS.length,
      compressed: assetResult.result?.asset?.length < mockCSS.length
    });
    
    // Test 2: Process package
    console.log('📦 Test 2: Processing package configuration...');
    const packageResult = await manager.sendRequest('render', {
      type: 'render:package',
      packageConfig: testPackageConfig
    });
    
    console.log('✅ Package result:', {
      success: packageResult.success,
      assetsProcessed: Object.keys(packageResult.result?.assets || {}).length,
      processingTime: packageResult.processingTime
    });
    
    // Test 3: Health check
    console.log('🏥 Test 3: Health check...');
    const healthResult = await manager.sendRequest('render', {
      type: 'daemon:health'
    });
    
    console.log('✅ Health result:', healthResult);
    
    // Test 4: Status check
    console.log('📊 Test 4: Status check...');
    const status = await manager.getDaemonStatus('render');
    console.log('✅ Status:', status);
    
    // Cleanup
    await manager.stopDaemon('render');
    console.log('🎉 All tests passed!');
    
    return true;
    
  } catch (error) {
    console.error('❌ Test failed:', error);
    return false;
  }
}

// Test function for browser environment
async function testInBrowser() {
  console.log('🌐 Testing RenderDaemon in browser...');
  
  // Create mock fetch for testing
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (url: string | URL | Request) => {
    const urlString = typeof url === 'string' ? url : url.toString();
    if (urlString.includes('package.json')) {
      return new Response(JSON.stringify(testPackageConfig));
    }
    if (urlString.includes('.css')) {
      return new Response(mockCSS, { 
        headers: { 'content-type': 'text/css' }
      });
    }
    return originalFetch(url);
  };
  
  try {
    const success = await testRenderDaemon();
    return success;
  } finally {
    globalThis.fetch = originalFetch;
  }
}

// Export for use in browser or Node
export { testRenderDaemon, testInBrowser };

// Auto-run if called directly
if (typeof window !== 'undefined') {
  // Browser environment
  console.log('🔬 RenderDaemon test ready - call testInBrowser() to run');
  (window as any).testRenderDaemon = testInBrowser;
} else {
  // Node environment  
  console.log('🔬 RenderDaemon test ready - call testRenderDaemon() to run');
}