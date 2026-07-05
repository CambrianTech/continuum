/**
 * Test Utilities Index - Clean module exports for shared test infrastructure
 * 
 * Organized by functional domain rather than meaningless adjectives.
 * Each module has a clear, descriptive purpose.
 */

// =============================================================================
// CORE INFRASTRUCTURE - Foundation utilities
// =============================================================================

// Client connection management
export * from './JTAGClientFactory';

// Test execution and orchestration  
export * from './TestExecution';

// Standardized constants and configuration
export * from './TestConstants';

// Comprehensive assertion utilities
export * from './TestAssertions';

// =============================================================================
// SPECIALIZED TESTING - Domain-specific utilities
// =============================================================================

// Visual testing and screenshot validation
export * from './ScreenshotTesting';

// Theme system testing and validation
export * from './ThemeTesting';

// =============================================================================
// CONVENIENCE RE-EXPORTS - Most commonly used functions
// =============================================================================

export {
  // Client management
  connectJTAGClient,
  executeJTAGCommand,
  takeJTAGScreenshot
} from './JTAGClientFactory';

export {
  // Test execution
  runSingleTest,
  runTestSuite
} from './TestExecution';

export {
  // Screenshot testing
  captureTestScreenshot,
  captureMultipleScreenshots,
  capturePageScreenshots
} from './ScreenshotTesting';

export {
  // Theme testing
  testThemeWithScreenshot,
  testAllThemesWithScreenshots,
  switchToTheme
} from './ThemeTesting';

export {
  // Assertions
  assertTestPassed,
  assertSuitePassed,
  assertScreenshotValid,
  assertThemeTestPassed,
  assertFileExists
} from './TestAssertions';