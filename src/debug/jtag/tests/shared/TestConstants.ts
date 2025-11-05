/**
 * Test Constants - Centralized configuration for all test suites
 * 
 * Eliminates magic strings and numbers across tests with descriptive,
 * well-organized constant definitions.
 */

// =============================================================================
// TIMEOUT CONFIGURATIONS - Operation-specific timing
// =============================================================================

export const TEST_TIMEOUTS = {
  QUICK_OPERATION: 5000,      // 5s for simple commands (ping, list)
  STANDARD_OPERATION: 15000,   // 15s for normal commands (screenshot, theme)
  COMPLEX_OPERATION: 30000,    // 30s for complex operations (compilation, large files)
  INTEGRATION_TEST: 60000,     // 60s for full cross-system integration tests
  SYSTEM_STARTUP: 45000        // 45s for JTAG system bootstrap
} as const;

// =============================================================================
// DOM SELECTORS - Standard element targeting
// =============================================================================

export const DOM_SELECTORS = {
  // Page structure
  BODY: 'body',
  APP_CONTAINER: '.app-container',
  
  // Chat components  
  CHAT_WIDGET: 'chat-widget',
  CHAT_INPUT: 'input[placeholder*="message"], textarea[placeholder*="message"], .chat-input, #chat-input',
  CHAT_SEND_BUTTON: 'button[type="submit"], .send-button, .chat-send',
  
  // Navigation components
  SIDEBAR: 'continuum-sidebar',
  
  // Theme components
  THEME_SELECTOR: '.theme-selector, [data-theme]'
} as const;

// =============================================================================
// FILE VALIDATION - Size and format requirements
// =============================================================================

export const FILE_VALIDATION = {
  MIN_SCREENSHOT_SIZE: 1000,        // 1KB minimum for valid screenshots
  MIN_LOG_SIZE: 100,                // 100 bytes minimum for meaningful logs
  MAX_REASONABLE_SCREENSHOT: 10 * 1024 * 1024, // 10MB max reasonable size
  
  SUPPORTED_IMAGE_FORMATS: ['png', 'jpg', 'jpeg', 'webp'] as const,
  DEFAULT_SCREENSHOT_FORMAT: 'png' as const
} as const;

// =============================================================================
// RETRY CONFIGURATIONS - Failure recovery patterns
// =============================================================================

export const RETRY_PATTERNS = {
  DEFAULT_ATTEMPTS: 3,              // Standard retry count
  QUICK_ATTEMPTS: 2,                // For fast operations
  PERSISTENT_ATTEMPTS: 5,           // For critical operations
  DELAY_BETWEEN_ATTEMPTS: 1000      // 1s between retries
} as const;

// =============================================================================
// THEME DEFINITIONS - Available theme configurations
// =============================================================================

export const THEME_CATALOG = {
  ALL_THEMES: ['base', 'classic', 'cyberpunk', 'light', 'monochrome', 'retro-mac'] as const,
  DEFAULT_THEME: 'base' as const,
  THEME_SWITCH_DELAY: 1500          // 1.5s for theme changes to apply
} as const;

// =============================================================================
// TEST CLASSIFICATION - Category and type definitions
// =============================================================================

export const TEST_CATEGORIES = {
  UNIT: 'unit',
  INTEGRATION: 'integration', 
  E2E: 'e2e',
  PERFORMANCE: 'performance',
  VISUAL: 'visual'
} as const;

export const TEST_ENVIRONMENTS = {
  BROWSER: 'browser',
  SERVER: 'server', 
  CROSS_CONTEXT: 'cross-context'
} as const;

// =============================================================================
// COMMAND EXECUTION - Standard command configurations
// =============================================================================

export const COMMAND_DEFAULTS = {
  SCREENSHOT: {
    FORMAT: 'png',
    QUALITY: 90,
    SELECTOR: DOM_SELECTORS.BODY
  },
  
  THEME_SWITCH: {
    APPLY_DELAY: THEME_CATALOG.THEME_SWITCH_DELAY,
    VERIFY_SCREENSHOT: true
  },
  
  CHAT_INTERACTION: {
    TYPE_DELAY: 100,              // Delay between keystrokes
    CLEAR_BEFORE_TYPE: true,
    WAIT_FOR_RESPONSE: 2000       // Wait for chat response
  }
} as const;

// Type definitions for constants
export type TestTimeout = typeof TEST_TIMEOUTS[keyof typeof TEST_TIMEOUTS];
export type DOMSelector = typeof DOM_SELECTORS[keyof typeof DOM_SELECTORS];
export type ThemeName = typeof THEME_CATALOG.ALL_THEMES[number];
export type TestCategory = typeof TEST_CATEGORIES[keyof typeof TEST_CATEGORIES];
export type TestEnvironment = typeof TEST_ENVIRONMENTS[keyof typeof TEST_ENVIRONMENTS];