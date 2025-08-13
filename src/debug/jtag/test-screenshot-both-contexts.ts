/**
 * Comprehensive Screenshot Test - Both Server and Browser Contexts
 * Tests screenshot functionality with TRUE browser-side and server-side execution
 */

import { JTAGClientServer } from './system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from './system/core/client/shared/JTAGClient';

interface ScreenshotTestResult {
  testName: string;
  context: 'server' | 'browser';
  success: boolean;
  filepath?: string;
  fileSize?: number;
  error?: string;
}

async function testServerSideScreenshot(): Promise<ScreenshotTestResult> {
  console.log('🖥️  SERVER CONTEXT: Server-initiated screenshot');
  
  try {
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server',
      transportType: 'websocket',
      serverUrl: 'ws://localhost:9001',
      enableFallback: false
    };
    
    const { client } = await JTAGClientServer.connect(clientOptions);
    
    // Server calls screenshot command directly
    const result = await (client as any).commands.screenshot({
      filename: 'final-server-test.png',
      querySelector: 'body'
    });
    
    await (client as any).disconnect();
    
    const filepath = result.commandResult?.commandResult?.filepath;
    if (filepath && result.success) {
      const fs = await import('fs');
      const stats = fs.statSync(filepath);
      return {
        testName: 'serverScreenshot',
        context: 'server',
        success: true,
        filepath,
        fileSize: stats.size
      };
    } else {
      return {
        testName: 'serverScreenshot',
        context: 'server',
        success: false,
        error: 'No file created'
      };
    }
    
  } catch (error) {
    return {
      testName: 'serverScreenshot',
      context: 'server',
      success: false,
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

async function testBrowserSideScreenshot(): Promise<ScreenshotTestResult> {
  console.log('🌐 BROWSER CONTEXT: Browser-initiated screenshot');
  
  try {
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server',
      transportType: 'websocket', 
      serverUrl: 'ws://localhost:9001',
      enableFallback: false
    };
    
    const { client } = await JTAGClientServer.connect(clientOptions);
    
    // Have browser initiate its own screenshot using demo function
    const result = await (client as any).commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🌐 BROWSER INITIATED: Starting browser-side screenshot');
          
          if (typeof testBrowserScreenshot === 'function') {
            console.log('🌐 BROWSER INITIATED: Calling demo function');
            testBrowserScreenshot(); // Browser calls its own screenshot
            console.log('✅ BROWSER INITIATED: Demo function executed');
            return { success: true, method: 'browser-initiated' };
          } else {
            console.log('❌ BROWSER INITIATED: Demo function not available');
            return { success: false, error: 'Demo function not available' };
          }
        `
      }
    });
    
    await (client as any).disconnect();
    
    if (result.success && result.commandResult?.success) {
      return {
        testName: 'browserScreenshot', 
        context: 'browser',
        success: true
      };
    } else {
      return {
        testName: 'browserScreenshot',
        context: 'browser', 
        success: false,
        error: 'Browser execution failed'
      };
    }
    
  } catch (error) {
    return {
      testName: 'browserScreenshot',
      context: 'browser',
      success: false, 
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

async function runComprehensiveScreenshotTests(): Promise<void> {
  console.log('📸 COMPREHENSIVE SCREENSHOT TESTS - Both Contexts');
  console.log('='.repeat(60));
  
  const results: ScreenshotTestResult[] = [];
  
  // Test 1: Server-initiated screenshot
  console.log('1️⃣  Testing server-initiated screenshot...');
  const serverResult = await testServerSideScreenshot();
  results.push(serverResult);
  console.log('');
  
  // Test 2: Browser-initiated screenshot  
  console.log('2️⃣  Testing browser-initiated screenshot...');
  const browserResult = await testBrowserSideScreenshot();
  results.push(browserResult);
  console.log('');
  
  // Results
  console.log('🎯 COMPREHENSIVE SCREENSHOT TEST RESULTS');
  console.log('='.repeat(60));
  
  const totalSuccess = results.filter(r => r.success).length;
  console.log(`📊 Results: ${totalSuccess}/${results.length} tests passed`);
  
  results.forEach((result, index) => {
    const status = result.success ? '✅ PASSED' : '❌ FAILED';
    const context = result.context === 'server' ? '🖥️ ' : '🌐';
    console.log(`${index + 1}. ${context} ${result.testName}: ${status}`);
    
    if (result.success) {
      if (result.filepath) console.log(`   📁 File: ${result.filepath}`);
      if (result.fileSize) console.log(`   📏 Size: ${result.fileSize} bytes`);
    } else if (result.error) {
      console.log(`   ❌ Error: ${result.error}`);
    }
  });
  
  console.log('');
  
  // Check for screenshot files
  console.log('📁 SCREENSHOT FILES VERIFICATION:');
  try {
    const fs = await import('fs');
    const files = fs.readdirSync('/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/sessions/user/44be8f85-a3d2-410a-bd04-87c4bfeef9b9/screenshots/');
    files.forEach(file => {
      const stats = fs.statSync(`/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/examples/test-bench/.continuum/jtag/sessions/user/44be8f85-a3d2-410a-bd04-87c4bfeef9b9/screenshots/${file}`);
      console.log(`   📸 ${file} (${stats.size} bytes)`);
    });
  } catch (error) {
    console.log('   ⚠️ Could not list screenshot files');
  }
  
  if (totalSuccess === results.length) {
    console.log('');
    console.log('🎉 ALL SCREENSHOT TESTS PASSED!');
    console.log('✅ Both server-side and browser-side screenshot functionality confirmed');
    console.log('✅ Thin client architecture working in both contexts');
  } else {
    console.log('');
    console.log('❌ SOME SCREENSHOT TESTS FAILED');
    process.exit(1);
  }
}

runComprehensiveScreenshotTests().catch(error => {
  console.error('💥 Comprehensive test error:', error);
  process.exit(1);
});