/**
 * Layer 1 Foundation Test: Browser Bootstrap
 * 
 * Tests that the browser can connect and establish WebSocket communication
 * in Layer 1 - not waiting until Layer 6 like before.
 */

import puppeteer from 'puppeteer';

/**
 * Browser Bootstrap Tests - Early browser integration
 */
async function runBrowserBootstrapTests(): Promise<void> {
  console.log('🌐 Layer 1: Browser Bootstrap Tests (Early Integration)');
  
  let browser: any = null;
  let page: any = null;
  let testCount = 0;
  let passCount = 0;
  
  try {
    // Test 1: Browser launches successfully
    testCount++;
    try {
      browser = await puppeteer.launch({ 
        headless: false, 
        devtools: true,
        args: ['--no-sandbox', '--disable-setuid-sandbox']
      });
      
      page = await browser.newPage();
      console.log('✅ Test 1: Browser launches successfully');
      passCount++;
    } catch (error: any) {
      console.log('❌ Test 1: Browser launch failed -', error.message);
    }
    
    // Test 2: Navigate to JTAG demo page
    testCount++;
    try {
      await page.goto('http://localhost:9002', { 
        waitUntil: 'networkidle2',
        timeout: 10000 
      });
      
      const title = await page.title();
      if (title.includes('JTAG') || title.includes('Demo')) {
        console.log('✅ Test 2: Navigate to JTAG demo page');
        passCount++;
      } else {
        throw new Error(`Unexpected page title: ${title}`);
      }
    } catch (error: any) {
      console.log('❌ Test 2: Demo page navigation failed -', error.message);
    }
    
    // Test 3: WebSocket connection availability
    testCount++;
    try {
      const websocketSupport = await page.evaluate(() => {
        return typeof WebSocket !== 'undefined';
      });
      
      if (websocketSupport) {
        console.log('✅ Test 3: WebSocket connection availability');
        passCount++;
      } else {
        throw new Error('WebSocket not available in browser context');
      }
    } catch (error: any) {
      console.log('❌ Test 3: WebSocket availability check failed -', error.message);
    }
    
    // Test 4: JTAG WebSocket server connectivity
    testCount++;
    try {
      const connectionTest = await page.evaluate(() => {
        return new Promise((resolve) => {
          const ws = new WebSocket('ws://localhost:9001');
          
          const timeout = setTimeout(() => {
            ws.close();
            resolve({ success: false, error: 'Connection timeout' });
          }, 5000);
          
          ws.onopen = () => {
            clearTimeout(timeout);
            ws.close();
            resolve({ success: true });
          };
          
          ws.onerror = (error) => {
            clearTimeout(timeout);
            resolve({ success: false, error: 'WebSocket connection failed' });
          };
        });
      });
      
      if ((connectionTest as any).success) {
        console.log('✅ Test 4: JTAG WebSocket server connectivity');
        passCount++;
      } else {
        throw new Error((connectionTest as any).error || 'WebSocket connection failed');
      }
    } catch (error: any) {
      console.log('❌ Test 4: WebSocket server connectivity failed -', error.message);
    }
    
    // Test 5: Console object available for interception
    testCount++;
    try {
      const consoleAvailable = await page.evaluate(() => {
        return typeof console !== 'undefined' && 
               typeof console.log === 'function' &&
               typeof console.error === 'function' &&
               typeof console.warn === 'function';
      });
      
      if (consoleAvailable) {
        console.log('✅ Test 5: Console object available for interception');
        passCount++;
      } else {
        throw new Error('Console object not properly available');
      }
    } catch (error: any) {
      console.log('❌ Test 5: Console availability check failed -', error.message);
    }
    
    // Test 6: Basic message sending capability
    testCount++;
    try {
      const messagingTest = await page.evaluate(() => {
        return new Promise((resolve) => {
          const ws = new WebSocket('ws://localhost:9001');
          
          const timeout = setTimeout(() => {
            ws.close();
            resolve({ success: false, error: 'Message timeout' });
          }, 5000);
          
          ws.onopen = () => {
            const testMessage = {
              type: 'ping',
              payload: { timestamp: new Date().toISOString(), context: 'client' },
              timestamp: new Date().toISOString()
            };
            
            ws.send(JSON.stringify(testMessage));
          };
          
          ws.onmessage = (event) => {
            clearTimeout(timeout);
            ws.close();
            resolve({ success: true, response: event.data });
          };
          
          ws.onerror = () => {
            clearTimeout(timeout);
            resolve({ success: false, error: 'Message send failed' });
          };
        });
      });
      
      if ((messagingTest as any).success) {
        console.log('✅ Test 6: Basic message sending capability');
        passCount++;
      } else {
        throw new Error((messagingTest as any).error || 'Message sending failed');
      }
    } catch (error: any) {
      console.log('❌ Test 6: Message sending test failed -', error.message);
    }
    
  } finally {
    // Cleanup
    if (page) {
      await page.close();
    }
    if (browser) {
      await browser.close();
    }
  }
  
  // Test Results
  console.log('');
  console.log(`📊 Layer 1 Browser Bootstrap Tests: ${passCount}/${testCount} passed`);
  
  if (passCount === testCount) {
    console.log('🎯 Layer 1 Browser Bootstrap: ALL TESTS PASSED - Browser ready for integration');
    process.exit(0);
  } else {
    console.log('❌ Layer 1 Browser Bootstrap: TESTS FAILED - Fix browser connectivity');
    process.exit(1);
  }
}

// Run the tests
runBrowserBootstrapTests().catch(error => {
  console.error('💥 Browser bootstrap test runner error:', error);
  process.exit(1);
});