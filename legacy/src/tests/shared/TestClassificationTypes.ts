/**
 * Well-defined types for JTAG test classification system
 * Ensures type safety and consistency across all test organization
 */

/**
 * Test execution levels based on middle-out architecture
 */
export enum TestLevel {
  /** Foundation Layer - Core types, utilities, low-level components */
  FOUNDATION = 'foundation',
  
  /** Unit Layer - Individual components in isolation */
  UNIT = 'unit',
  
  /** Integration Layer - Component interactions, cross-module communication */
  INTEGRATION = 'integration',
  
  /** System Layer - Full system with all components running */
  SYSTEM = 'system',
  
  /** End-to-End Layer - User workflows, UI interactions, complete scenarios */
  E2E = 'e2e'
}

/**
 * Test importance levels based on JTAG debugging impact
 * Ordered from most critical to least critical
 */
export enum TestImportance {
  /** BLOCKER - Breaks JTAG debugging system itself (nothing works) */
  BLOCKER = 'blocker',
  
  /** CRITICAL - Must pass for core JTAG functionality (screenshots, commands, transport) */
  CRITICAL = 'critical',
  
  /** HIGH - Important features that enhance JTAG but don't break core debugging */
  HIGH = 'high',
  
  /** MEDIUM - Nice to have, good coverage, feature completeness */
  MEDIUM = 'medium',
  
  /** LOW - Edge cases, performance optimizations, polish */
  LOW = 'low',
  
  /** EXPERIMENTAL - New features being developed, can fail without blocking */
  EXPERIMENTAL = 'experimental'
}

/**
 * Test categories based on JTAG system components
 * Organized by criticality to JTAG debugging functionality
 */
export enum TestCategory {
  // 🚨 Core JTAG Infrastructure (BLOCKER if broken)
  /** WebSocket connection management, transport reliability */
  TRANSPORT = 'transport',
  
  /** Message routing, correlation, request/response handling */
  MESSAGING = 'messaging',
  
  /** Command routing, daemon communication, message dispatch */
  ROUTING = 'routing',
  
  /** Session management, state tracking, cleanup */
  SESSION = 'session',
  
  // 📸 Core JTAG Features (CRITICAL if broken)
  /** Visual debugging capabilities - screenshot capture, querySelector */
  SCREENSHOT = 'screenshot',
  
  /** Command execution system - discovery, validation, execution */
  COMMANDS = 'commands',
  
  /** JavaScript execution in browser - code injection, result capture */
  EXEC = 'exec',
  
  /** Database operations - CRUD, collections, persistence */
  DATA = 'data',
  
  // 📈 Enhanced Features (HIGH importance)
  /** Multi-user chat system - messaging, rooms, history */
  CHAT = 'chat',
  
  /** Real-time event system - publishing, routing, cross-context */
  EVENTS = 'events',
  
  /** UI widget system - lifecycle, state, integration */
  WIDGETS = 'widgets',
  
  // 📊 System Concerns (MEDIUM-LOW importance)
  /** Health monitoring, logging, console capture */
  HEALTH = 'health',
  
  /** Speed, resource usage, optimization */
  PERFORMANCE = 'performance',
  
  /** Authentication, authorization, input validation */
  SECURITY = 'security',
  
  /** Cross-browser, version compatibility */
  COMPATIBILITY = 'compatibility'
}

/**
 * Test profile names for package.json script execution
 */
export enum TestProfile {
  // Git Hook Profiles
  PRE_COMMIT = 'pre-commit',
  PRE_PUSH = 'pre-push',
  CI_PR = 'ci-pr',
  CI_MAIN = 'ci-main',
  
  // Importance Profiles
  BLOCKER = 'blocker',
  CRITICAL = 'critical',
  HIGH = 'high',
  MEDIUM = 'medium',
  LOW = 'low',
  EXPERIMENTAL = 'experimental',
  
  // Category Profiles
  TRANSPORT_CATEGORY = 'transport-category',
  SCREENSHOT_CATEGORY = 'screenshot-category',
  CHAT_CATEGORY = 'chat-category',
  EVENTS_CATEGORY = 'events-category',
  ROUTING_CATEGORY = 'routing-category',
  LOGGING_CATEGORY = 'logging-category',
  
  // System Requirement Profiles
  NO_SYSTEM = 'no-system',
  REQUIRES_SYSTEM = 'requires-system',
  
  // Combined Profiles
  CORE_DEBUGGING = 'core-debugging',
  ALL = 'all'
}

/**
 * Complete test metadata specification
 */
export interface TestMetadata {
  /** Architectural level of the test */
  level: TestLevel;
  
  /** Importance for JTAG debugging functionality */
  importance: TestImportance;
  
  /** System component category */
  category: TestCategory;
  
  /** Human-readable description of what the test validates */
  description: string;
  
  /** Maximum execution time in milliseconds */
  timeout?: number;
  
  /** Required dependencies (commands, daemons, etc.) */
  dependencies?: string[];
  
  /** Skip in continuous integration */
  skipInCI?: boolean;
  
  /** Requires full JTAG system to be running */
  requiresSystem?: boolean;
  
  /** Blocks commit if test fails */
  blocksCommit?: boolean;
  
  /** Blocks push if test fails */
  blocksPush?: boolean;
}

/**
 * Test execution configuration
 */
export interface TestRunConfig {
  /** Test levels to include */
  level?: TestLevel[];
  
  /** Importance levels to include */
  importance?: TestImportance[];
  
  /** Categories to include */
  category?: TestCategory[];
  
  /** Maximum concurrent test execution */
  maxConcurrency?: number;
  
  /** Stop execution on first failure */
  failFast?: boolean;
  
  /** Verbose output */
  verbose?: boolean;
  
  /** Skip tests that require system startup */
  skipSystem?: boolean;
  
  /** Maximum total execution time in seconds */
  maxTimeSeconds?: number;
}

/**
 * Test execution result
 */
export interface TestResult {
  /** Test identifier */
  name: string;
  
  /** Test passed successfully */
  success: boolean;
  
  /** Execution time in milliseconds */
  duration: number;
  
  /** Test metadata */
  metadata: TestMetadata;
  
  /** Standard output from test */
  output?: string;
  
  /** Error message if test failed */
  error?: string;
  
  /** Timestamp when test started */
  startTime?: Date;
  
  /** Timestamp when test completed */
  endTime?: Date;
}

/**
 * Test profile configuration mapping
 */
export interface TestProfileConfig {
  /** Profile description */
  description: string;
  
  /** Test execution configuration */
  config: TestRunConfig;
  
  /** Message to display when running profile */
  message: string;
  
  /** Maximum execution time for this profile */
  maxTime?: number;
  
  /** Profile blocks commits if tests fail */
  blocksCommit?: boolean;
  
  /** Profile blocks pushes if tests fail */
  blocksPush?: boolean;
}

/**
 * Type guard functions for runtime validation
 */
export const TestClassificationGuards = {
  isTestLevel: (value: string): value is TestLevel => {
    return Object.values(TestLevel).includes(value as TestLevel);
  },
  
  isTestImportance: (value: string): value is TestImportance => {
    return Object.values(TestImportance).includes(value as TestImportance);
  },
  
  isTestCategory: (value: string): value is TestCategory => {
    return Object.values(TestCategory).includes(value as TestCategory);
  },
  
  isTestProfile: (value: string): value is TestProfile => {
    return Object.values(TestProfile).includes(value as TestProfile);
  }
};

/**
 * Importance ordering for execution priority
 */
export const IMPORTANCE_ORDER: Record<TestImportance, number> = {
  [TestImportance.BLOCKER]: 0,
  [TestImportance.CRITICAL]: 1,
  [TestImportance.HIGH]: 2,
  [TestImportance.MEDIUM]: 3,
  [TestImportance.LOW]: 4,
  [TestImportance.EXPERIMENTAL]: 5
};

/**
 * Category criticality mapping for JTAG debugging
 */
export const CATEGORY_CRITICALITY: Record<TestCategory, TestImportance> = {
  // BLOCKER categories
  [TestCategory.TRANSPORT]: TestImportance.BLOCKER,
  [TestCategory.MESSAGING]: TestImportance.BLOCKER,
  [TestCategory.ROUTING]: TestImportance.BLOCKER,
  [TestCategory.SESSION]: TestImportance.BLOCKER,
  
  // CRITICAL categories
  [TestCategory.SCREENSHOT]: TestImportance.CRITICAL,
  [TestCategory.COMMANDS]: TestImportance.CRITICAL,
  [TestCategory.EXEC]: TestImportance.CRITICAL,
  [TestCategory.DATA]: TestImportance.CRITICAL,
  
  // HIGH categories
  [TestCategory.CHAT]: TestImportance.HIGH,
  [TestCategory.EVENTS]: TestImportance.HIGH,
  [TestCategory.WIDGETS]: TestImportance.HIGH,
  
  // MEDIUM-LOW categories
  [TestCategory.HEALTH]: TestImportance.MEDIUM,
  [TestCategory.PERFORMANCE]: TestImportance.MEDIUM,
  [TestCategory.SECURITY]: TestImportance.LOW,
  [TestCategory.COMPATIBILITY]: TestImportance.LOW
};