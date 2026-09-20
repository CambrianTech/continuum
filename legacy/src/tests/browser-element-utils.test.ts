/**
 * Browser Element Utilities - Integration Test
 * Tests coordinate calculation accuracy in real browser environment
 */

import { JTAGClient } from '../system/core/client/JTAGClient';

interface TestResult {
  testName: string;
  passed: boolean;
  error?: string;
  elementBounds?: any;
  calculatedCoords?: any;
  actualScreenshot?: any;
}

export async function runBrowserElementUtilsTests(): Promise<TestResult[]> {
  const results: TestResult[] = [];
  
  console.log('🧪 BROWSER ELEMENT UTILS INTEGRATION TEST');
  console.log('===========================================');
  
  try {
    // Connect to JTAG system
    const jtag = new JTAGClient();
    await jtag.connect();
    
    // Test 1: Chat widget coordinate calculation accuracy
    try {
      console.log('🧪 Test 1: Chat widget coordinate calculation...');
      
      // Get the chat widget element directly from DOM
      const chatWidget = document.querySelector('chat-widget') || document.querySelector('.chat-widget');
      if (!chatWidget) {
        throw new Error('Chat widget not found in DOM');
      }
      
      // Load our utilities (they're now built into the browser bundle)
      const utils = (window as any).BrowserElementUtils;
      if (!utils) {
        throw new Error('BrowserElementUtils not available in browser');
      }
      
      // Test coordinate calculation
      const bounds = utils.getElementBounds(chatWidget, true);
      const coords = utils.calculateCropCoordinates(chatWidget, document.body, 1, true);
      
      console.log('📐 Element bounds:', bounds);
      console.log('📐 Calculated coordinates:', coords);
      
      // Take screenshot to validate coordinates
      const screenshot = await jtag.commands.screenshot({
        querySelector: 'chat-widget',
        filename: 'coordinate-validation-test.png',
        destination: 'file'
      });
      
      results.push({
        testName: 'chatWidgetCoordinates',
        passed: screenshot.success && coords.width > 0 && coords.height > 0,
        elementBounds: bounds,
        calculatedCoords: coords,
        actualScreenshot: screenshot
      });
      
    } catch (error) {
      results.push({
        testName: 'chatWidgetCoordinates',
        passed: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
    
    // Test 2: Overflow content detection
    try {
      console.log('🧪 Test 2: Overflow content detection...');
      
      // Create element with overflow content for testing
      const testDiv = document.createElement('div');
      testDiv.style.cssText = `
        position: absolute;
        top: 100px;
        left: 100px;
        width: 200px;
        height: 100px;
        overflow: hidden;
        border: 1px solid red;
      `;
      testDiv.innerHTML = 'Test content that is much longer than the container width and should trigger overflow detection. This text should definitely overflow the 200px width container.';
      testDiv.id = 'overflow-test-element';
      document.body.appendChild(testDiv);
      
      const utils = (window as any).BrowserElementUtils;
      const boundsWithOverflow = utils.getElementBounds(testDiv, true);
      const boundsWithoutOverflow = utils.getElementBounds(testDiv, false);
      
      console.log('📐 With overflow:', boundsWithOverflow);
      console.log('📐 Without overflow:', boundsWithoutOverflow);
      
      // Clean up test element
      document.body.removeChild(testDiv);
      
      results.push({
        testName: 'overflowDetection',
        passed: boundsWithOverflow.hasOverflow && boundsWithOverflow.width > boundsWithoutOverflow.width,
        elementBounds: { withOverflow: boundsWithOverflow, withoutOverflow: boundsWithoutOverflow }
      });
      
    } catch (error) {
      results.push({
        testName: 'overflowDetection',
        passed: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
    
    // Test 3: Coordinate math validation with known element
    try {
      console.log('🧪 Test 3: Coordinate math validation...');
      
      // Use document.body as reference (known coordinates)
      const utils = (window as any).BrowserElementUtils;
      const bodyCoords = utils.calculateCropCoordinates(document.body, document.body, 1, false);
      
      // Body relative to itself should be (0,0) plus scroll offsets
      const expectedX = window.scrollX;
      const expectedY = window.scrollY;
      
      console.log('📐 Body coordinates:', bodyCoords);
      console.log('📐 Expected coordinates:', { x: expectedX, y: expectedY });
      
      const coordinatesMatch = Math.abs(bodyCoords.x - expectedX) < 5 && Math.abs(bodyCoords.y - expectedY) < 5;
      
      results.push({
        testName: 'coordinateMathValidation',
        passed: coordinatesMatch,
        calculatedCoords: bodyCoords,
        elementBounds: { expected: { x: expectedX, y: expectedY }, calculated: { x: bodyCoords.x, y: bodyCoords.y } }
      });
      
    } catch (error) {
      results.push({
        testName: 'coordinateMathValidation',
        passed: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
    
    await jtag.disconnect();
    
  } catch (error) {
    console.error('❌ Test setup failed:', error);
    results.push({
      testName: 'testSetup',
      passed: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
  
  // Print results
  const passedTests = results.filter(r => r.passed).length;
  const totalTests = results.length;
  
  console.log(`\n🎯 ============= BROWSER ELEMENT UTILS TEST RESULTS =============`);
  console.log(`📊 Tests: ${passedTests}/${totalTests} passed`);
  
  results.forEach(result => {
    const status = result.passed ? '✅' : '❌';
    console.log(`${status} ${result.testName}: ${result.passed ? 'PASSED' : 'FAILED'}`);
    if (result.error) console.log(`   Error: ${result.error}`);
  });
  
  return results;
}

// Run tests if executed directly in browser
if (typeof window !== 'undefined') {
  (window as any).runBrowserElementUtilsTests = runBrowserElementUtilsTests;
}