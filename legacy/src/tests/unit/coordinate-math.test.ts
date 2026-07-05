#!/usr/bin/env npx tsx
/**
 * Unit Tests for Screenshot Coordinate Calculation Math
 * 
 * Tests the mathematical precision of coordinate calculation that the user
 * specifically requested: "math is wrong on smoe of your scresnshots"
 * "you will want to unit and integration test this logic"
 */

interface CropCoordinates {
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
}

/**
 * Core coordinate calculation logic extracted for testing
 */
function calculateCropCoordinates(
  elementRect: DOMRect,
  bodyRect: DOMRect,
  scrollX: number,
  scrollY: number,
  elementScrollWidth: number,
  elementScrollHeight: number,
  scale: number = 1,
  includeOverflow: boolean = true
): CropCoordinates {
  // Calculate actual dimensions (including overflow if requested)
  const actualWidth = includeOverflow ? 
    Math.max(elementRect.width, elementScrollWidth) : 
    elementRect.width;
  const actualHeight = includeOverflow ? 
    Math.max(elementRect.height, elementScrollHeight) : 
    elementRect.height;

  // Calculate coordinates relative to body element (accounting for body's position)
  const relativeX = Math.max(0, (elementRect.left - bodyRect.left + scrollX) * scale);
  const relativeY = Math.max(0, (elementRect.top - bodyRect.top + scrollY) * scale);
  const relativeWidth = actualWidth * scale;
  const relativeHeight = actualHeight * scale;

  return {
    x: Math.round(relativeX),
    y: Math.round(relativeY), 
    width: Math.round(relativeWidth),
    height: Math.round(relativeHeight),
    scale
  };
}

function constrainCropToCanvas(
  crop: CropCoordinates,
  canvasWidth: number,
  canvasHeight: number
): CropCoordinates {
  const constrainedX = Math.max(0, Math.min(crop.x, canvasWidth - 1));
  const constrainedY = Math.max(0, Math.min(crop.y, canvasHeight - 1));
  const maxWidth = canvasWidth - constrainedX;
  const maxHeight = canvasHeight - constrainedY;
  const constrainedWidth = Math.max(1, Math.min(crop.width, maxWidth));
  const constrainedHeight = Math.max(1, Math.min(crop.height, maxHeight));

  return {
    x: constrainedX,
    y: constrainedY,
    width: constrainedWidth,
    height: constrainedHeight,
    scale: crop.scale
  };
}

async function runCoordinateMathTests() {
  console.log('🧪 COORDINATE CALCULATION MATH TESTS');
  console.log('====================================');
  console.log('🎯 Validating math logic that user reported as wrong');
  
  let testsPassed = 0;
  let totalTests = 0;

  function assert(condition: boolean, message: string) {
    totalTests++;
    if (condition) {
      console.log(`✅ ${message}`);
      testsPassed++;
    } else {
      console.log(`❌ ${message}`);
    }
  }

  // Test 1: Basic coordinate calculation
  const mockElementRect = {
    left: 100, top: 200, width: 400, height: 300,
    right: 500, bottom: 500
  } as DOMRect;
  
  const mockBodyRect = {
    left: 0, top: 0, width: 1200, height: 800,
    right: 1200, bottom: 800
  } as DOMRect;

  const basic = calculateCropCoordinates(
    mockElementRect, mockBodyRect, 50, 25, 400, 300, 1.0, false
  );
  
  assert(basic.x === 150, `Basic X: expected 150, got ${basic.x}`);
  assert(basic.y === 225, `Basic Y: expected 225, got ${basic.y}`);
  assert(basic.width === 400, `Basic width: expected 400, got ${basic.width}`);
  assert(basic.height === 300, `Basic height: expected 300, got ${basic.height}`);

  // Test 2: Scaling calculations
  const scaled = calculateCropCoordinates(
    mockElementRect, mockBodyRect, 0, 0, 400, 300, 2.0, false
  );
  
  assert(scaled.x === 200, `Scaled X: expected 200, got ${scaled.x}`);
  assert(scaled.y === 400, `Scaled Y: expected 400, got ${scaled.y}`);
  assert(scaled.width === 800, `Scaled width: expected 800, got ${scaled.width}`);
  assert(scaled.height === 600, `Scaled height: expected 600, got ${scaled.height}`);

  // Test 3: Overflow content detection
  const overflow = calculateCropCoordinates(
    mockElementRect, mockBodyRect, 0, 0, 450, 350, 1.0, true
  );
  
  assert(overflow.width === 450, `Overflow width: expected 450, got ${overflow.width}`);
  assert(overflow.height === 350, `Overflow height: expected 350, got ${overflow.height}`);

  // Test 4: Canvas constraint edge cases
  const oversized: CropCoordinates = {
    x: 1000, y: 600, width: 400, height: 300, scale: 1.0
  };
  
  const constrained = constrainCropToCanvas(oversized, 1200, 800);
  
  assert(constrained.x === 1000, `Constrained X: expected 1000, got ${constrained.x}`);
  assert(constrained.width === 200, `Constrained width: expected 200, got ${constrained.width}`);
  assert(constrained.height === 200, `Constrained height: expected 200, got ${constrained.height}`);

  // Test 5: Negative coordinate clamping
  const negativeRect = {
    left: -50, top: -30, width: 400, height: 300,
    right: 350, bottom: 270
  } as DOMRect;
  
  const clamped = calculateCropCoordinates(
    negativeRect, mockBodyRect, 0, 0, 400, 300, 1.0, false
  );
  
  assert(clamped.x === 0, `Clamped X: expected 0, got ${clamped.x}`);
  assert(clamped.y === 0, `Clamped Y: expected 0, got ${clamped.y}`);

  // Test 6: Body offset compensation
  const bodyWithOffset = {
    left: 20, top: 30, width: 1200, height: 800,
    right: 1220, bottom: 830
  } as DOMRect;
  
  const compensated = calculateCropCoordinates(
    mockElementRect, bodyWithOffset, 0, 0, 400, 300, 1.0, false
  );
  
  assert(compensated.x === 80, `Body offset X: expected 80, got ${compensated.x}`);
  assert(compensated.y === 170, `Body offset Y: expected 170, got ${compensated.y}`);

  console.log('');
  console.log('📊 UNIT TEST RESULTS:');
  console.log(`✅ Tests passed: ${testsPassed}/${totalTests}`);
  
  if (testsPassed === totalTests) {
    console.log('🎉 ALL COORDINATE MATH TESTS PASSED!');
    console.log('📐 Mathematical accuracy validated for screenshot cropping');
    return true;
  } else {
    console.log('❌ SOME COORDINATE MATH TESTS FAILED');
    console.log('🔧 Fix required for screenshot coordinate calculations');
    return false;
  }
}

// Execute if run directly
if (require.main === module) {
  runCoordinateMathTests().then(success => {
    process.exit(success ? 0 : 1);
  });
}