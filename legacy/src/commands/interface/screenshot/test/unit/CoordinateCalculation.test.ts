/**
 * Unit Tests for Coordinate Calculation Functions
 * Testing small, modular functions in isolation
 */

import {
  getPageScrollOffset,
  getViewportCoordinates,
  viewportToDocumentCoords,
  applyCoordinateScaling,
  getAbsolutePosition,
  type ViewportCoordinates,
  type ScrollOffset,
  type DocumentCoordinates
} from '../../shared/browser-utils/BrowserElementUtils';

console.log('🧪 Coordinate Calculation Unit Tests');

function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

function assertDeepEqual(actual: any, expected: any, message: string) {
  const actualStr = JSON.stringify(actual);
  const expectedStr = JSON.stringify(expected);
  if (actualStr !== expectedStr) {
    throw new Error(`❌ Assertion failed: ${message}\n  Expected: ${expectedStr}\n  Actual: ${actualStr}`);
  }
  console.log(`✅ ${message}`);
}

/**
 * Test viewportToDocumentCoords - Pure function
 */
function testViewportToDocumentCoords() {
  console.log('\n⚡ Testing viewportToDocumentCoords');
  
  // Test case 1: No scroll
  const result1 = viewportToDocumentCoords(
    { left: 100, top: 200 },
    { x: 0, y: 0 }
  );
  assertDeepEqual(result1, { x: 100, y: 200 }, 'No scroll - coordinates unchanged');
  
  // Test case 2: With scroll
  const result2 = viewportToDocumentCoords(
    { left: 100, top: 200 },
    { x: 50, y: 75 }
  );
  assertDeepEqual(result2, { x: 150, y: 275 }, 'With scroll - coordinates adjusted correctly');
  
  // Test case 3: Negative viewport (element off-screen left/top)
  const result3 = viewportToDocumentCoords(
    { left: -50, top: -25 },
    { x: 100, y: 200 }
  );
  assertDeepEqual(result3, { x: 50, y: 175 }, 'Negative viewport coords handled correctly');
  
  // Test case 4: Large scroll values
  const result4 = viewportToDocumentCoords(
    { left: 10, top: 20 },
    { x: 1000, y: 2000 }
  );
  assertDeepEqual(result4, { x: 1010, y: 2020 }, 'Large scroll values handled correctly');
}

/**
 * Test applyCoordinateScaling - Pure function
 */
function testApplyCoordinateScaling() {
  console.log('\n⚡ Testing applyCoordinateScaling');
  
  // Test case 1: No scaling (scale = 1)
  const result1 = applyCoordinateScaling(
    { x: 100, y: 200, width: 300, height: 400 },
    1
  );
  assertDeepEqual(result1, { x: 100, y: 200, width: 300, height: 400 }, 'Scale 1.0 - no change');
  
  // Test case 2: 2x scaling
  const result2 = applyCoordinateScaling(
    { x: 100, y: 200, width: 300, height: 400 },
    2
  );
  assertDeepEqual(result2, { x: 200, y: 400, width: 600, height: 800 }, 'Scale 2.0 - doubled');
  
  // Test case 3: 0.5x scaling (half size)
  const result3 = applyCoordinateScaling(
    { x: 100, y: 200, width: 300, height: 400 },
    0.5
  );
  assertDeepEqual(result3, { x: 50, y: 100, width: 150, height: 200 }, 'Scale 0.5 - halved');
  
  // Test case 4: Fractional coordinates (should round)
  const result4 = applyCoordinateScaling(
    { x: 33, y: 66, width: 99, height: 132 },
    0.3333
  );
  // 33 * 0.3333 = 10.9989 -> rounds to 11
  // 66 * 0.3333 = 21.9978 -> rounds to 22
  // 99 * 0.3333 = 32.9967 -> rounds to 33
  // 132 * 0.3333 = 43.9956 -> rounds to 44
  assertDeepEqual(result4, { x: 11, y: 22, width: 33, height: 44 }, 'Fractional values rounded correctly');
}

/**
 * Test coordinate calculation edge cases
 */
function testEdgeCases() {
  console.log('\n⚡ Testing edge cases');
  
  // Test case 1: Zero coordinates
  const result1 = viewportToDocumentCoords(
    { left: 0, top: 0 },
    { x: 0, y: 0 }
  );
  assertDeepEqual(result1, { x: 0, y: 0 }, 'Zero coordinates handled');
  
  // Test case 2: Zero scaling
  const result2 = applyCoordinateScaling(
    { x: 100, y: 200, width: 300, height: 400 },
    0
  );
  assertDeepEqual(result2, { x: 0, y: 0, width: 0, height: 0 }, 'Zero scaling handled');
  
  // Test case 3: Very large values
  const result3 = applyCoordinateScaling(
    { x: 999999, y: 888888, width: 777777, height: 666666 },
    1.5
  );
  assertDeepEqual(result3, { 
    x: Math.round(999999 * 1.5), 
    y: Math.round(888888 * 1.5), 
    width: Math.round(777777 * 1.5), 
    height: Math.round(666666 * 1.5) 
  }, 'Large values handled correctly');
}

/**
 * Test realistic screenshot scenarios
 */
function testRealisticScenarios() {
  console.log('\n⚡ Testing realistic screenshot scenarios');
  
  // Scenario 1: Chat widget in viewport, no scroll
  const chatWidgetViewport = { left: 100, top: 150, width: 400, height: 200 };
  const noScroll = { x: 0, y: 0 };
  const chatResult = viewportToDocumentCoords(chatWidgetViewport, noScroll);
  assertDeepEqual(chatResult, { x: 100, y: 150 }, 'Chat widget - no scroll scenario');
  
  // Scenario 2: Element after scrolling down
  const scrolledElementViewport = { left: 50, top: 100, width: 300, height: 150 };
  const scrolledDown = { x: 0, y: 500 };
  const scrolledResult = viewportToDocumentCoords(scrolledElementViewport, scrolledDown);
  assertDeepEqual(scrolledResult, { x: 50, y: 600 }, 'Scrolled element scenario');
  
  // Scenario 3: High-DPI scaling (2x)
  const highDPICoords = { x: 200, y: 300, width: 600, height: 400 };
  const scaledResult = applyCoordinateScaling(highDPICoords, 2);
  assertDeepEqual(scaledResult, { x: 400, y: 600, width: 1200, height: 800 }, 'High-DPI scaling scenario');
  
  // Scenario 4: Element partially off-screen (negative viewport coords)
  const offScreenViewport = { left: -50, top: -25 };
  const normalScroll = { x: 100, y: 200 };
  const offScreenResult = viewportToDocumentCoords(offScreenViewport, normalScroll);
  assertDeepEqual(offScreenResult, { x: 50, y: 175 }, 'Partially off-screen element scenario');
}

/**
 * Run all coordinate calculation unit tests
 */
async function runCoordinateTests(): Promise<void> {
  console.log('🚀 Starting Coordinate Calculation Unit Tests\n');
  
  try {
    testViewportToDocumentCoords();
    testApplyCoordinateScaling(); 
    testEdgeCases();
    testRealisticScenarios();
    
    console.log('\n🎉 ALL COORDINATE CALCULATION TESTS PASSED!');
    console.log('📋 Validated:');
    console.log('  ✅ Viewport to document coordinate conversion');
    console.log('  ✅ Coordinate scaling with proper rounding');
    console.log('  ✅ Edge cases (zero values, large values)');
    console.log('  ✅ Realistic screenshot scenarios');
    console.log('  ✅ Mathematical correctness of transformations');
    
  } catch (error) {
    console.error('\n❌ Coordinate calculation tests failed:', error.message);
    throw error;
  }
}

// Export for use in other tests
export { runCoordinateTests };

// Run if called directly
if (require.main === module) {
  runCoordinateTests().catch(() => process.exit(1));
}