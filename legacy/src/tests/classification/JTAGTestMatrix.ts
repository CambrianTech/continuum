/**
 * JTAG Test Classification Matrix
 * Comprehensive test organization for autonomous development
 */

import { TestLevel, TestImportance, TestCategory } from '../shared/TestDecorators';

/**
 * JTAG Test Priority Matrix
 * Based on "what breaks JTAG debugging if this fails?"
 */
export const JTAG_TEST_MATRIX = {

  // 🚨 BLOCKER TESTS - NOTHING works if these fail
  // These BLOCK commits - core infrastructure that everything depends on
  BLOCKER: {
    description: "Infrastructure that breaks JTAG completely",
    blockCommit: true,
    maxTime: 15, // seconds
    tests: {
      'TransportFoundation': {
        level: TestLevel.FOUNDATION,
        category: TestCategory.TRANSPORT,
        focus: 'WebSocket connection, message correlation, timeout handling'
      },
      'RouterCore': {
        level: TestLevel.FOUNDATION, 
        category: TestCategory.ROUTING,
        focus: 'Daemon registration, command routing, message dispatch'
      },
      'MessageSystem': {
        level: TestLevel.FOUNDATION,
        category: TestCategory.MESSAGING,
        focus: 'Message encoding, correlation IDs, response handling'
      },
      'SessionCore': {
        level: TestLevel.UNIT,
        category: TestCategory.SESSION,
        focus: 'Session creation, state tracking, cleanup'
      }
    }
  },

  // 📸 CRITICAL TESTS - Core JTAG debugging features
  // These are the "why JTAG exists" features - visual debugging, execution, logging
  CRITICAL: {
    description: "Core JTAG debugging capabilities",
    blockCommit: false,
    blockPush: true,
    maxTime: 120, // seconds
    tests: {
      'ScreenshotSystem': {
        level: TestLevel.INTEGRATION,
        category: TestCategory.SCREENSHOT,
        focus: 'Screenshot capture, querySelector, file saving, browser automation'
      },
      'LoggingSystem': {
        level: TestLevel.INTEGRATION,
        category: TestCategory.HEALTH, // Logging is about system observability
        focus: 'Console capture, log routing, file persistence, log levels'
      },
      'CommandExecution': {
        level: TestLevel.INTEGRATION,
        category: TestCategory.COMMANDS,
        focus: 'Command discovery, parameter validation, execution pipeline'
      },
      'JavaScriptExec': {
        level: TestLevel.SYSTEM,
        category: TestCategory.EXEC,
        focus: 'Browser JS execution, result capture, error handling'
      },
      'EventRouting': {
        level: TestLevel.INTEGRATION,
        category: TestCategory.EVENTS,
        focus: 'Event publishing, cross-context routing, DOM events'
      },
      'DatabaseOps': {
        level: TestLevel.INTEGRATION,
        category: TestCategory.DATA,
        focus: 'CRUD operations, collection management, query filtering'
      }
    }
  },

  // 📈 HIGH TESTS - Enhanced JTAG features
  // Important for advanced workflows but basic debugging still works
  HIGH: {
    description: "Enhanced JTAG collaboration and advanced features",
    blockCommit: false,
    blockPush: false,
    blockCI: true,
    maxTime: 300, // seconds
    tests: {
      'MultiUserChat': {
        level: TestLevel.SYSTEM,
        category: TestCategory.CHAT,
        focus: 'Multi-user messaging, room management, history persistence'
      },
      'RealTimeEvents': {
        level: TestLevel.SYSTEM,
        category: TestCategory.EVENTS,
        focus: 'Cross-user event delivery, widget updates, state synchronization'
      },
      'WidgetSystem': {
        level: TestLevel.SYSTEM,
        category: TestCategory.WIDGETS,
        focus: 'Widget lifecycle, state management, UI integration'
      },
      'BrowserIntegration': {
        level: TestLevel.E2E,
        category: TestCategory.COMPATIBILITY,
        focus: 'Cross-browser compatibility, automation reliability'
      }
    }
  },

  // 📊 MEDIUM TESTS - System quality and robustness
  MEDIUM: {
    description: "System quality, performance, edge cases",
    blockCommit: false,
    blockPush: false,
    blockCI: false,
    maxTime: 600, // seconds
    tests: {
      'PerformanceMetrics': {
        level: TestLevel.INTEGRATION,
        category: TestCategory.PERFORMANCE,
        focus: 'Response times, memory usage, connection efficiency'
      },
      'ErrorRecovery': {
        level: TestLevel.SYSTEM,
        category: TestCategory.HEALTH,
        focus: 'Graceful degradation, reconnection, error boundaries'
      },
      'SecurityBasics': {
        level: TestLevel.INTEGRATION,
        category: TestCategory.SECURITY,
        focus: 'Input validation, XSS protection, safe execution'
      }
    }
  },

  // 🧪 LOW/EXPERIMENTAL TESTS - Future features and optimizations
  LOW: {
    description: "Future features, optimizations, experimental capabilities",
    blockCommit: false,
    blockPush: false,
    blockCI: false,
    runInNightly: true,
    tests: {
      'P2PNetworking': {
        level: TestLevel.E2E,
        category: TestCategory.TRANSPORT,
        focus: 'Multi-node routing, mesh networking, distributed commands'
      },
      'AIIntegration': {
        level: TestLevel.E2E,
        category: TestCategory.CHAT,
        focus: 'OpenAI/Anthropic integration, persona management'
      }
    }
  }
};

/**
 * Generate test file paths from matrix
 */
export function generateTestPaths(): { [key: string]: string } {
  const paths: { [key: string]: string } = {};
  
  Object.entries(JTAG_TEST_MATRIX).forEach(([priority, config]) => {
    if (config.tests) {
      Object.entries(config.tests).forEach(([testName, testSpec]) => {
        const categoryDir = testSpec.category.toLowerCase();
        const priorityPrefix = priority.toLowerCase();
        paths[testName] = `tests/classified/${priorityPrefix}/${categoryDir}/${testName}.test.ts`;
      });
    }
  });
  
  return paths;
}

/**
 * Git Hook Test Profiles based on matrix
 */
export const GIT_HOOK_PROFILES = {
  
  // Pre-commit: Only BLOCKER tests
  'pre-commit': {
    description: "🚨 BLOCKER TESTS ONLY - Must pass to commit",
    importance: [TestImportance.BLOCKER],
    category: [TestCategory.TRANSPORT, TestCategory.ROUTING, TestCategory.MESSAGING, TestCategory.SESSION],
    maxTime: JTAG_TEST_MATRIX.BLOCKER.maxTime,
    failFast: true,
    skipSystem: true,
    message: "If these fail, JTAG debugging is completely broken"
  },
  
  // Pre-push: CRITICAL tests (skip BLOCKER since we tested those)
  'pre-push': {
    description: "📸 CRITICAL TESTS - Core JTAG debugging features",
    importance: [TestImportance.CRITICAL],
    category: [TestCategory.SCREENSHOT, TestCategory.COMMANDS, TestCategory.EXEC, TestCategory.EVENTS, TestCategory.DATA],
    maxTime: JTAG_TEST_MATRIX.CRITICAL.maxTime,
    failFast: true,
    skipSystem: false,
    message: "Visual debugging, logging, and command execution must work"
  },
  
  // CI Pull Request: Include HIGH priority
  'ci-pr': {
    description: "📈 CRITICAL + HIGH - Full feature validation", 
    importance: [TestImportance.CRITICAL, TestImportance.HIGH],
    category: [TestCategory.SCREENSHOT, TestCategory.COMMANDS, TestCategory.EXEC, TestCategory.EVENTS, TestCategory.DATA, TestCategory.CHAT, TestCategory.WIDGETS],
    maxTime: JTAG_TEST_MATRIX.HIGH.maxTime,
    failFast: false,
    skipSystem: false,
    message: "Complete JTAG feature set must be operational"
  }
};

/**
 * Test execution order based on JTAG debugging priorities
 */
export const EXECUTION_ORDER = [
  // 1. Foundation first - nothing works without this
  { priority: 'BLOCKER', description: 'Core infrastructure' },
  
  // 2. Critical JTAG features - the reason JTAG exists
  { priority: 'CRITICAL', description: 'Visual debugging, logging, execution' },
  
  // 3. Enhanced features - collaboration and advanced workflows
  { priority: 'HIGH', description: 'Multi-user features, widgets' },
  
  // 4. Quality and robustness
  { priority: 'MEDIUM', description: 'Performance, error handling' },
  
  // 5. Future capabilities
  { priority: 'LOW', description: 'Experimental features' }
];

export default JTAG_TEST_MATRIX;