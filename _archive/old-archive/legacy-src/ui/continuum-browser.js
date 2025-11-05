// package.json
var package_default = {
  name: "continuum",
  version: "0.2.3187",
  description: "Designed by AI and humans for AI and humans - A standard protocol for configuring AI assistants to work consistently with codebases",
  main: "src/core/continuum-core.cjs",
  bin: {
    continuum: "./continuum"
  },
  type: "module",
  workspaces: [
    "packages/*"
  ],
  scripts: {
    compile: "npx tsc --noEmit --project .",
    start: "npm run log:npm-start manual && npm run compile && npm run build:browser-ts && ./continuum",
    jtag: "npm run log:npm-start jtag-command && npm start && echo '' && echo '\u{1F6F8} JTAG System Ready' && echo '  continuum help jtag              # Show available JTAG methods' && echo '  continuum jtag                   # Default JTAG probe'",
    "test:jtag": "cd src/debug/jtag && npm test",
    "test:jtag:layer": "cd src/debug/jtag && npm run test:layer-",
    "install:global": "node scripts/install.js",
    prepare: "husky",
    postinstall: "lerna run prepare",
    "build:browser-ts": "npm run clean:all && npm run version:bump && rm -f src/ui/continuum-browser.js* && node scripts/build-browser.cjs",
    "build:browser-hot": "npm run version:bump && rm -f src/ui/continuum-browser.js* && node scripts/build-browser.cjs",
    "build:ui-components": "tsc --project tsconfig.ui.json",
    "build:ui": "tsc --project tsconfig.ui.json",
    "dev:browser": "npm run build:browser-ts -- --watch",
    "dev:ui": "npm run build:ui-components -- --watch",
    test: "echo 'WARNING_legacy system, cd into src/debug/jtag IMMEDIATELY!' && npm run compile && npm run test:unit && npm run test:integration:layer2-3",
    "test:integration:layer2-3": "npm run test:integration:eventbus && npm run test:integration:routing && npm run test:integration:html && npm run test:integration:wildcard && npm run test:integration:types && npm run test:integration:modules && npm run test:integration:browser-manager",
    "test:full": "npx tsx src/system/testing/test/universal-layer-runner.ts",
    "test:layer": "npx tsx src/system/testing/test/universal-layer-runner.ts --layer=",
    "test:widgets": "npx tsx src/testing/IntelligentModularTestRunner.ts widget",
    "test:daemons": "npx tsx src/testing/IntelligentModularTestRunner.ts daemon",
    "test:chatroom": "npx tsx src/daemons/chatroom/test/run-tests.ts",
    "test:chatroom:unit": "npx tsx --test src/daemons/chatroom/test/unit/ChatRoomDaemon.test.ts",
    "test:chatroom:integration": "npx tsx --test src/daemons/chatroom/test/integration/ChatRoomDaemon.integration.test.ts",
    "test:commands": "npx tsx src/testing/IntelligentModularTestRunner.ts command",
    "test:compliance": "npx tsx src/testing/ModuleComplianceReport.ts --use-whitelist",
    "test:compliance:strict": "npx tsx src/testing/ModuleComplianceReport.ts --exit-on-failure",
    "test:compliance:details": "npx tsx src/testing/ModuleComplianceReport.ts --use-whitelist --details",
    "test:compliance:incremental": "npx tsx src/testing/IncrementalComplianceTracker.ts --track-history",
    "test:compliance:focus": "npx tsx src/testing/IncrementalComplianceTracker.ts",
    "test:compliance:graduation": "npx tsx src/testing/ModuleGraduationTracker.ts",
    "test:compliance:next": "npx tsx src/testing/ModuleGraduationTracker.ts --suggest",
    "test:eslint:graduation": "npx tsx src/testing/ESLintGraduationTracker.ts",
    "test:eslint:next": "npx tsx src/testing/ESLintGraduationTracker.ts --suggest",
    "test:quality:commit": "npx tsx src/testing/QualityEnforcementEngine.ts --commit",
    "test:quality:push": "npx tsx src/testing/QualityEnforcementEngine.ts --push",
    "fix:command-compliance": "node scripts/fix-command-compliance.js",
    "fix:command-compliance:dry-run": "node scripts/fix-command-compliance.js --dry-run",
    "test:integration:all": "npm run test:integration:eventbus && npm run test:integration:routing && npm run test:integration:html && npm run test:integration:wildcard && npm run test:integration:types && npm run test:integration:modules",
    "test:integration:eventbus": "npx tsx --test src/test/integration/DaemonEventBus.integration.test.ts",
    "test:integration:routing": "npx tsx --test src/test/integration/CommandRouting.integration.test.ts",
    "test:integration:html": "npx tsx --test src/test/integration/HTMLRendering.integration.test.ts",
    "test:integration:wildcard": "npx tsx --test src/test/integration/WildcardRouting.integration.test.ts",
    "test:integration:types": "npx tsx --test src/test/integration/TypeSafety.integration.test.ts",
    "test:integration:modules": "npx tsx --test src/test/integration/DaemonModuleStructure.integration.test.ts",
    "test:integration:browser-manager": "npx tsx --test src/daemons/browser-manager/test/integration/BrowserManager.integration.test.ts",
    "test:unit:browser-manager": "npx tsx --test src/daemons/browser-manager/test/unit/BrowserTabAdapter.test.ts",
    "test:system": "npx tsx --test src/test/integration/DaemonSystem.integration.test.ts",
    "test:layer4": "npx tsx --test src/test/integration/SystemIntegration.layer4.test.ts",
    "test:unit": "npm run test:unit:js && npm run test:unit:python",
    "test:unit:js": "npm run test:unit:browser-manager",
    "test:unit:python": "echo 'Python unit tests pending configuration'",
    "test:integration": "npm run test:integration:js && npm run test:integration:python && npm run test:browser",
    "test:integration:js": "jest __tests__/integration --testPathPattern='\\.(js|cjs|ts)$' --config __tests__/config/jest.config.cjs",
    "test:integration:python": "cd __tests__ && python3 -m pytest integration -v -c config/pytest.ini",
    "test:functional": "jest __tests__/functional --config __tests__/config/jest.config.cjs && cd __tests__ && python3 -m pytest functional -v -c config/pytest.ini",
    "test:comprehensive": "jest __tests__/comprehensive --config __tests__/config/jest.config.cjs && cd __tests__ && python3 -m pytest comprehensive -v -c config/pytest.ini",
    "test:critical": "jest __tests__/critical --config __tests__/config/jest.config.cjs",
    "test:screenshot": "node __tests__/config/test-runner.cjs screenshot",
    "test:websocket": "npx tsx src/testing/ModularTestRunner.ts integration",
    "test:browser": "npx tsx --test src/daemons/browser-manager/test/integration/BrowserManager.integration.test.ts",
    "test:visual": "npx tsx src/ui/test/VisualBrowserIntegration.ts",
    "test:ai": "jest __tests__/integration/ai --config __tests__/config/jest.config.cjs",
    "test:ui": "jest __tests__/integration/ui --testPathPattern='\\.(js|cjs|ts)$' --config __tests__/config/jest.config.cjs && cd __tests__ && python3 -m pytest integration/ui -v -c config/pytest.ini",
    "test:watch": "jest --watch --config __tests__/config/jest.config.cjs",
    "test:coverage": "npm run test:coverage:js && npm run test:coverage:python",
    "test:coverage:js": "jest --coverage --config __tests__/config/jest.config.cjs",
    "test:coverage:python": "cd __tests__ && python3 -m pytest --cov=../src --cov=../python-client/continuum_client --cov-report=html --cov-report=term -c config/pytest.ini",
    "log:npm-start": "npx tsx src/monitoring/build-logger.ts npm-start",
    "log:build": "npx tsx src/monitoring/build-logger.ts build",
    "log:git-hook": "npx tsx src/monitoring/build-logger.ts git-hook",
    "log:browser": "npx tsx src/monitoring/build-logger.ts browser",
    "log:show": "npx tsx src/monitoring/build-logger.ts show",
    build: "npm run clean:all && npm run build:browser-ts && npm run lint && node check-imports.cjs && npm run test",
    lint: "eslint . --ext .ts,.js --config eslint.config.js",
    format: 'prettier --write "**/*.{js,ts,json,md}"',
    ci: "./scripts/test-ci.sh",
    "validate-schema": "node scripts/validate-schema.js",
    "check-deps": "npm audit --production && npm outdated",
    "version:patch": "npm version patch --no-git-tag-version",
    "version:minor": "npm version minor --no-git-tag-version",
    "version:major": "npm version major --no-git-tag-version",
    "publish:dev": "lerna publish --dist-tag dev --no-push --no-git-tag-version",
    "build:packages": "npm run build --workspaces",
    "test:packages": "npm run test --workspaces",
    "lint:packages": "npm run lint --workspaces",
    demo: "npx ts-node demo-complete-system.ts",
    "test-system": "npx jest tests/integration-full-system.test.ts",
    "version:bump": "npm version patch --no-git-tag-version",
    clean: "scripts/clean-sessions.sh",
    "clean:all": "scripts/clean-sessions.sh --all",
    "build:clean": "scripts/clean-sessions.sh && npm run build"
  },
  keywords: [
    "ai",
    "configuration",
    "protocol",
    "assistant",
    "claude",
    "gpt",
    "gemini",
    "llama",
    "mistral",
    "meta",
    "anthropic",
    "openai",
    "google",
    "copilot",
    "standardization",
    "cognitive-systems",
    "contextual",
    "devops",
    "devsecops"
  ],
  author: "Cambrian Technologies",
  license: "MIT",
  devDependencies: {
    "@babel/core": "^7.27.4",
    "@babel/plugin-proposal-class-properties": "^7.18.6",
    "@babel/plugin-proposal-decorators": "^7.27.1",
    "@babel/plugin-transform-class-properties": "^7.27.1",
    "@babel/preset-env": "^7.27.2",
    "@babel/preset-typescript": "^7.27.1",
    "@types/jest": "^29.5.14",
    "@types/node": "^22.15.29",
    "@typescript-eslint/eslint-plugin": "^8.29.1",
    "@typescript-eslint/parser": "^8.29.1",
    "babel-jest": "^29.7.0",
    eslint: "^9.24.0",
    husky: "^9.1.7",
    jest: "^29.5.0",
    "jest-environment-jsdom": "^29.7.0",
    lerna: "^8.2.1",
    prettier: "^3.5.3",
    supertest: "^7.1.1",
    "ts-jest": "^29.3.4",
    tsx: "^4.20.3",
    typescript: "^5.8.3"
  },
  engines: {
    node: ">=20.0.0"
  },
  continuum: {
    quality: {
      enforcement: {
        commit: "warn",
        push: "strict"
      },
      schema: {
        version: "1.0.0",
        description: "Quality standards are now configured per-module in their package.json files",
        documentation: "See src/types/ModuleQualitySchema.ts for the complete schema definition"
      }
    }
  },
  dependencies: {
    "@anthropic-ai/sdk": "^0.52.0",
    ajv: "^8.17.1",
    dotenv: "^16.5.0",
    glob: "^11.0.1",
    html2canvas: "^1.4.1",
    inquirer: "^12.5.2",
    jsdom: "^26.1.0",
    "node-fetch": "^3.3.2",
    "node-notifier": "^10.0.1",
    openai: "^5.0.1",
    punycode: "^2.3.1",
    systray: "^1.0.5",
    ws: "^8.18.3",
    yaml: "^2.7.1"
  }
};

// src/types/shared/ConsoleTypes.ts
var Console;
((Console2) => {
  let Level;
  ((Level2) => {
    Level2["ERROR"] = "error";
    Level2["WARN"] = "warn";
    Level2["INFO"] = "info";
    Level2["LOG"] = "log";
    Level2["DEBUG"] = "debug";
    Level2["TRACE"] = "trace";
    Level2["TABLE"] = "table";
    Level2["GROUP"] = "group";
    Level2["GROUP_END"] = "groupEnd";
    Level2["PROBE"] = "probe";
  })(Level = Console2.Level || (Console2.Level = {}));
  class MessageUtils {
    /**
     * Properly serialize a console argument for wire transfer
     */
    static serializeArgument(arg) {
      if (arg === null) return null;
      if (arg === void 0) return void 0;
      const argType = typeof arg;
      if (argType === "string" || argType === "number" || argType === "boolean") {
        return arg;
      }
      if (Array.isArray(arg)) {
        return arg.map((item) => this.serializeArgument(item));
      }
      if (argType === "object") {
        try {
          if (arg instanceof Error) {
            return {
              type: "object",
              value: {
                name: arg.name,
                message: arg.message,
                stack: arg.stack,
                cause: arg.cause
                // Error.cause is ES2022, use any for compatibility
              },
              stringRepresentation: `${arg.name}: ${arg.message}
${arg.stack || "No stack trace available"}`,
              originalType: "[object Error]"
            };
          }
          const jsonString = JSON.stringify(arg, null, 2);
          return {
            type: "object",
            value: arg,
            stringRepresentation: jsonString,
            originalType: Object.prototype.toString.call(arg)
          };
        } catch (error) {
          return {
            type: "object",
            value: {},
            stringRepresentation: `[${Object.prototype.toString.call(arg)}]`,
            originalType: Object.prototype.toString.call(arg)
          };
        }
      }
      return String(arg);
    }
    /**
     * Convert console arguments to a readable string with proper object formatting
     */
    static argumentsToString(args) {
      return args.map((arg) => {
        if (arg === null) return "null";
        if (arg === void 0) return "undefined";
        if (typeof arg === "object" && arg !== null) {
          if (Array.isArray(arg)) {
            return `[${arg.map((item) => this.argumentsToString([item])).join(", ")}]`;
          }
          if ("type" in arg && arg.type === "object") {
            return arg.stringRepresentation;
          }
        }
        return String(arg);
      }).join(" ");
    }
    /**
     * Create a properly formatted console log entry
     */
    static createLogEntry(level, args, metadata) {
      const serializedArgs = args.map((arg) => this.serializeArgument(arg));
      const mainMessage = serializedArgs.length > 0 ? this.argumentsToString([serializedArgs[0]]) : "";
      const additionalArgs = serializedArgs.slice(1);
      const stackTrace = metadata?.stackTrace || this.captureStackTrace();
      const browserContext = this.captureBrowserContext();
      return {
        level,
        message: mainMessage,
        arguments: additionalArgs,
        timestamp: (/* @__PURE__ */ new Date()).toISOString(),
        metadata: {
          stackTrace,
          ...browserContext,
          ...metadata
        }
      };
    }
    /**
     * Capture actual stack trace from current execution context
     */
    static captureStackTrace() {
      try {
        const error = new Error();
        if (error.stack) {
          const lines = error.stack.split("\n");
          const filteredLines = lines.filter(
            (line) => !line.includes("MessageUtils.captureStackTrace") && !line.includes("MessageUtils.createLogEntry") && !line.includes("ConsoleForwarder.forwardConsole") && !line.includes("ClientConsoleManager.forwardConsole") && !line.includes("ClientConsoleManager.") && !line.includes("ClientLoggerDaemon.") && !line.includes("console.log") && !line.includes("console.warn") && !line.includes("console.error") && !line.includes("console.info") && !line.includes("console.debug") && !line.includes("console.trace")
          );
          return filteredLines.join("\n");
        }
      } catch (e) {
      }
      return "";
    }
    /**
     * Capture comprehensive browser context
     */
    static captureBrowserContext() {
      if (typeof window === "undefined") {
        return {};
      }
      const context = {
        url: window.location.href,
        userAgent: navigator.userAgent,
        fileName: window.location.pathname,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
        timestamp: (/* @__PURE__ */ new Date()).toISOString()
      };
      return context;
    }
  }
  Console2.MessageUtils = MessageUtils;
})(Console || (Console = {}));

// src/types/shared/core/ContinuumTypes.ts
function generateUUID() {
  if (typeof crypto !== "undefined" && crypto.randomUUID) {
    return crypto.randomUUID();
  } else {
    return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, function(c) {
      const r = Math.random() * 16 | 0;
      const v = c === "x" ? r : r & 3 | 8;
      return v.toString(16);
    });
  }
}
var uuidValidator = {
  /**
   * Validate UUID format
   */
  validate: (uuid) => {
    return typeof uuid === "string" && uuid.length === 36 && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(uuid);
  },
  /**
   * Generate a new UUID
   */
  generate: () => generateUUID()
};
var continuumContextFactory = {
  /**
   * Create a new ContinuumContext with required fields
   */
  create: (options = {}) => ({
    sessionId: options.sessionId ?? uuidValidator.generate(),
    sessionStartTime: (/* @__PURE__ */ new Date()).toISOString(),
    ...options
  }),
  /**
   * Merge contexts with proper precedence
   */
  merge: (base, override) => ({
    ...base,
    ...override,
    // Preserve critical fields
    sessionId: override.sessionId ?? base.sessionId
  }),
  /**
   * Clone context with different environment
   * Preserves session, user, and other context but changes execution environment
   */
  withEnvironment: (context, environment) => ({
    ...context,
    environment
  }),
  /**
   * Push new execution frame onto context stack
   * Creates proper call stack trace through system layers
   */
  push: (context, frame) => {
    const newFrame = {
      ...frame,
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
    return {
      ...context,
      environment: frame.environment,
      executionStack: [...context.executionStack || [], newFrame]
    };
  },
  /**
   * Pop execution frame from context stack
   * Returns to previous execution environment
   */
  pop: (context) => {
    const stack = context.executionStack || [];
    if (stack.length === 0) {
      return context;
    }
    const newStack = stack.slice(0, -1);
    const previousEnvironment = newStack.length > 0 ? newStack[newStack.length - 1].environment : context.environment;
    const result2 = {
      ...context,
      executionStack: newStack
    };
    if (previousEnvironment) {
      result2.environment = previousEnvironment;
    }
    return result2;
  },
  /**
   * Validate that a context has required fields
   */
  validate: (context) => {
    if (typeof context !== "object" || context === null) {
      return false;
    }
    const ctx = context;
    if (typeof ctx.sessionId !== "string" || ctx.sessionId === null) {
      return false;
    }
    return uuidValidator.validate(ctx.sessionId);
  }
};

// src/daemons/logger/shared/ConsoleTypes.ts
var ConsoleUtils = class _ConsoleUtils {
  /**
   * Serialize console arguments for logging
   */
  static serializeArgs(args) {
    return args.map((arg) => _ConsoleUtils.serializeArg(arg)).join(" ");
  }
  /**
   * Serialize a single argument for logging
   */
  static serializeArg(arg) {
    if (arg === null) return "null";
    if (arg === void 0) return "undefined";
    if (typeof arg === "string") return arg;
    if (typeof arg === "number" || typeof arg === "boolean") return String(arg);
    if (typeof arg === "function") return `[Function: ${arg.name || "anonymous"}]`;
    if (arg instanceof Error) return `${arg.name}: ${arg.message}`;
    try {
      return JSON.stringify(arg, null, 2);
    } catch {
      return String(arg);
    }
  }
  /**
   * Create a log entry from console arguments
   */
  static createLogEntry(level, args, context, source = "browser-console") {
    const entry = {
      level,
      message: _ConsoleUtils.serializeArgs(args),
      timestamp: Date.now(),
      sessionId: context.sessionId,
      source,
      context
    };
    if (args.length > 0) {
      entry.data = { args };
    }
    return entry;
  }
  /**
   * Convert between old Console.Level and new LogLevel
   */
  static normalizeLogLevel(level) {
    switch (level.toLowerCase()) {
      case "error":
        return "error";
      case "warn":
        return "warn";
      case "info":
        return "info";
      case "log":
        return "log";
      case "debug":
        return "debug";
      case "trace":
        return "trace";
      case "probe":
        return "probe";
      default:
        return "log";
    }
  }
};

// src/daemons/logger/shared/LoggerMessageTypes.ts
function generateUUID2() {
  if (typeof crypto !== "undefined" && crypto.randomUUID) {
    return crypto.randomUUID();
  } else {
    return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, function(c) {
      const r = Math.random() * 16 | 0;
      const v = c === "x" ? r : r & 3 | 8;
      return v.toString(16);
    });
  }
}
var LoggerMessageFactory = class {
  static createLogMessage(logEntryOrFrom, to, logEntry, priority = "normal") {
    if (typeof logEntryOrFrom === "object" && !to) {
      return {
        type: "log",
        payload: logEntryOrFrom
      };
    }
    return {
      id: generateUUID2(),
      from: logEntryOrFrom,
      to,
      type: "log",
      data: {
        type: "log",
        payload: logEntry
      },
      timestamp: /* @__PURE__ */ new Date(),
      priority
    };
  }
  static createFlushMessage(from, to, flushRequest = {}) {
    return {
      id: generateUUID2(),
      from,
      to,
      type: "flush",
      data: {
        type: "flush",
        payload: flushRequest
      },
      timestamp: /* @__PURE__ */ new Date(),
      priority: "high"
    };
  }
  static createRotateMessage(from, to, rotateRequest = {}) {
    return {
      id: generateUUID2(),
      from,
      to,
      type: "rotate",
      data: {
        type: "rotate",
        payload: rotateRequest
      },
      timestamp: /* @__PURE__ */ new Date(),
      priority: "normal"
    };
  }
  static createConfigureMessage(from, to, configureRequest) {
    return {
      id: generateUUID2(),
      from,
      to,
      type: "configure",
      data: {
        type: "configure",
        payload: configureRequest
      },
      timestamp: /* @__PURE__ */ new Date(),
      priority: "high"
    };
  }
};

// src/daemons/logger/client/ClientLoggerDaemon.ts
var ClientLoggerDaemon = class {
  constructor(context) {
    this.webSocketTransport = null;
    this.context = context;
  }
  /**
   * Set WebSocket transport for server communication
   */
  setWebSocketTransport(transport) {
    this.webSocketTransport = transport;
  }
  /**
   * Process a log message (used by ClientLoggerClient)
   */
  async processLogMessage(loggerMessage) {
    if (this.webSocketTransport && this.webSocketTransport.isConnected()) {
      await this.webSocketTransport.send(loggerMessage);
    }
    this.logToBrowserConsole(loggerMessage);
  }
  // Serialization methods removed - now using unified ConsoleUtils
  /**
   * Log to browser console (fallback/local logging)
   * DISABLED: Let server-side forwarding handle all console output with proper stack traces
   */
  logToBrowserConsole(_loggerMessage) {
    return;
  }
  /**
   * Clean up (console methods are now managed by ClientConsoleManager)
   */
  cleanup() {
    this.webSocketTransport = null;
  }
};

// src/daemons/logger/client/ClientLoggerClient.ts
var ClientLoggerClient = class _ClientLoggerClient {
  constructor() {
    this.clientLoggerDaemon = null;
  }
  static getInstance() {
    if (!_ClientLoggerClient.instance) {
      _ClientLoggerClient.instance = new _ClientLoggerClient();
    }
    return _ClientLoggerClient.instance;
  }
  /**
   * Initialize daemon with context
   */
  initialize(context) {
    if (this.clientLoggerDaemon) {
      return;
    }
    this.clientLoggerDaemon = new ClientLoggerDaemon(context);
  }
  /**
   * Set WebSocket transport for server communication
   */
  setWebSocketTransport(transport) {
    if (!this.clientLoggerDaemon) {
      throw new Error("ClientLoggerClient not initialized");
    }
    this.clientLoggerDaemon.setWebSocketTransport(transport);
  }
  /**
   * Log a message directly (bypasses console overrides)
   */
  async log(level, message2, data2) {
    if (!this.clientLoggerDaemon) {
      console.warn("ClientLoggerClient not initialized, using console fallback");
      console[level](message2, data2);
      return;
    }
    const entry = {
      level,
      message: message2,
      timestamp: Date.now(),
      sessionId: this.clientLoggerDaemon.context.sessionId,
      source: "client-direct",
      context: this.clientLoggerDaemon.context
    };
    if (data2) {
      entry.data = data2;
    }
    const logMessage = LoggerMessageFactory.createLogMessage(entry);
    try {
      await this.clientLoggerDaemon.processLogMessage(logMessage);
    } catch (error) {
      console.error("Failed to process log message:", error);
    }
  }
  /**
   * Convenience methods for different log levels
   */
  async info(message2, data2) {
    return this.log("info", message2, data2);
  }
  async warn(message2, data2) {
    return this.log("warn", message2, data2);
  }
  async error(message2, data2) {
    return this.log("error", message2, data2);
  }
  async debug(message2, data2) {
    return this.log("debug", message2, data2);
  }
  /**
   * Check if daemon is initialized
   */
  isInitialized() {
    return this.clientLoggerDaemon !== null;
  }
  /**
   * Clean up
   */
  destroy() {
    if (this.clientLoggerDaemon) {
      this.clientLoggerDaemon.cleanup();
      this.clientLoggerDaemon = null;
    }
  }
};
var clientLoggerClient = ClientLoggerClient.getInstance();

// src/daemons/logger/client/ClientConsoleManager.ts
var ClientConsoleManager = class {
  constructor(getState, getSessionId) {
    this.getState = getState;
    this.getSessionId = getSessionId;
    this.consoleForwarding = false;
    this.consoleMessageQueue = [];
    this.originalConsole = {};
    const sessionId = this.getSessionId();
    if (!sessionId) {
      throw new Error("ClientConsoleManager requires a valid session ID");
    }
    this.context = continuumContextFactory.create({ sessionId });
    this.initializeLoggerDaemon();
    this.enableConsoleForwarding();
  }
  initializeLoggerDaemon() {
    clientLoggerClient.initialize(this.context);
  }
  setExecuteCallback(callback) {
    this.executeCallback = callback;
  }
  setWebSocketTransport(transport) {
    clientLoggerClient.setWebSocketTransport(transport);
  }
  enableConsoleForwarding() {
    if (this.consoleForwarding) {
      console.warn("\u26A0\uFE0F Console forwarding is already enabled, called twice?");
      return;
    }
    this.originalConsole = {
      log: console.log.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      info: console.info.bind(console),
      debug: console.debug ? console.debug.bind(console) : console.log.bind(console),
      trace: console.trace.bind(console)
    };
    console.log("\u{1F504} Console forwarding initialized (no console override)");
    console.probe = (messageOrProbeData, data) => {
      let probeData;
      if (typeof messageOrProbeData === "string") {
        probeData = {
          message: messageOrProbeData,
          category: "ai-diagnostic"
        };
        if (data) {
          probeData.data = data;
        }
      } else {
        probeData = messageOrProbeData;
      }
      if (probeData.executeJS) {
        try {
          const result = eval(probeData.executeJS);
          let processedResult = result;
          if (typeof result === "string") {
            try {
              processedResult = JSON.parse(result);
            } catch (error) {
              processedResult = result;
            }
          }
          probeData.data = {
            ...probeData.data,
            executeJSResult: processedResult,
            jsCode: probeData.executeJS
            // Keep original for local logging
          };
        } catch (error) {
          probeData.data = {
            ...probeData.data,
            executeJSError: error instanceof Error ? error.message : String(error),
            jsCode: probeData.executeJS
            // Keep original for local logging
          };
        }
        probeData.executeJSBase64 = btoa(probeData.executeJS);
        delete probeData.executeJS;
      }
      this.originalConsole.log("\u{1F52C} PROBE:", probeData.message, probeData.data || "");
      this.forwardConsole("probe", [probeData]);
    };
    this.consoleForwarding = true;
    console.log("\u2705 Console forwarding enabled (without console override)");
    console.log("\u{1F52C} console.probe() method added for AI diagnostics");
  }
  /**
   * Forward console messages to server (called by probe or manual forwarding)
   */
  forwardConsole(type, args) {
    try {
      const level = type;
      const stackTrace = this.captureCallerStackTrace();
      const logEntry = Console.MessageUtils.createLogEntry(level, args, {
        stackTrace
        // Pass the captured stack trace
      });
      const consoleLogEntry = {
        ...logEntry,
        sessionId: this.context.sessionId
      };
      if (clientLoggerClient.isInitialized()) {
        const message2 = ConsoleUtils.serializeArgs(args);
        clientLoggerClient.log(level, message2, {
          originalArgs: args,
          consoleLogEntry
        }).catch((error) => {
          this.originalConsole.error("Failed to forward to logger daemon:", error);
        });
      }
      if (this.getState() === "ready" && this.executeCallback) {
        this.executeConsoleCommand(consoleLogEntry);
      } else {
        this.queueConsoleMessage(consoleLogEntry);
      }
    } catch (error) {
      this.originalConsole.error("\u274C Error forwarding console message:", error);
    }
  }
  // Serialization methods removed - now using unified ConsoleUtils
  /**
   * Capture stack trace from the actual calling location (not our console forwarding methods)
   */
  captureCallerStackTrace() {
    try {
      const error = new Error();
      if (error.stack) {
        const lines = error.stack.split("\n");
        const filteredLines = lines.filter(
          (line) => (
            // Filter out our own console management methods
            !line.includes("ClientConsoleManager.forwardConsole") && !line.includes("ClientConsoleManager.captureCallerStackTrace") && !line.includes("ClientConsoleManager.") && !line.includes("ClientLoggerDaemon.") && !line.includes("console.log") && !line.includes("console.warn") && !line.includes("console.error") && !line.includes("console.info") && !line.includes("console.debug") && !line.includes("console.trace") && !line.includes("console.probe") && // Filter out the Error constructor and other internal methods
            !line.includes("Error.captureStackTrace") && !line.includes("at new Error") && !line.includes("at Object.") && // Common for arrow functions
            line.trim().length > 0
          )
        );
        return filteredLines.slice(0, 5).join("\n");
      }
    } catch (e) {
    }
    return "";
  }
  queueConsoleMessage(logEntry) {
    this.consoleMessageQueue.push(logEntry);
  }
  executeConsoleCommand(logEntry) {
    if (this.executeCallback) {
      this.executeCallback("console", logEntry).catch((error) => {
        this.originalConsole.error("\u274C Error executing console command:", error);
      });
    }
  }
  /**
   * Process queued messages when ready
   */
  processQueuedMessages() {
    if (this.consoleMessageQueue.length === 0) return;
    const messages = [...this.consoleMessageQueue];
    this.consoleMessageQueue = [];
    for (const message2 of messages) {
      this.executeConsoleCommand(message2);
    }
  }
  /**
   * Get current context
   */
  getContext() {
    return this.context;
  }
  /**
   * Update context (e.g., when session changes)
   */
  updateContext(newContext) {
    this.context = newContext;
    clientLoggerClient.destroy();
    clientLoggerClient.initialize(this.context);
  }
  /**
   * Clean up console forwarding
   */
  destroy() {
    if (this.consoleForwarding) {
      delete console.probe;
      this.consoleForwarding = false;
    }
    clientLoggerClient.destroy();
  }
};

// src/ui/continuum-browser-client/console/ConsoleForwarder.ts
var ConsoleForwarder = class {
  constructor(getState, getSessionId) {
    this.getState = getState;
    this.getSessionId = getSessionId;
    const sessionIdGetter = () => {
      const sessionId = this.getSessionId();
      if (!sessionId) {
        throw new Error("ConsoleForwarder requires a valid session ID");
      }
      return sessionId;
    };
    this.consoleManager = new ClientConsoleManager(this.getState, sessionIdGetter);
  }
  setExecuteCallback(callback) {
    this.consoleManager.setExecuteCallback(callback);
  }
  setWebSocketTransport(transport) {
    this.consoleManager.setWebSocketTransport(transport);
  }
  processQueuedMessages() {
    this.consoleManager.processQueuedMessages();
  }
  getContext() {
    return this.consoleManager.getContext();
  }
  updateContext(newContext) {
    this.consoleManager.updateContext(newContext);
  }
  // Legacy methods for backward compatibility
  executeAndFlushConsoleMessageQueue() {
    this.processQueuedMessages();
  }
  performHealthCheck() {
    return this.consoleManager.getContext() !== null;
  }
  destroy() {
    this.consoleManager.destroy();
  }
};

// src/ui/continuum-browser-client/connection/WebSocketManager.ts
var WebSocketManager = class {
  constructor(version) {
    this.ws = null;
    this.version = version;
  }
  setCallbacks(callbacks) {
    this.onStateChange = callbacks.onStateChange;
    this.onClientId = callbacks.onClientId;
    this.onSessionId = callbacks.onSessionId;
    this.onMessage = callbacks.onMessage;
  }
  initializeConnection() {
    try {
      this.ws = new WebSocket("ws://localhost:9000");
      this.ws.onopen = () => {
        console.log("\u{1F50C} WebSocket connected");
        this.onStateChange?.("connected");
        const clientInitData = {
          userAgent: navigator.userAgent,
          url: window.location.href,
          timestamp: (/* @__PURE__ */ new Date()).toISOString(),
          version: this.version,
          mode: "join_existing"
        };
        this.sendMessage({
          type: "client_init",
          data: clientInitData,
          timestamp: (/* @__PURE__ */ new Date()).toISOString()
        });
      };
      this.ws.onmessage = (event2) => {
        this.handleMessage(event2);
      };
      this.ws.onclose = () => {
        console.log("\u{1F50C} WebSocket disconnected");
        this.onStateChange?.("error");
      };
      this.ws.onerror = (error) => {
        console.error("\u{1F50C} WebSocket error:", error);
        this.onStateChange?.("error");
      };
    } catch (error) {
      console.error("\u274C Failed to initialize WebSocket:", error);
      this.onStateChange?.("error");
    }
  }
  async handleMessage(event) {
    try {
      const message = JSON.parse(event.data);
      if (message.type === "connection_confirmed") {
        const clientId = message.data?.clientId;
        if (clientId) {
          console.log(`\u{1F50C} Client ID: ${clientId}`);
          this.onClientId?.(clientId);
        }
        return;
      }
      if (message.type === "session_ready") {
        const sessionId = message.data?.sessionId;
        if (sessionId) {
          console.log(`\u{1F3AF} Session: ${sessionId}`);
          this.onSessionId?.(sessionId);
          this.onStateChange?.("ready");
        }
        return;
      }
      if (message.type === "execute_command_response") {
        const event2 = new CustomEvent("continuum:command_response", {
          detail: message
        });
        document.dispatchEvent(event2);
        return;
      }
      if (message.type === "remote_execution_request") {
        await this.delegateRemoteExecution(message.data);
        return;
      }
      if (message.type === "execute_js") {
        console.log("\u{1F680} Executing JavaScript from server:", message.script);
        try {
          const wrappedScript = `(async function() { ${message.script} })()`;
          const result = await eval(wrappedScript);
          console.log("\u2705 JavaScript execution result:", result);
          if (message.requestId) {
            this.sendMessage({
              type: "execute_js_result",
              requestId: message.requestId,
              result,
              timestamp: (/* @__PURE__ */ new Date()).toISOString()
            });
          }
        } catch (error) {
          console.error("\u274C JavaScript execution error:", error);
          if (message.requestId) {
            this.sendMessage({
              type: "execute_js_result",
              requestId: message.requestId,
              error: error instanceof Error ? error.message : String(error),
              timestamp: (/* @__PURE__ */ new Date()).toISOString()
            });
          }
        }
        return;
      }
      this.onMessage?.(message);
    } catch (error) {
      console.error("\u274C Error parsing message:", error);
    }
  }
  sendMessage(message2) {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({
        ...message2,
        timestamp: (/* @__PURE__ */ new Date()).toISOString()
      }));
    }
  }
  isOpen() {
    return this.ws?.readyState === WebSocket.OPEN;
  }
  /**
   * Delegate remote execution to proper command handler - modular approach
   */
  async delegateRemoteExecution(data2) {
    const startTime = Date.now();
    console.log(`\u{1F50D} Browser delegating remote execution: ${data2.command}`, data2);
    try {
      const event2 = new CustomEvent("continuum:remote_execution", {
        detail: {
          request: data2,
          respond: (response) => {
            this.sendMessage({
              type: "remote_execution_response",
              data: response
            });
          }
        }
      });
      document.dispatchEvent(event2);
      setTimeout(() => {
        const errorResponse = {
          success: false,
          error: `No handler registered for command: ${data2.command}`,
          requestId: data2.requestId,
          clientMetadata: {
            userAgent: navigator.userAgent,
            timestamp: Date.now(),
            executionTime: Date.now() - startTime
          }
        };
        this.sendMessage({
          type: "remote_execution_response",
          data: errorResponse,
          timestamp: (/* @__PURE__ */ new Date()).toISOString()
        });
      }, 1e3);
    } catch (error) {
      console.error(`\u274C Remote execution delegation failed: ${error}`);
      const response = {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        requestId: data2.requestId,
        clientMetadata: {
          userAgent: navigator.userAgent,
          timestamp: Date.now(),
          executionTime: Date.now() - startTime
        }
      };
      this.sendMessage({
        type: "remote_execution_response",
        data: response
      });
    }
  }
};

// src/parsers/shared/ParserBase.ts
var ParserBase = class {
  /**
   * Common utility for safe JSON parsing
   */
  safeJsonParse(input) {
    try {
      return JSON.parse(input);
    } catch (error) {
      return null;
    }
  }
  /**
   * Common utility for extracting command name from various input formats
   */
  extractCommandName(input) {
    if (typeof input === "object" && input !== null) {
      return input.command || input.commandName || null;
    }
    return null;
  }
  /**
   * Common utility for parameter validation
   */
  validateParameters(params2, required) {
    const missing = required.filter((key) => !(key in params2) || params2[key] === void 0);
    if (missing.length > 0) {
      return {
        success: false,
        error: `Missing required parameters: ${missing.join(", ")}`,
        details: { missingParameters: missing }
      };
    }
    return { success: true };
  }
  /**
   * Common utility for output formatting status display
   */
  getStatusDisplay(success) {
    return success ? { icon: "\u2705", text: "Success" } : { icon: "\u274C", text: "Failed" };
  }
  /**
   * Common utility for formatting file sizes
   */
  formatFileSize(bytes) {
    if (bytes < 1024) return `${bytes}B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)}GB`;
  }
  /**
   * Common utility for formatting execution time
   */
  formatExecutionTime(ms) {
    if (ms < 1e3) return `${ms}ms`;
    if (ms < 6e4) return `${(ms / 1e3).toFixed(1)}s`;
    return `${(ms / 6e4).toFixed(1)}m`;
  }
};

// src/parsers/integrations/cli-parser/shared/CLIParserTypes.ts
var DEFAULT_CLI_FORMATTING = {
  showTimestamp: false,
  showExecutionTime: true,
  showStatusIcon: true,
  compactMode: false
};

// src/parsers/integrations/cli-parser/client/CLIClientParser.ts
var CLIClientParser = class extends ParserBase {
  constructor(config = {}, formatting = {}) {
    super();
    config;
    this.formatting = { ...DEFAULT_CLI_FORMATTING, ...formatting };
  }
  /**
   * Parse CLI arguments into universal parameters
   */
  parseInput(input) {
    const { args } = input;
    const params2 = {};
    for (let i = 0; i < args.length; i++) {
      const arg = args[i];
      if (arg.startsWith("--") && arg.includes("=")) {
        const [key, ...valueParts] = arg.substring(2).split("=");
        params2[key] = valueParts.join("=");
      } else if (arg.startsWith("--") && i + 1 < args.length && !args[i + 1].startsWith("--")) {
        const key = arg.substring(2);
        params2[key] = args[i + 1];
        i++;
      } else if (arg.startsWith("--")) {
        const key = arg.substring(2);
        params2[key] = true;
      } else {
        if (!params2.args) params2.args = [];
        params2.args.push(arg);
      }
    }
    return params2;
  }
  /**
   * Format command output for CLI display
   */
  formatOutput(output, command) {
    switch (command) {
      case "screenshot":
        this.formatScreenshotOutput(output);
        break;
      case "chat":
        this.formatChatOutput(output);
        break;
      case "help":
        this.formatHelpOutput(output);
        break;
      default:
        this.formatGenericOutput(output, command);
    }
  }
  /**
   * Format screenshot command output
   */
  formatScreenshotOutput(output) {
    const { success, data: data2 } = output;
    const status = this.getStatusDisplay(success);
    console.log(`
\u{1F4F8} SCREENSHOT CAPTURED`);
    console.log(`${status.icon} Status: ${status.text}`);
    if (success && data2) {
      if (data2.broadcastSent) {
        console.log(`\u{1F4E1} Broadcast sent to ${data2.connectionCount} browser connection(s)`);
        console.log(`\u{1F4BE} Screenshot saved to session directory`);
        console.log(`\u{1F4C1} Check your screenshots folder for the captured image`);
      } else if (data2.filename) {
        console.log(`\u{1F4C1} File: ${data2.filename}`);
        if (data2.filePath) console.log(`\u{1F4BE} Saved to: ${data2.filePath}`);
        if (data2.width && data2.height) console.log(`\u{1F4CF} Dimensions: ${data2.width}x${data2.height}px`);
      }
    }
    if (output.executionTime && this.formatting.showExecutionTime) {
      console.log(`\u23F1\uFE0F Execution time: ${this.formatExecutionTime(output.executionTime)}`);
    }
    console.log(`
\u{1F916} AI-Friendly: Full JSON data available programmatically`);
  }
  /**
   * Format chat command output
   */
  formatChatOutput(output) {
    const { success, data: data2 } = output;
    if (success && data2) {
      if (data2.response) {
        console.log(data2.response);
      } else if (data2.message) {
        console.log(data2.message);
      }
    }
  }
  /**
   * Format help command output
   */
  formatHelpOutput(output) {
    const { success, data: data2 } = output;
    if (success && data2) {
      if (data2.commands) {
        console.log("\n\u{1F310} CONTINUUM COMMANDS\n");
        Object.entries(data2.commands).forEach(([category, commands]) => {
          console.log(`\u{1F4CB} ${category.toUpperCase()}:`);
          commands.forEach((cmd) => {
            console.log(`  continuum ${cmd}`);
          });
          console.log("");
        });
      }
    }
  }
  /**
   * Format generic command output
   */
  formatGenericOutput(output, command) {
    const { success, data: data2, error } = output;
    const status = this.getStatusDisplay(success);
    console.log(`
\u26A1 ${command.toUpperCase()} RESULT`);
    console.log(`${status.icon} Status: ${status.text}`);
    if (success && data2) {
      if (typeof data2 === "string") {
        console.log(data2);
      } else {
        console.log(JSON.stringify(data2, null, 2));
      }
    }
    if (error) {
      console.error(`\u274C Error: ${error}`);
    }
    if (output.executionTime && this.formatting.showExecutionTime) {
      console.log(`\u23F1\uFE0F Execution time: ${this.formatExecutionTime(output.executionTime)}`);
    }
  }
  /**
   * Validate CLI input format
   */
  validateInput(input) {
    if (!input || typeof input !== "object") {
      return { success: false, error: "Invalid input format" };
    }
    if (!Array.isArray(input.args)) {
      return { success: false, error: "Args must be an array" };
    }
    return { success: true };
  }
  /**
   * Validate CLI output format
   */
  validateOutput(output) {
    if (!output || typeof output !== "object") {
      return { success: false, error: "Invalid output format" };
    }
    if (typeof output.success !== "boolean") {
      return { success: false, error: "Success field must be boolean" };
    }
    return { success: true };
  }
  /**
   * Get parser information
   */
  getParserInfo() {
    return {
      name: "CLI Client Parser",
      version: "1.0.0",
      description: "Parses CLI arguments and formats output for command-line display",
      supportedCommands: ["*"]
      // Supports all commands
    };
  }
  /**
   * Check if parser can handle input
   */
  canHandle(input) {
    return input && typeof input === "object" && Array.isArray(input.args);
  }
  /**
   * Parser priority for selection
   */
  getPriority() {
    return 100;
  }
};

// src/commands/core/base-command/parsers/IntegrationParser.ts
var IntegrationParserRegistry = class {
  static register(parser) {
    this.parsers.push(parser);
    this.parsers.sort((a, b) => (b.priority || 0) - (a.priority || 0));
  }
  static getAll() {
    return [...this.parsers];
  }
  static parse(params2) {
    for (const parser of this.parsers) {
      if (parser.canHandle(params2)) {
        return parser.parse(params2);
      }
    }
    return params2;
  }
};
IntegrationParserRegistry.parsers = [];

// src/commands/core/base-command/parsers/CLIIntegrationParser.ts
var CLIIntegrationParser = class {
  constructor() {
    this.name = "CLI";
    this.priority = 100;
  }
  // High priority - specific format
  canHandle(params2) {
    return !!(params2 && typeof params2 === "object" && "args" in params2 && Array.isArray(params2.args) && params2.args.length > 0);
  }
  parse(params2) {
    const { args } = params2;
    const result2 = {};
    const positionalArgs = [];
    console.log("\u{1F50D} CLIIntegrationParser.parse - input params:", params2);
    for (let i = 0; i < args.length; i++) {
      const arg = args[i];
      if (typeof arg === "string" && arg.startsWith("--")) {
        if (arg.includes("=")) {
          const [key, ...valueParts] = arg.substring(2).split("=");
          const rawValue = valueParts.join("=");
          result2[key] = this.parseValue(rawValue);
        } else {
          const key = arg.substring(2);
          const nextArg = args[i + 1];
          if (nextArg && !nextArg.startsWith("--")) {
            result2[key] = this.parseValue(nextArg);
            i++;
          } else {
            result2[key] = true;
          }
        }
      } else {
        positionalArgs.push(arg);
      }
    }
    if (positionalArgs.length > 0) {
      result2.command = positionalArgs[0];
      result2.filename = positionalArgs[0];
      result2.target = positionalArgs[0];
      result2.script = positionalArgs[0];
      if (positionalArgs.length > 1) {
        result2.args = positionalArgs;
      }
    }
    console.log("\u{1F50D} CLIIntegrationParser.parse - output result:", result2);
    return result2;
  }
  parseValue(value) {
    if (!value) return true;
    try {
      return JSON.parse(value);
    } catch {
      return value;
    }
  }
};

// src/commands/core/base-command/parsers/JSONIntegrationParser.ts
var JSONWithArgsIntegrationParser = class {
  constructor() {
    this.name = "JSON-with-args";
    this.priority = 90;
  }
  // High priority - specific hybrid format
  canHandle(params2) {
    return !!(params2 && typeof params2 === "object" && "args" in params2 && Array.isArray(params2.args) && params2.args.length === 0);
  }
  parse(params2) {
    const { args, ...cleanParams } = params2;
    return cleanParams;
  }
};
var PureJSONIntegrationParser = class {
  constructor() {
    this.name = "Pure-JSON";
    this.priority = 10;
  }
  // Low priority - catch-all for objects
  canHandle(params2) {
    return !!(params2 && typeof params2 === "object");
  }
  parse(params2) {
    return params2;
  }
};
var StringJSONIntegrationParser = class {
  constructor() {
    this.name = "String-JSON";
    this.priority = 80;
  }
  // High priority - specific string format
  canHandle(params2) {
    return typeof params2 === "string";
  }
  parse(params2) {
    try {
      return JSON.parse(params2);
    } catch {
      return { data: params2 };
    }
  }
};

// src/commands/core/base-command/parsers/PersonaMeshParser.ts
var PersonaMeshParser = class {
  constructor() {
    this.name = "Persona-Mesh";
    this.priority = 90;
  }
  // High priority for AI collaboration
  canHandle(params2) {
    return params2 !== null && typeof params2 === "object" && "persona" in params2 && "intent" in params2 && "action" in params2;
  }
  parse(params2) {
    const mesh = params2;
    const result2 = {
      ...mesh.action,
      _personaContext: {
        persona: mesh.persona,
        intent: mesh.intent,
        context: mesh.context,
        collaboration: mesh.collaboration
      }
    };
    if (mesh.collaboration?.chainId) {
      result2._collaborationChain = {
        id: mesh.collaboration.chainId,
        dependencies: mesh.collaboration.dependencies || [],
        urgency: mesh.collaboration.urgency || "medium"
      };
    }
    return result2;
  }
};

// src/commands/core/base-command/parsers/MCPIntegrationParser.ts
var MCPIntegrationParser = class {
  constructor() {
    this.name = "MCP";
    this.priority = 85;
  }
  // High priority for MCP ecosystem
  canHandle(params2) {
    return params2 !== null && typeof params2 === "object" && "jsonrpc" in params2 && "method" in params2 && params2.jsonrpc === "2.0";
  }
  parse(params2) {
    const mcpRequest = params2;
    const result2 = this.translateMCPMethod(mcpRequest);
    result2._mcpContext = {
      method: mcpRequest.method,
      id: mcpRequest.id,
      jsonrpc: mcpRequest.jsonrpc
    };
    return result2;
  }
  translateMCPMethod(request) {
    const { method, params: params2 } = request;
    switch (method) {
      case "tools/list":
        return { command: "help", format: "tools" };
      case "tools/call":
        const toolCall = params2;
        return {
          command: toolCall.name,
          ...toolCall.arguments
        };
      case "resources/list":
        return { command: "projects", format: "resources" };
      case "prompts/list":
        return { command: "personas", format: "prompts" };
      case "prompts/get":
        return {
          command: "persona",
          action: "get",
          name: params2?.name
        };
      case "resources/read":
        return {
          command: "data-marshal",
          operation: "read",
          path: params2?.uri
        };
      case "completion/complete":
        return {
          command: "chat",
          message: params2?.prompt,
          context: params2?.argument?.context
        };
      default:
        const commandMatch = method.match(/(\w+)\/(\w+)/);
        if (commandMatch) {
          return {
            command: commandMatch[2],
            service: commandMatch[1],
            ...params2
          };
        }
        return {
          command: method,
          ...params2
        };
    }
  }
};

// src/commands/core/base-command/parsers/index.ts
IntegrationParserRegistry.register(new CLIIntegrationParser());
IntegrationParserRegistry.register(new PersonaMeshParser());
IntegrationParserRegistry.register(new MCPIntegrationParser());
IntegrationParserRegistry.register(new JSONWithArgsIntegrationParser());
IntegrationParserRegistry.register(new StringJSONIntegrationParser());
IntegrationParserRegistry.register(new PureJSONIntegrationParser());

// src/commands/core/base-command/BaseCommand.ts
var BaseCommand = class _BaseCommand {
  /**
   * Get command definition - must be implemented by subclasses
   */
  static getDefinition() {
    throw new Error("getDefinition() must be implemented by subclass");
  }
  /**
   * Execute command - implement this in subclasses with typed parameters
   * Parameters are automatically parsed and validated before calling this method
   * 
   * @param params - Raw parameters (will be validated and cast to TParams)
   * @param context - Execution context
   * @returns Promise resolving to command result
   */
  static async execute(_params2, _context) {
    throw new Error("execute() must be implemented by subclass with typed parameters");
  }
  // Note: validateParams method removed to avoid conflicts with existing file command implementations
  // Commands should use the inline pattern demonstrated in ExecCommand instead
  // Note: getTypedParams method removed due to static inheritance limitations
  // Commands should use the inline pattern demonstrated in ExecCommand instead
  /**
   * Generic typed execution pattern - eliminates 'any' parameters
   * Use this in your execute method for automatic type safety and error handling
   * 
   * @param rawParams - Raw parameters from command call
   * @param context - Execution context
   * @param validator - Type guard function that validates and throws descriptive errors
   * @param executor - Your typed execution function
   * @returns Promise resolving to command result with automatic error handling
   */
  static async executeWithStrongTypes(rawParams, context, validator, executor) {
    try {
      const parsedParams = _BaseCommand.preprocessParameters(rawParams);
      validator(parsedParams);
      const typedParams = parsedParams;
      return await executor(typedParams, context);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage,
        timestamp: (/* @__PURE__ */ new Date()).toISOString()
      };
    }
  }
  /**
   * Parse CLI arguments using the shared CLI parser
   * Public method available to all commands for consistent CLI parsing
   */
  static parseCliArguments(params2) {
    if (params2.args && Array.isArray(params2.args)) {
      const cliParser = new CLIClientParser();
      const parsedArgs = cliParser.parseInput({ args: params2.args });
      return { ...params2, ...parsedArgs };
    }
    return params2;
  }
  /**
   * Automatic parameter preprocessing for clean command pattern
   * Handles CLI parsing transparently so commands don't need to know about it
   */
  static preprocessParameters(parameters) {
    if (typeof parameters !== "object" || parameters === null) {
      throw new Error("Parameters must be a non-null object");
    }
    return _BaseCommand.parseCliArguments(parameters);
  }
  /**
   * Parse parameters using modular integration parser system
   * Any format to BaseCommand's canonical JSON format
   * 
   * ⚠️  INTERNAL USE ONLY: Should only be called by UniversalCommandRegistry
   * ⚠️  Individual commands should NOT call this - parameters are pre-parsed
   */
  static _registryParseParams(params2) {
    return IntegrationParserRegistry.parse(params2);
  }
  /**
   * Get typed parameters - ensures parameters are pre-parsed by registry
   * 
   * @param params - Parameters that should already be parsed by UniversalCommandRegistry
   * @returns Typed parameters with runtime validation that they're pre-parsed
   */
  static args(params2) {
    if (typeof params2 === "object" && params2 !== null && "args" in params2) {
      console.warn("\u26A0\uFE0F  CLI args detected - parameters should be pre-parsed by registry");
      console.warn("\u26A0\uFE0F  This suggests UniversalCommandRegistry is not parsing properly");
    }
    return params2;
  }
  /**
   * @deprecated Do not call parseParams in commands - parameters are pre-parsed by registry
   * Use args<T>() instead for type-safe access to pre-parsed parameters
   */
  static parseParams(params2) {
    console.warn("\u26A0\uFE0F  parseParams called in command - parameters should be pre-parsed by registry");
    console.warn("\u26A0\uFE0F  Use args<T>() instead for type-safe access to pre-parsed parameters");
    return IntegrationParserRegistry.parse(params2);
  }
  /**
   * Create standardized success result
   * Legacy signature: createSuccessResult(message, data) or createSuccessResult(data)
   */
  static createSuccessResult(messageOrData, data2) {
    const result2 = {
      success: true,
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
    if (typeof messageOrData === "string" && data2 !== void 0) {
      result2.data = data2;
    } else if (messageOrData !== void 0) {
      result2.data = messageOrData;
    }
    return result2;
  }
  /**
   * Create standardized error result
   */
  static createErrorResult(error, _data) {
    return {
      success: false,
      error,
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    };
  }
  /**
   * Validate required parameters
   */
  static validateRequired(params2, requiredFields) {
    const missing = [];
    for (const field of requiredFields) {
      if (params2[field] === void 0 || params2[field] === null) {
        missing.push(field);
      }
    }
    return {
      valid: missing.length === 0,
      missing
    };
  }
  /**
   * Log command execution with consistent format
   */
  static logExecution(commandName, params2, context) {
    const sessionInfo = context.sessionId ? ` [${context.sessionId}]` : "";
    console.log(`\u{1F3AF} COMMAND: ${commandName}${sessionInfo} - params:`, params2);
  }
  /**
   * Broadcast message to WebSocket clients if available
   */
  static async broadcast(context, message2) {
    if (context.webSocketServer && typeof context.webSocketServer.broadcast === "function") {
      try {
        const messageStr = typeof message2 === "string" ? message2 : JSON.stringify(message2);
        context.webSocketServer.broadcast(messageStr);
      } catch (error) {
        console.error("Failed to broadcast message:", error);
      }
    }
  }
  /**
   * Update continuon status if available
   */
  static async updateStatus(context, status, data2) {
    if (context?.continuumStatus && typeof context.continuumStatus.update === "function") {
      try {
        context.continuumStatus.update(status, data2);
      } catch (error) {
        console.error("Failed to update continuum status:", error);
      }
    }
  }
  /**
   * Create command registry entry
   */
  static createRegistryEntry() {
    const definition = this.getDefinition();
    return {
      name: definition.name.toUpperCase(),
      execute: this.execute.bind(this),
      definition
    };
  }
};

// src/commands/file/shared/FileTypes.ts
var ArtifactType = /* @__PURE__ */ ((ArtifactType2) => {
  ArtifactType2["SCREENSHOT"] = "screenshot";
  ArtifactType2["LOG"] = "log";
  ArtifactType2["RECORDING"] = "recording";
  ArtifactType2["FILE"] = "file";
  ArtifactType2["DEVTOOLS"] = "devtools";
  ArtifactType2["METADATA"] = "metadata";
  return ArtifactType2;
})(ArtifactType || {});

// src/commands/file/shared/BaseFileCommand.ts
var pathUtils = {
  join: (...parts) => {
    return parts.join("/").replace(/\/+/g, "/");
  },
  dirname: (filePath) => {
    const parts = filePath.split("/");
    return parts.slice(0, -1).join("/") || "/";
  }
};
var FileSystemOperation = /* @__PURE__ */ ((FileSystemOperation2) => {
  FileSystemOperation2["READ_DIRECTORY"] = "read_directory";
  FileSystemOperation2["CREATE_DIRECTORY"] = "create_directory";
  FileSystemOperation2["WRITE_FILE"] = "write_file";
  FileSystemOperation2["READ_FILE"] = "read_file";
  FileSystemOperation2["APPEND_FILE"] = "append_file";
  FileSystemOperation2["DELETE_FILE"] = "delete_file";
  FileSystemOperation2["GET_FILE_STATS"] = "get_file_stats";
  FileSystemOperation2["CHECK_FILE_ACCESS"] = "check_file_access";
  return FileSystemOperation2;
})(FileSystemOperation || {});
var DirectoryOperation = /* @__PURE__ */ ((DirectoryOperation2) => {
  DirectoryOperation2["GET_ROOT_DIRECTORY"] = "get_root_directory";
  DirectoryOperation2["GET_SESSION_DIRECTORY"] = "get_session_directory";
  DirectoryOperation2["CREATE_SESSION_STRUCTURE"] = "create_session_structure";
  DirectoryOperation2["GET_ARTIFACT_LOCATION"] = "get_artifact_location";
  return DirectoryOperation2;
})(DirectoryOperation || {});
var BaseFileCommand = class extends BaseCommand {
  /**
   * Get .continuum root directory from ContinuumDirectoryDaemon
   */
  static async getContinuumRoot() {
    try {
      const response = await this.delegateToContinuumDirectoryDaemon("get_root_directory" /* GET_ROOT_DIRECTORY */, {});
      return response.rootPath;
    } catch (error) {
      console.warn("ContinuumDirectoryDaemon delegation failed, using fallback:", error);
      const workingDir = typeof process !== "undefined" ? process.cwd() : "";
      return pathUtils.join(workingDir, ".continuum");
    }
  }
  /**
   * DEPRECATED: Complex findSessionPath logic replaced with simple session structure
   * Use getTargetPath() instead which uses predictable session paths
   */
  static async findSessionPath(sessionId) {
    console.warn("findSessionPath is deprecated. Use getTargetPath() for predictable session structure.");
    const continuumRoot = await this.getContinuumRoot();
    return pathUtils.join(continuumRoot, "sessions", "user", "shared", sessionId);
  }
  /**
   * Get artifact subdirectory name following daemon conventions
   * Now uses shared ArtifactType enum for consistency
   */
  static getArtifactSubdirectory(artifactType) {
    switch (artifactType) {
      case "screenshot" /* SCREENSHOT */:
      case "screenshot":
        return "screenshots";
      case "log" /* LOG */:
      case "log":
        return "logs";
      case "recording" /* RECORDING */:
      case "recording":
        return "recordings";
      case "file" /* FILE */:
      case "file":
        return "files";
      case "devtools" /* DEVTOOLS */:
      case "devtools":
        return "devtools";
      case "metadata" /* METADATA */:
      case "metadata":
        return "metadata";
      default:
        return "files";
    }
  }
  /**
   * Ensure directory exists using ContinuumFileSystemDaemon
   */
  static async ensureDirectoryExists(dirPath) {
    try {
      await this.delegateToContinuumFileSystemDaemon("create_directory" /* CREATE_DIRECTORY */, {
        path: dirPath,
        recursive: true
      });
    } catch (error) {
      throw new Error(`Failed to create directory ${dirPath}: ${error}`);
    }
  }
  /**
   * Get target path for file operation using simple session context-based path resolution
   * Replaces complex findSessionPath logic with predictable session structure
   */
  static async getTargetPath(params2) {
    if (params2.directory) {
      return pathUtils.join(params2.directory, params2.filename);
    }
    const continuumRoot = await this.getContinuumRoot();
    if (!params2.sessionId) {
      const defaultDir = params2.artifactType ? pathUtils.join(continuumRoot, this.getArtifactSubdirectory(params2.artifactType)) : pathUtils.join(continuumRoot, "files");
      return pathUtils.join(defaultDir, params2.filename);
    }
    const sessionPath = pathUtils.join(continuumRoot, "sessions", "user", "shared", params2.sessionId);
    if (params2.artifactType) {
      const artifactSubdir = this.getArtifactSubdirectory(params2.artifactType);
      return pathUtils.join(sessionPath, artifactSubdir, params2.filename);
    }
    return pathUtils.join(sessionPath, params2.filename);
  }
  /**
   * Log file operation for debugging and forensics
   */
  static async logFileOperation(operation2, targetPath, metadata = {}) {
    try {
      const continuumRoot = await this.getContinuumRoot();
      const logDir = pathUtils.join(continuumRoot, "logs");
      await this.ensureDirectoryExists(logDir);
      const logEntry = {
        timestamp: (/* @__PURE__ */ new Date()).toISOString(),
        operation: operation2,
        path: targetPath,
        ...metadata
      };
      const logFile = pathUtils.join(logDir, "file-operations.log");
      const logLine = JSON.stringify(logEntry) + "\n";
      await this.delegateToContinuumFileSystemDaemon("append_file" /* APPEND_FILE */, {
        path: logFile,
        content: logLine,
        encoding: "utf8"
      });
    } catch {
    }
  }
  /**
   * Get file stats safely
   */
  static async getFileStats(filePath) {
    try {
      return await this.delegateToContinuumFileSystemDaemon("get_file_stats" /* GET_FILE_STATS */, { path: filePath });
    } catch {
      return null;
    }
  }
  /**
   * Check if file exists
   */
  static async fileExists(filePath) {
    try {
      await this.delegateToContinuumFileSystemDaemon("check_file_access" /* CHECK_FILE_ACCESS */, { path: filePath });
      return true;
    } catch {
      return false;
    }
  }
  /**
   * Delegate filesystem operations to ContinuumFileSystemDaemon
   * All file commands use this pattern for actual filesystem operations
   */
  static async delegateToContinuumFileSystemDaemon(operation2, params2) {
    try {
      const daemonMessage = {
        type: "daemon_request",
        target: "continuum-filesystem",
        operation: operation2,
        params: params2,
        requestId: `file_${operation2}_${Date.now()}`,
        timestamp: Date.now()
      };
      const response = await this.sendDaemonMessage(daemonMessage);
      if (response.success) {
        return response.data;
      } else {
        throw new Error(response.error || `Filesystem ${operation2} failed`);
      }
    } catch (error) {
      console.warn(`ContinuumFileSystemDaemon ${operation2} failed, using fallback:`, error);
      return this.fallbackFileOperation(operation2, params2);
    }
  }
  /**
   * Delegate directory operations to ContinuumDirectoryDaemon
   */
  static async delegateToContinuumDirectoryDaemon(operation2, params2) {
    try {
      const daemonMessage = {
        type: "daemon_request",
        target: "continuum-directory",
        operation: operation2,
        params: params2,
        requestId: `dir_${operation2}_${Date.now()}`,
        timestamp: Date.now()
      };
      const response = await this.sendDaemonMessage(daemonMessage);
      if (response.success) {
        return response.data;
      } else {
        throw new Error(response.error || `Directory ${operation2} failed`);
      }
    } catch (error) {
      console.warn(`ContinuumDirectoryDaemon ${operation2} failed, using fallback:`, error);
      return this.fallbackDirectoryOperation(operation2, params2);
    }
  }
  /**
   * Send message via internal daemon message bus (not WebSocket ports)
   * Commands communicate with daemons through the CommandProcessorDaemon bus
   */
  static async sendDaemonMessage(_message) {
    return new Promise((resolve) => {
      setTimeout(() => {
        resolve({
          success: false,
          error: "Internal daemon bus communication not yet implemented",
          mockMode: true
        });
      }, 10);
    });
  }
  /**
   * Fallback file operations for development mode
   */
  static async fallbackFileOperation(operation, params) {
    if (typeof window !== "undefined") {
      throw new Error("File operations not available in browser environment");
    }
    const fs = await eval('import("fs/promises")');
    switch (operation) {
      case "read_directory" /* READ_DIRECTORY */:
        return await fs.readdir(params.path, { withFileTypes: params.withFileTypes });
      case "create_directory" /* CREATE_DIRECTORY */:
        await fs.mkdir(params.path, { recursive: params.recursive });
        return { success: true };
      case "write_file" /* WRITE_FILE */:
        await fs.writeFile(params.path, params.content, params.encoding ? { encoding: params.encoding } : void 0);
        return { success: true };
      case "append_file" /* APPEND_FILE */:
        await fs.appendFile(params.path, params.content, { encoding: params.encoding });
        return { success: true };
      case "get_file_stats" /* GET_FILE_STATS */:
        return await fs.stat(params.path);
      case "check_file_access" /* CHECK_FILE_ACCESS */:
        await fs.access(params.path);
        return { success: true };
      default:
        throw new Error(`Unknown file operation: ${operation}`);
    }
  }
  /**
   * Fallback directory operations for development mode
   */
  static async fallbackDirectoryOperation(operation, _params) {
    if (typeof window !== "undefined") {
      throw new Error("Directory operations not available in browser environment");
    }
    const fs = await eval('import("fs/promises")');
    switch (operation) {
      case "get_root_directory" /* GET_ROOT_DIRECTORY */:
        const workingDir = process.cwd();
        const rootPath = pathUtils.join(workingDir, ".continuum");
        await fs.mkdir(rootPath, { recursive: true });
        return { rootPath };
      default:
        throw new Error(`Unknown directory operation: ${operation}`);
    }
  }
};

// src/commands/file/shared/FileValidator.ts
var pathUtils2 = {
  extname: (filename) => {
    const lastDot = filename.lastIndexOf(".");
    return lastDot === -1 ? "" : filename.substring(lastDot);
  },
  basename: (filename, ext) => {
    const base = filename.split("/").pop() || filename;
    return ext && base.endsWith(ext) ? base.slice(0, -ext.length) : base;
  }
};
var FILE_CONSTRAINTS = {
  MAX_FILENAME_LENGTH: 255,
  MAX_FILE_SIZE: 50 * 1024 * 1024,
  // 50MB
  ALLOWED_EXTENSIONS: [".png", ".jpg", ".jpeg", ".gif", ".svg", ".txt", ".log", ".json", ".csv", ".md", ".html", ".css", ".js", ".ts"],
  FORBIDDEN_CHARS: ["<", ">", ":", '"', "|", "?", "*", "\0"],
  RESERVED_NAMES: ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"]
};
var FileValidator = class {
  /**
   * Validate file operation parameters
   */
  static validateParams(params2) {
    const errors = [];
    const warnings = [];
    const filenameResult = this.validateFilename(params2.filename);
    errors.push(...filenameResult.errors);
    warnings.push(...filenameResult.warnings);
    const contentResult = this.validateContent(params2.content);
    errors.push(...contentResult.errors);
    warnings.push(...contentResult.warnings);
    if (params2.artifactType) {
      const artifactResult = this.validateArtifactType(params2.artifactType);
      errors.push(...artifactResult.errors);
      warnings.push(...artifactResult.warnings);
    }
    if (params2.encoding) {
      const encodingResult = this.validateEncoding(params2.encoding);
      errors.push(...encodingResult.errors);
      warnings.push(...encodingResult.warnings);
    }
    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }
  /**
   * Validate filename
   */
  static validateFilename(filename) {
    const errors = [];
    const warnings = [];
    if (!filename || typeof filename !== "string") {
      errors.push("Filename is required and must be a string");
      return { valid: false, errors, warnings };
    }
    if (filename.length > FILE_CONSTRAINTS.MAX_FILENAME_LENGTH) {
      errors.push(`Filename too long (max ${FILE_CONSTRAINTS.MAX_FILENAME_LENGTH} characters)`);
    }
    const forbiddenChars = FILE_CONSTRAINTS.FORBIDDEN_CHARS.filter((char) => filename.includes(char));
    if (forbiddenChars.length > 0) {
      errors.push(`Filename contains forbidden characters: ${forbiddenChars.join(", ")}`);
    }
    const baseName = pathUtils2.basename(filename, pathUtils2.extname(filename)).toUpperCase();
    if (FILE_CONSTRAINTS.RESERVED_NAMES.includes(baseName)) {
      errors.push(`Filename uses reserved name: ${baseName}`);
    }
    const ext = pathUtils2.extname(filename).toLowerCase();
    if (ext && !FILE_CONSTRAINTS.ALLOWED_EXTENSIONS.includes(ext)) {
      warnings.push(`File extension ${ext} is not in allowed list`);
    }
    if (filename.trim() !== filename) {
      warnings.push("Filename has leading or trailing spaces");
    }
    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }
  /**
   * Validate file content
   */
  static validateContent(content) {
    const errors = [];
    const warnings = [];
    if (content === null || content === void 0) {
      errors.push("Content is required");
      return { valid: false, errors, warnings };
    }
    let size;
    if (typeof content === "string") {
      size = Buffer.byteLength(content, "utf8");
    } else if (content instanceof Buffer) {
      size = content.length;
    } else if (content instanceof Uint8Array) {
      size = content.length;
    } else {
      errors.push("Content must be string, Buffer, or Uint8Array");
      return { valid: false, errors, warnings };
    }
    if (size > FILE_CONSTRAINTS.MAX_FILE_SIZE) {
      errors.push(`Content too large (max ${FILE_CONSTRAINTS.MAX_FILE_SIZE} bytes)`);
    }
    if (size === 0) {
      warnings.push("Content is empty");
    }
    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }
  /**
   * Validate artifact type
   */
  static validateArtifactType(artifactType) {
    const errors = [];
    const warnings = [];
    if (!Object.values(ArtifactType).includes(artifactType)) {
      errors.push(`Invalid artifact type: ${artifactType}`);
    }
    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }
  /**
   * Validate encoding
   */
  static validateEncoding(encoding) {
    const errors = [];
    const warnings = [];
    const validEncodings = [
      "ascii",
      "utf8",
      "utf-8",
      "utf16le",
      "ucs2",
      "ucs-2",
      "base64",
      "base64url",
      "latin1",
      "binary",
      "hex"
    ];
    if (!validEncodings.includes(encoding)) {
      errors.push(`Invalid encoding: ${encoding}`);
    }
    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }
  /**
   * Validate file path
   */
  static validatePath(filePath) {
    const errors = [];
    const warnings = [];
    if (!filePath || typeof filePath !== "string") {
      errors.push("File path is required and must be a string");
      return { valid: false, errors, warnings };
    }
    if (filePath.includes("..")) {
      errors.push("Path traversal not allowed");
    }
    if (filePath.includes("\0")) {
      errors.push("Path contains null bytes");
    }
    if (filePath.length > 4096) {
      errors.push("Path too long (max 4096 characters)");
    }
    const normalizedPath = filePath.replace(/\\/g, "/").replace(/\/+/g, "/");
    if (normalizedPath !== filePath) {
      warnings.push("Path was normalized during validation");
    }
    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }
  /**
   * Validate session ID format
   */
  static validateSessionId(sessionId) {
    const errors = [];
    const warnings = [];
    if (!sessionId || typeof sessionId !== "string") {
      errors.push("Session ID is required and must be a string");
      return { valid: false, errors, warnings };
    }
    const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
    if (!uuidPattern.test(sessionId)) {
      errors.push("Session ID must be a valid UUID format");
    }
    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }
};

// src/commands/file/client/FileClient.ts
var FileClient = class extends BaseFileCommand {
  /**
   * Get continuum global instance
   */
  static getContinuumInstance() {
    const continuum2 = window.continuum;
    if (!continuum2) {
      throw new Error("Continuum not available in browser context");
    }
    return continuum2;
  }
  /**
   * Validate file operation parameters using shared validator
   */
  static validateParams(params2) {
    return FileValidator.validateParams(params2);
  }
  /**
   * Send file operation to server via continuum.execute()
   */
  static async sendToServer(command, params2) {
    const continuum2 = this.getContinuumInstance();
    return await continuum2.execute(command, params2);
  }
  /**
   * Convert content to base64 for server transport
   */
  static async convertToBase64(content) {
    if (typeof content === "string") {
      return btoa(content);
    }
    if (content instanceof Uint8Array) {
      const blob = new Blob([content]);
      const reader = new FileReader();
      return new Promise((resolve, reject) => {
        reader.onload = () => {
          const result2 = reader.result;
          const base64Data = result2.split(",")[1];
          resolve(base64Data);
        };
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
      });
    }
    throw new Error("Unsupported content type for base64 conversion");
  }
  /**
   * Convert base64 to Uint8Array for processing
   */
  static base64ToUint8Array(base64) {
    const binaryString = atob(base64);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i);
    }
    return bytes;
  }
  /**
   * Create data URL for preview purposes
   */
  static createDataUrl(content, mimeType) {
    if (typeof content === "string") {
      return `data:${mimeType};base64,${btoa(content)}`;
    }
    if (content instanceof Uint8Array) {
      const blob = new Blob([content], { type: mimeType });
      return URL.createObjectURL(blob);
    }
    throw new Error("Unsupported content type for data URL creation");
  }
  /**
   * Detect MIME type from filename
   */
  static detectMimeType(filename) {
    const ext = filename.split(".").pop()?.toLowerCase();
    switch (ext) {
      case "png":
        return "image/png";
      case "jpg":
      case "jpeg":
        return "image/jpeg";
      case "gif":
        return "image/gif";
      case "svg":
        return "image/svg+xml";
      case "webp":
        return "image/webp";
      case "pdf":
        return "application/pdf";
      case "txt":
        return "text/plain";
      case "html":
        return "text/html";
      case "css":
        return "text/css";
      case "js":
        return "application/javascript";
      case "json":
        return "application/json";
      default:
        return "application/octet-stream";
    }
  }
  /**
   * Log client-side file operation
   */
  static logClientOperation(operation2, params2) {
    console.log(`\u{1F4BE} FileClient: ${operation2}`, {
      filename: params2.filename,
      size: params2.content?.length || 0,
      artifactType: params2.artifactType,
      timestamp: (/* @__PURE__ */ new Date()).toISOString()
    });
  }
  /**
   * Handle client-side errors consistently
   */
  static handleClientError(error, operation2) {
    console.error(`\u274C FileClient: ${operation2} failed:`, error);
    throw new Error(`File ${operation2} failed: ${error instanceof Error ? error.message : String(error)}`);
  }
  /**
   * Check if browser supports required APIs
   */
  static checkBrowserSupport() {
    const missing = [];
    if (!window.FileReader) missing.push("FileReader");
    if (!window.Blob) missing.push("Blob");
    if (!window.URL) missing.push("URL");
    if (!window.atob) missing.push("atob");
    if (!window.btoa) missing.push("btoa");
    return {
      supported: missing.length === 0,
      missing
    };
  }
};

// src/commands/file/fileSave/client/FileSaveClient.ts
var _FileSaveClient = class _FileSaveClient extends FileClient {
  /**
   * Get singleton instance
   */
  static getInstance() {
    _FileSaveClient.instance ?? (_FileSaveClient.instance = new _FileSaveClient());
    return _FileSaveClient.instance;
  }
  /**
   * Save file through the server command
   */
  async saveFile(options) {
    try {
      const browserSupport = _FileSaveClient.checkBrowserSupport();
      if (!browserSupport.supported) {
        throw new Error(`Browser missing required APIs: ${browserSupport.missing.join(", ")}`);
      }
      _FileSaveClient.logClientOperation("saveFile", options);
      const continuum2 = _FileSaveClient.getContinuumInstance();
      const base64Content = await _FileSaveClient.convertToBase64(options.content);
      if (!base64Content) {
        throw new Error("Base64 content conversion failed");
      }
      const result2 = await continuum2.execute("file_save", {
        filename: options.filename,
        content: base64Content,
        encoding: "base64",
        artifactType: options.artifactType ?? "screenshot" /* SCREENSHOT */
      });
      return {
        success: true,
        data: result2.data || result2
      };
    } catch (error) {
      console.error("\u274C FileSaveClient: Save failed:", error);
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
  /**
   * Save file with preview generation
   */
  async saveFileWithPreview(options) {
    try {
      const result2 = await this.saveFile(options);
      if (result2.success && result2.data) {
        const mimeType = _FileSaveClient.detectMimeType(options.filename);
        if (mimeType.startsWith("image/")) {
          const preview = _FileSaveClient.createDataUrl(options.content, mimeType);
          return { ...result2, preview };
        }
      }
      return result2;
    } catch (error) {
      _FileSaveClient.handleClientError(error, "saveFileWithPreview");
    }
  }
};
_FileSaveClient.instance = null;
var FileSaveClient = _FileSaveClient;
async function saveFile(options) {
  const client = FileSaveClient.getInstance();
  return await client.saveFile(options);
}
async function saveFileWithPreview(options) {
  const client = FileSaveClient.getInstance();
  return await client.saveFileWithPreview(options);
}
window.FileSaveClient = FileSaveClient;
window.saveFile = saveFile;
window.saveFileWithPreview = saveFileWithPreview;

// src/commands/browser/screenshot/shared/ScreenshotBase.ts
var ScreenshotBase = class {
  /**
   * Dynamically load html2canvas library (shared across all instances)
   */
  static async loadHtml2Canvas() {
    if (typeof window === "undefined") {
      throw new Error("html2canvas is only available in browser context");
    }
    if (window.html2canvas) {
      return window.html2canvas;
    }
    if (this.html2canvasPromise) {
      return this.html2canvasPromise;
    }
    this.html2canvasPromise = new Promise((resolve, reject) => {
      const script = document.createElement("script");
      script.src = "https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js";
      script.onload = () => {
        console.log("\u2705 html2canvas loaded successfully");
        this.html2canvasLoaded = true;
        const html2canvas = window.html2canvas;
        if (html2canvas) {
          resolve(html2canvas);
        } else {
          reject(new Error("html2canvas not available after load"));
        }
      };
      script.onerror = () => {
        reject(new Error("Failed to load html2canvas library"));
      };
      document.head.appendChild(script);
    });
    return this.html2canvasPromise;
  }
  /**
   * Find target element with fallback logic
   */
  static findTargetElement(options) {
    const selector = options.querySelector || options.selector || "body";
    if (selector === "body") {
      return document.body;
    }
    const element = document.querySelector(selector);
    if (!element) {
      throw new Error(`Screenshot target not found: ${selector}`);
    }
    return element;
  }
  /**
   * Get element name for logging (simplified)
   */
  static getElementName(element) {
    if (element.id) return `#${element.id}`;
    if (element.classList.length > 0) return `.${element.classList[0]}`;
    return element.tagName.toLowerCase();
  }
  /**
   * Validate screenshot parameters
   */
  static validateOptions(options) {
    if (options.scale && (options.scale < 0.1 || options.scale > 2)) {
      throw new Error("Scale must be between 0.1 and 2.0");
    }
    if (options.quality && (options.quality < 0.1 || options.quality > 1)) {
      throw new Error("Quality must be between 0.1 and 1.0");
    }
    if (options.width && options.width <= 0) {
      throw new Error("Width must be positive");
    }
    if (options.height && options.height <= 0) {
      throw new Error("Height must be positive");
    }
  }
  /**
   * Process canvas with AI-friendly features: scaling, cropping, compression
   */
  static processCanvas(canvas, options) {
    const processedCanvas = document.createElement("canvas");
    const ctx = processedCanvas.getContext("2d");
    let sourceX = options.cropX || 0;
    let sourceY = options.cropY || 0;
    let sourceWidth = options.cropWidth || canvas.width;
    let sourceHeight = options.cropHeight || canvas.height;
    sourceX = Math.max(0, Math.min(sourceX, canvas.width));
    sourceY = Math.max(0, Math.min(sourceY, canvas.height));
    sourceWidth = Math.min(sourceWidth, canvas.width - sourceX);
    sourceHeight = Math.min(sourceHeight, canvas.height - sourceY);
    let targetWidth = options.width || sourceWidth;
    let targetHeight = options.height || sourceHeight;
    if (options.width && !options.height) {
      targetHeight = sourceHeight * options.width / sourceWidth;
    } else if (options.height && !options.width) {
      targetWidth = sourceWidth * options.height / sourceHeight;
    }
    targetWidth = Math.min(targetWidth, sourceWidth);
    targetHeight = Math.min(targetHeight, sourceHeight);
    processedCanvas.width = targetWidth;
    processedCanvas.height = targetHeight;
    ctx.drawImage(
      canvas,
      sourceX,
      sourceY,
      sourceWidth,
      sourceHeight,
      0,
      0,
      targetWidth,
      targetHeight
    );
    return processedCanvas;
  }
  /**
   * Generate filename with timestamp if not provided
   */
  static generateFilename(options, format = "png" /* PNG */) {
    if (options.filename) {
      return options.filename;
    }
    const timestamp = Date.now();
    const selector = options.querySelector || options.selector || "body";
    const elementName = selector.replace(/[^a-zA-Z0-9]/g, "-");
    return `screenshot-${elementName}-${timestamp}.${format}`;
  }
};
ScreenshotBase.html2canvasLoaded = false;
ScreenshotBase.html2canvasPromise = null;

// src/commands/browser/screenshot/client/ScreenshotClient.ts
var _ScreenshotClient = class _ScreenshotClient extends ScreenshotBase {
  constructor() {
    super(...arguments);
    this.isRegistered = false;
  }
  /**
   * Singleton pattern for global event handler
   */
  static getInstance() {
    if (!_ScreenshotClient.instance) {
      _ScreenshotClient.instance = new _ScreenshotClient();
    }
    return _ScreenshotClient.instance;
  }
  /**
   * Register screenshot handler with WebSocket manager for remote execution
   */
  register() {
    if (this.isRegistered) return;
    document.addEventListener("continuum:remote_execution", this.handleRemoteExecution.bind(this));
    this.isRegistered = true;
    console.log("\u{1F4F8} ScreenshotClient registered for remote execution");
  }
  /**
   * Handle remote execution events - only respond to SCREENSHOT commands
   */
  async handleRemoteExecution(event2) {
    const customEvent = event2;
    const { request, respond } = customEvent.detail;
    if (request.command.toUpperCase() !== "SCREENSHOT") {
      return;
    }
    const startTime = Date.now();
    console.log("\u{1F4F8} ScreenshotClient handling remote screenshot request", request);
    try {
      const result2 = await this.executeCapture(request.params);
      const response = {
        success: true,
        data: result2,
        requestId: request.requestId,
        clientMetadata: {
          userAgent: navigator.userAgent,
          timestamp: Date.now(),
          executionTime: Date.now() - startTime
        }
      };
      respond(response);
    } catch (error) {
      console.error("\u274C Remote screenshot execution failed:", error);
      const response = {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        requestId: request.requestId,
        clientMetadata: {
          userAgent: navigator.userAgent,
          timestamp: Date.now(),
          executionTime: Date.now() - startTime
        }
      };
      respond(response);
    }
  }
  /**
   * Direct screenshot capture function (maintains backward compatibility)
   */
  async captureScreenshot(params2) {
    console.log("\u{1F4F8} BROWSER: Starting AI-enhanced screenshot capture");
    console.log("\u{1F4CB} BROWSER: Params:", params2);
    try {
      const result2 = await this.executeCapture(params2);
      const base64Data = result2.imageData;
      const bytes = Uint8Array.from(atob(base64Data), (c) => c.charCodeAt(0));
      console.log(`\u{1F4BE} BROWSER: Final image: ${bytes.length} bytes`);
      const continuum2 = window.continuum;
      const saveResult = await continuum2.fileSave({
        content: bytes,
        filename: params2.filename,
        artifactType: "screenshot"
      });
      console.log("\u{1F4BE} BROWSER: FileSave result:", saveResult);
      return {
        success: true,
        data: {
          ...result2,
          saved: saveResult.success,
          filePath: saveResult.data?.filePath || null,
          fullPath: saveResult.data?.fullPath || null,
          relativePath: saveResult.data?.relativePath || null,
          bytes: params2.destination === "bytes" || params2.destination === "both" ? bytes : void 0
        },
        timestamp: (/* @__PURE__ */ new Date()).toISOString(),
        processor: "browser-html2canvas-ai"
      };
    } catch (error) {
      console.error("\u274C BROWSER: Screenshot capture failed:", error);
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        timestamp: (/* @__PURE__ */ new Date()).toISOString(),
        processor: "browser-html2canvas-ai"
      };
    }
  }
  /**
   * Execute screenshot capture using html2canvas (implements abstract method)
   */
  async executeCapture(options) {
    ScreenshotBase.validateOptions(options);
    const targetElement = ScreenshotBase.findTargetElement(options);
    const elementRect = targetElement.getBoundingClientRect();
    const elementName = options.elementName || ScreenshotBase.getElementName(targetElement);
    console.log(`\u{1F3AF} BROWSER: Targeting element '${elementName}' at ${elementRect.width}x${elementRect.height}`);
    const html2canvas = await ScreenshotBase.loadHtml2Canvas();
    console.log("\u{1F4E6} BROWSER: html2canvas available - starting capture");
    const scale = options.scale || 1;
    const canvas = await html2canvas(document.body, {
      allowTaint: true,
      useCORS: true,
      scale,
      logging: false
    });
    const bodyRect = document.body.getBoundingClientRect();
    const relativeX = Math.max(0, (elementRect.left - bodyRect.left) * scale);
    const relativeY = Math.max(0, (elementRect.top - bodyRect.top) * scale);
    const relativeWidth = Math.min(elementRect.width * scale, canvas.width - relativeX);
    const relativeHeight = Math.min(elementRect.height * scale, canvas.height - relativeY);
    console.log(`\u{1F4CF} BROWSER: Element coordinates: ${relativeX},${relativeY} ${relativeWidth}x${relativeHeight}`);
    const selector = options.querySelector || options.selector || "body";
    const needsCropping = selector !== "body";
    let finalCanvas = canvas;
    if (needsCropping) {
      const croppedCanvas = document.createElement("canvas");
      const croppedCtx = croppedCanvas.getContext("2d");
      croppedCanvas.width = relativeWidth;
      croppedCanvas.height = relativeHeight;
      croppedCtx.drawImage(
        canvas,
        relativeX,
        relativeY,
        relativeWidth,
        relativeHeight,
        0,
        0,
        relativeWidth,
        relativeHeight
      );
      finalCanvas = croppedCanvas;
      console.log(`\u2702\uFE0F BROWSER: Cropped to element coordinates`);
    }
    const originalWidth = finalCanvas.width;
    const originalHeight = finalCanvas.height;
    console.log(`\u{1F5BC}\uFE0F BROWSER: Canvas captured, original size: ${originalWidth}x${originalHeight}`);
    const processedCanvas = ScreenshotBase.processCanvas(finalCanvas, options);
    const finalWidth = processedCanvas.width;
    const finalHeight = processedCanvas.height;
    console.log(`\u{1F3A8} BROWSER: Processing complete, final size: ${finalWidth}x${finalHeight}`);
    const format = options.format || "png";
    let quality = options.quality || 0.9;
    let imageData;
    do {
      imageData = format === "png" ? processedCanvas.toDataURL("image/png") : processedCanvas.toDataURL(`image/${format}`, quality);
      if (options.maxFileSize) {
        const estimatedSize = imageData.length * 3 / 4;
        if (estimatedSize > options.maxFileSize && quality > 0.1) {
          quality -= 0.1;
          console.log(`\u{1F4C9} BROWSER: Reducing quality to ${quality} for file size limit`);
          continue;
        }
      }
      break;
    } while (true);
    const base64Data = imageData.replace(/^data:image\/[a-z]+;base64,/, "");
    const bytes = Uint8Array.from(atob(base64Data), (c) => c.charCodeAt(0));
    const filename = ScreenshotBase.generateFilename(options, format);
    console.log(`\u2705 Screenshot captured: ${bytes.length} bytes`);
    return {
      imageData: base64Data,
      format,
      selector,
      filename,
      width: finalWidth,
      height: finalHeight,
      elementName,
      originalWidth,
      originalHeight,
      scale,
      cropped: needsCropping || !!(options.cropX || options.cropY || options.cropWidth || options.cropHeight),
      compressed: quality < 0.9,
      fileSizeBytes: bytes.length,
      dataUrl: imageData
    };
  }
};
_ScreenshotClient.instance = null;
var ScreenshotClient = _ScreenshotClient;
async function clientScreenshot(params2) {
  const client = ScreenshotClient.getInstance();
  return await client.captureScreenshot(params2);
}
window.clientScreenshot = clientScreenshot;
window.ScreenshotClient = ScreenshotClient;
ScreenshotClient.getInstance().register();

// src/ui/continuum-browser-client/ContinuumBrowserClient.ts
var ContinuumBrowserClient = class {
  constructor() {
    this.sessionId = null;
    this.clientId = null;
    this.SESSION_COOKIE_NAME = "continuum_session_id";
    this.SESSION_COOKIE_MAX_AGE = 30 * 24 * 60 * 60 * 1e3;
    // 30 days in ms
    this._state = "initializing";
    this.stateCallbacks = [];
    this.readyCallbacks = [];
    this.dynamicMethods = /* @__PURE__ */ new Map();
    this.version = package_default.version;
    this.initializeSessionFromCookie();
    window.continuum = this;
    this.consoleForwarder = new ConsoleForwarder(
      () => this._state,
      () => this.sessionId
    );
    this.webSocketManager = new WebSocketManager(this.version);
    this.setupModuleCallbacks();
    console.log(`\u{1F310} Continuum v${this.version} lifecycle starting...`);
    this.setState("connecting");
    this.webSocketManager.initializeConnection();
  }
  setupModuleCallbacks() {
    this.consoleForwarder.setExecuteCallback((command, params2) => {
      return this.execute(command, params2);
    });
    this.webSocketManager.setCallbacks({
      onStateChange: (state) => this.setState(state),
      onClientId: (clientId) => {
        this.clientId = clientId;
      },
      onSessionId: (sessionId) => {
        this.sessionId = sessionId;
        this.saveSessionToCookie(sessionId);
      },
      onMessage: (message2) => this.handleCustomMessage(message2)
    });
  }
  get state() {
    return this._state;
  }
  setState(newState) {
    if (this._state !== newState) {
      console.log(`\u{1F504} Continuum state: ${this._state} \u2192 ${newState}`);
      this._state = newState;
      this.stateCallbacks.forEach((callback) => callback(newState));
      this.handleStateChange(newState);
    }
  }
  handleStateChange(state) {
    switch (state) {
      case "connected":
        console.log("\u{1F50C} Continuum API connected to server");
        break;
      case "ready":
        this.readyCallbacks.forEach((callback) => callback());
        this.consoleForwarder.executeAndFlushConsoleMessageQueue();
        console.log("\u2705 Continuum API ready for use");
        this.consoleForwarder.performHealthCheck();
        console.log("\u{1F3A8} BROWSER_DEBUG: Testing widget discovery via WidgetDaemon...");
        this.execute("widget:discover", { paths: ["src/ui/components"] }).then((result2) => {
          console.log("\u{1F3A8} BROWSER_DEBUG: Widget discovery via WidgetDaemon result:", result2);
          const testObj = {
            test: "serialization",
            number: 42,
            nested: { key: "value", array: [1, 2, 3] },
            boolean: true
          };
          console.log("\u{1F9EA} SERIALIZATION_TEST: Testing object logging:", testObj);
          console.probe({
            message: "\u{1F52C} AI_DIAGNOSTIC_TEST: Probing DOM and browser state",
            data: { widgets: result2, serialization: testObj },
            executeJS: "JSON.stringify({ title: document.title, url: window.location.href, bodyChildren: document.body?.children.length || 0, scripts: document.scripts.length, stylesheets: document.styleSheets.length, readyState: document.readyState })",
            category: "dom-analysis",
            tags: ["test", "dom", "browser-state", "widget-discovery"]
          });
          console.probe({
            message: "\u{1F50D} WIDGET_INVESTIGATION: Checking widget custom elements status",
            executeJS: 'JSON.stringify({ sidebarElement: !!document.querySelector("continuum-sidebar"), chatElement: !!document.querySelector("chat-widget"), sidebarInnerHTML: document.querySelector("continuum-sidebar")?.innerHTML?.substring(0, 100) || "empty", chatInnerHTML: document.querySelector("chat-widget")?.innerHTML?.substring(0, 100) || "empty", customElementsSupported: !!window.customElements, bodyHTML: document.body?.innerHTML?.substring(0, 300) || "no body" })',
            category: "widget-debug",
            tags: ["widgets", "custom-elements", "debug"]
          });
          console.probe({
            message: "\u{1F50D} CUSTOM_ELEMENT_PROBE: Checking widget class registration",
            executeJS: 'JSON.stringify({ customElementsRegistry: Object.keys(window.customElements || {}), sidebarClass: typeof window.customElements?.get("continuum-sidebar"), chatClass: typeof window.customElements?.get("chat-widget"), globalClasses: Object.keys(window).filter(k => k.includes("Widget") || k.includes("Sidebar")).slice(0, 10) })',
            category: "custom-elements",
            tags: ["registration", "classes", "debug"]
          });
        }).catch((error) => {
          console.error("\u274C Widget discovery error:", error);
        });
        break;
      case "error":
        console.error("\u274C Continuum API in error state");
        break;
    }
  }
  handleCustomMessage(message2) {
    console.log("\u{1F4E8} Received custom message:", message2);
  }
  isConnected() {
    return this.state === "ready" && this.webSocketManager.isOpen();
  }
  async execute(command, params2 = {}) {
    if (!this.isConnected()) {
      throw new Error(`Continuum not ready (state: ${this.state})`);
    }
    return new Promise((resolve, reject) => {
      const requestId = `cmd_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      const timeout = setTimeout(() => {
        reject(new Error(`Command '${command}' timed out`));
      }, 1e4);
      const responseHandler = (event2) => {
        const message2 = event2.detail;
        if (message2.requestId === requestId) {
          clearTimeout(timeout);
          document.removeEventListener("continuum:command_response", responseHandler);
          if (message2.success) {
            resolve(message2.data ?? { success: true });
          } else {
            reject(new Error(message2.error ?? "Command failed"));
          }
        }
      };
      document.addEventListener("continuum:command_response", responseHandler);
      const commandData = {
        command,
        params: JSON.stringify(params2),
        requestId,
        sessionId: this.sessionId
      };
      this.webSocketManager.sendMessage({
        type: "execute_command",
        data: commandData,
        timestamp: (/* @__PURE__ */ new Date()).toISOString(),
        clientId: this.clientId,
        sessionId: this.sessionId
      });
    });
  }
  // File saving functionality - delegates to FileSaveClient
  async fileSave(options) {
    const fileSaveClient = FileSaveClient.getInstance();
    const result2 = await fileSaveClient.saveFile({
      content: options.content,
      filename: options.filename,
      artifactType: options.artifactType || void 0
    });
    if (result2.success) {
      return {
        success: true,
        data: result2.data ?? {}
      };
    } else {
      throw new Error(result2.error ?? "File save failed");
    }
  }
  // Dynamic method attachment
  attachMethod(name, method) {
    this.dynamicMethods.set(name, method);
    this[name] = method.bind(this);
    console.log(`\u{1F527} Dynamic method attached: ${name}`);
  }
  hasMethod(name) {
    return this.dynamicMethods.has(name) || typeof this[name] === "function";
  }
  // Lifecycle event handlers
  onStateChange(callback) {
    this.stateCallbacks.push(callback);
  }
  onReady(callback) {
    if (this.state === "ready") {
      callback();
    } else {
      this.readyCallbacks.push(callback);
    }
  }
  // Session cookie management
  initializeSessionFromCookie() {
    const cookieSessionId = this.getSessionFromCookie();
    if (cookieSessionId) {
      this.sessionId = cookieSessionId;
      console.log(`\u{1F36A} ContinuumBrowserClient: Restored session from cookie: ${cookieSessionId}`);
    }
  }
  getSessionFromCookie() {
    const cookies = document.cookie.split(";");
    for (const cookie of cookies) {
      const [name, value] = cookie.trim().split("=");
      if (name === this.SESSION_COOKIE_NAME) {
        return decodeURIComponent(value);
      }
    }
    return null;
  }
  saveSessionToCookie(sessionId) {
    if (!sessionId) return;
    const expirationDate = /* @__PURE__ */ new Date();
    expirationDate.setTime(expirationDate.getTime() + this.SESSION_COOKIE_MAX_AGE);
    const cookieValue = `${this.SESSION_COOKIE_NAME}=${encodeURIComponent(sessionId)}; expires=${expirationDate.toUTCString()}; path=/; SameSite=Strict`;
    document.cookie = cookieValue;
    console.log(`\u{1F36A} ContinuumBrowserClient: Saved session to cookie: ${sessionId}`);
  }
};

// src/ui/components/shared/WidgetServerControls.ts
var WidgetServerControls = class _WidgetServerControls {
  constructor() {
    this.commandDataSourceMap = /* @__PURE__ */ new Map();
    this.eventMappings = [];
    this.controlEventNames = [];
    this.initializeDynamicMappings();
    this.setupEventListeners();
  }
  static getInstance() {
    if (!_WidgetServerControls.instance) {
      _WidgetServerControls.instance = new _WidgetServerControls();
    }
    return _WidgetServerControls.instance;
  }
  /**
   * Initialize dynamic mappings using npm intelligence and command discovery
   */
  async initializeDynamicMappings() {
    await this.discoverCommandDataSourceMappings();
    this.initializeEventMappings();
    this.initializeControlEventNames();
  }
  /**
   * Discover command data source mappings using npm intelligence
   */
  async discoverCommandDataSourceMappings() {
    try {
      const continuum2 = window.continuum;
      if (continuum2 && typeof continuum2.execute === "function") {
        const helpResult = await continuum2.execute("help", {});
        if (helpResult.success && helpResult.data) {
          this.parseCommandDefinitionsForDataSources(helpResult.data);
        }
      }
      this.setupIntelligentDefaults();
    } catch (error) {
      console.warn("Could not discover command mappings dynamically, using intelligent defaults:", error);
      this.setupIntelligentDefaults();
    }
  }
  /**
   * Parse command definitions to find data source mappings
   */
  parseCommandDefinitionsForDataSources(_commandData) {
    const directMappings = [
      ["personas", "personas"],
      ["projects", "projects"],
      ["sessions", "sessions"],
      ["health", "health"],
      ["widgets", "discover_widgets"],
      ["logs", "console"],
      // Console command handles logs
      ["metrics", "health"]
      // Health command includes metrics
    ];
    for (const [dataSource, command] of directMappings) {
      this.commandDataSourceMap.set(dataSource, command);
    }
  }
  /**
   * Setup intelligent defaults using npm intelligence patterns
   */
  setupIntelligentDefaults() {
    const intelligentMappings = [
      { command: "personas", dataSource: "personas" },
      { command: "projects", dataSource: "projects" },
      { command: "sessions", dataSource: "sessions" },
      { command: "health", dataSource: "health" },
      { command: "discover_widgets", dataSource: "widgets" },
      { command: "help", dataSource: "commands" },
      // Help lists commands
      { command: "agents", dataSource: "daemons" },
      // Agents command for daemon info
      { command: "console", dataSource: "logs" },
      { command: "health", dataSource: "metrics" }
      // Health includes metrics
    ];
    for (const mapping of intelligentMappings) {
      this.commandDataSourceMap.set(mapping.dataSource, mapping.command);
    }
  }
  /**
   * Initialize event mappings dynamically
   */
  initializeEventMappings() {
    this.eventMappings = [
      { serverEvent: "session:created", widgetEvent: "server:session-created", enabled: true },
      { serverEvent: "session:joined", widgetEvent: "server:session-joined", enabled: true },
      { serverEvent: "health:updated", widgetEvent: "server:health-updated", enabled: true },
      { serverEvent: "data:updated", widgetEvent: "server:data-updated", enabled: true }
    ];
  }
  /**
   * Initialize control event names from types
   */
  initializeControlEventNames() {
    this.controlEventNames = [
      "widget:screenshot",
      "widget:refresh",
      "widget:export",
      "widget:validate",
      "widget:fetch-data",
      "widget:execute-command"
    ];
  }
  /**
   * Setup global event listeners for widget server control events
   * Uses dynamic discovery instead of hardcoded event names
   */
  setupEventListeners() {
    for (const eventName of this.controlEventNames) {
      this.setupControlEventListener(eventName);
    }
    this.setupServerEventListeners();
  }
  /**
   * Setup individual control event listener with proper routing
   */
  setupControlEventListener(eventName) {
    document.addEventListener(eventName, (event2) => {
      switch (eventName) {
        case "widget:screenshot":
          this.handleScreenshotEvent(event2);
          break;
        case "widget:refresh":
          this.handleRefreshEvent(event2);
          break;
        case "widget:export":
          this.handleExportEvent(event2);
          break;
        case "widget:validate":
          this.handleValidateEvent(event2);
          break;
        case "widget:fetch-data":
          this.handleFetchDataEvent(event2);
          break;
        case "widget:execute-command":
          this.handleExecuteCommandEvent(event2);
          break;
      }
    });
  }
  /**
   * Handle widget screenshot events
   */
  async handleScreenshotEvent(event2) {
    const customEvent = event2;
    try {
      console.log("\u{1F4F8} Server Control: Widget screenshot requested", customEvent.detail);
      const widgetElement = event2.target;
      const widgetId = widgetElement.tagName.toLowerCase();
      const continuum2 = window.continuum;
      if (continuum2) {
        const result2 = await continuum2.execute("screenshot", {
          target: "widget",
          widgetId,
          selector: widgetElement.tagName.toLowerCase(),
          includeContext: true,
          ...customEvent.detail
        });
        widgetElement.dispatchEvent(new CustomEvent("widget:screenshot-complete", {
          detail: { success: true, result: result2 }
        }));
        console.log("\u2705 Widget screenshot completed:", result2);
      } else {
        throw new Error("Continuum API not available");
      }
    } catch (error) {
      console.error("\u274C Widget screenshot failed:", error);
      const widgetElement = event2.target;
      widgetElement.dispatchEvent(new CustomEvent("widget:screenshot-complete", {
        detail: {
          success: false,
          error: error instanceof Error ? error.message : String(error)
        }
      }));
    }
  }
  /**
   * Handle widget refresh events
   */
  async handleRefreshEvent(event2) {
    const customEvent = event2;
    try {
      console.log("\u{1F504} Server Control: Widget refresh requested", customEvent.detail);
      const widgetElement = event2.target;
      const widgetId = widgetElement.tagName.toLowerCase();
      const continuum2 = window.continuum;
      if (continuum2) {
        const result2 = await continuum2.execute("reload", {
          target: "widget",
          widgetId,
          preserveState: customEvent.detail?.preserveState || true,
          ...customEvent.detail
        });
        widgetElement.dispatchEvent(new CustomEvent("widget:refresh-complete", {
          detail: { success: true, result: result2 }
        }));
        console.log("\u2705 Widget refresh completed:", result2);
      }
    } catch (error) {
      console.error("\u274C Widget refresh failed:", error);
      const widgetElement = event2.target;
      widgetElement.dispatchEvent(new CustomEvent("widget:refresh-complete", {
        detail: { success: false, error: String(error) }
      }));
    }
  }
  /**
   * Handle widget export events
   */
  async handleExportEvent(event2) {
    const customEvent = event2;
    try {
      console.log("\u{1F4BE} Server Control: Widget export requested", customEvent.detail);
      const widgetElement = event2.target;
      const widgetId = widgetElement.tagName.toLowerCase();
      const continuum2 = window.continuum;
      if (continuum2) {
        const result2 = await continuum2.execute("export", {
          target: "widget",
          widgetId,
          format: customEvent.detail?.format || "json",
          ...customEvent.detail
        });
        widgetElement.dispatchEvent(new CustomEvent("widget:export-complete", {
          detail: { success: true, result: result2 }
        }));
        console.log("\u2705 Widget export completed:", result2);
      }
    } catch (error) {
      console.error("\u274C Widget export failed:", error);
      const widgetElement = event2.target;
      widgetElement.dispatchEvent(new CustomEvent("widget:export-complete", {
        detail: { success: false, error: String(error) }
      }));
    }
  }
  /**
   * Handle widget validation events
   */
  async handleValidateEvent(event2) {
    const customEvent = event2;
    try {
      console.log("\u2705 Server Control: Widget validation requested", customEvent.detail);
      const widgetElement = event2.target;
      const widgetId = widgetElement.tagName.toLowerCase();
      const continuum2 = window.continuum;
      if (continuum2) {
        const result2 = await continuum2.execute("validate", {
          target: "widget",
          widgetId,
          validateAssets: true,
          validateContent: true,
          ...customEvent.detail
        });
        widgetElement.dispatchEvent(new CustomEvent("widget:validate-complete", {
          detail: { success: true, result: result2 }
        }));
        console.log("\u2705 Widget validation completed:", result2);
      }
    } catch (error) {
      console.error("\u274C Widget validation failed:", error);
      const widgetElement = event2.target;
      widgetElement.dispatchEvent(new CustomEvent("widget:validate-complete", {
        detail: { success: false, error: String(error) }
      }));
    }
  }
  /**
   * Handle widget data fetching events
   */
  async handleFetchDataEvent(event2) {
    const customEvent = event2;
    try {
      console.log("\u{1F4E1} Server Control: Widget data fetch requested", customEvent.detail);
      const widgetElement = event2.target;
      const widgetId = widgetElement.tagName.toLowerCase();
      const continuum2 = window.continuum;
      if (continuum2) {
        const request = customEvent.detail;
        const command = this.commandDataSourceMap.get(request.dataSource);
        if (!command) {
          throw new Error(`No command found for data source: ${request.dataSource}. Available: ${Array.from(this.commandDataSourceMap.keys()).join(", ")}`);
        }
        const result2 = await continuum2.execute(command, {
          requestingWidget: widgetId,
          ...request.params,
          ...request.filters && { filters: request.filters }
        });
        const response = {
          success: true,
          dataSource: request.dataSource,
          data: result2.data,
          timestamp: Date.now(),
          metadata: {
            fromCache: false,
            totalCount: result2.data?.length,
            hasMore: false
          }
        };
        widgetElement.dispatchEvent(new CustomEvent("widget:data-received", {
          detail: response
        }));
        console.log(`\u2705 Widget data fetched for ${widgetId}:`, result2);
      } else {
        throw new Error("Continuum API not available");
      }
    } catch (error) {
      console.error("\u274C Widget data fetch failed:", error);
      const widgetElement = event2.target;
      const errorResponse = {
        success: false,
        dataSource: customEvent.detail?.dataSource || "unknown",
        error: error instanceof Error ? error.message : String(error),
        timestamp: Date.now()
      };
      widgetElement.dispatchEvent(new CustomEvent("widget:data-received", {
        detail: errorResponse
      }));
    }
  }
  /**
   * Handle widget command execution events
   */
  async handleExecuteCommandEvent(event2) {
    const customEvent = event2;
    try {
      console.log("\u26A1 Server Control: Widget command execution requested", customEvent.detail);
      const widgetElement = event2.target;
      const widgetId = widgetElement.tagName.toLowerCase();
      const continuum2 = window.continuum;
      if (continuum2) {
        const request = customEvent.detail;
        const startTime = Date.now();
        const result2 = await continuum2.execute(request.command, {
          requestingWidget: widgetId,
          ...request.params,
          ...request.timeout && { timeout: request.timeout },
          ...request.priority && { priority: request.priority }
        });
        const executionTime = Date.now() - startTime;
        const response = {
          success: true,
          command: request.command,
          result: result2.data,
          timestamp: Date.now(),
          executionTime
        };
        widgetElement.dispatchEvent(new CustomEvent("widget:command-complete", {
          detail: response
        }));
        console.log(`\u2705 Widget command executed for ${widgetId}: ${request.command} (${executionTime}ms)`, result2);
      } else {
        throw new Error("Continuum API not available");
      }
    } catch (error) {
      console.error("\u274C Widget command execution failed:", error);
      const widgetElement = event2.target;
      const errorResponse = {
        success: false,
        command: customEvent.detail?.command || "unknown",
        error: error instanceof Error ? error.message : String(error),
        timestamp: Date.now()
      };
      widgetElement.dispatchEvent(new CustomEvent("widget:command-complete", {
        detail: errorResponse
      }));
    }
  }
  /**
   * Setup listeners for server events that widgets should respond to
   */
  setupServerEventListeners() {
    const continuum2 = window.continuum;
    if (continuum2 && typeof continuum2.on === "function") {
      for (const mapping of this.eventMappings) {
        if (mapping.enabled) {
          this.setupServerEventListener(continuum2, mapping);
        }
      }
    }
  }
  /**
   * Setup individual server event listener with proper typing
   */
  setupServerEventListener(continuum2, mapping) {
    continuum2.on(mapping.serverEvent, (data2) => {
      const typedData = this.castServerEventData(mapping.serverEvent, data2);
      this.broadcastToWidgets(mapping.widgetEvent, typedData);
    });
  }
  /**
   * Cast server event data to proper types based on event name
   */
  castServerEventData(serverEvent, data2) {
    switch (serverEvent) {
      case "session:created":
        return data2;
      case "session:joined":
        return data2;
      case "health:updated":
        return data2;
      case "data:updated":
        return data2;
      default:
        throw new Error(`Unknown server event: ${serverEvent}`);
    }
  }
  /**
   * Register a command data source mapping (used by commands to declare what data they provide)
   */
  static registerCommandDataSource(mapping) {
    const instance = _WidgetServerControls.getInstance();
    instance.commandDataSourceMap.set(mapping.dataSource, mapping.command);
    if (mapping.aliases) {
      for (const alias of mapping.aliases) {
        if (alias) {
          instance.commandDataSourceMap.set(alias, mapping.command);
        }
      }
    }
  }
  /**
   * Get current command data source mappings (for debugging)
   */
  static getCommandDataSourceMappings() {
    return new Map(_WidgetServerControls.getInstance().commandDataSourceMap);
  }
  /**
   * Register event mapping configuration (allows dynamic event setup)
   */
  static registerEventMapping(mapping) {
    const instance = _WidgetServerControls.getInstance();
    const existingIndex = instance.eventMappings.findIndex((m) => m.serverEvent === mapping.serverEvent);
    if (existingIndex !== -1) {
      instance.eventMappings[existingIndex] = mapping;
    } else {
      instance.eventMappings.push(mapping);
    }
    instance.setupServerEventListeners();
  }
  /**
   * Broadcast server events to all widgets that want to listen
   */
  broadcastToWidgets(eventType, data2) {
    const widgets = document.querySelectorAll("*");
    widgets.forEach((element) => {
      if (element.tagName.includes("-") && element.shadowRoot) {
        element.dispatchEvent(new CustomEvent(eventType, {
          detail: data2
        }));
      }
    });
  }
};
WidgetServerControls.getInstance();

// src/ui/continuum-browser-client/error/GlobalErrorHandler.ts
var GlobalErrorHandler = class _GlobalErrorHandler {
  constructor() {
    this.errorCount = 0;
  }
  static getInstance() {
    if (!_GlobalErrorHandler.instance) {
      _GlobalErrorHandler.instance = new _GlobalErrorHandler();
    }
    return _GlobalErrorHandler.instance;
  }
  /**
   * Wrap any function with comprehensive error handling
   */
  wrapFunction(fn, context) {
    return (...args) => {
      try {
        const result2 = fn(...args);
        if (result2 instanceof Promise) {
          return result2.catch((error) => {
            this.captureError(error, context);
            throw error;
          });
        }
        return result2;
      } catch (error) {
        this.captureError(error, context);
        throw error;
      }
    };
  }
  /**
   * Wrap async functions with error handling
   */
  wrapAsync(fn, context) {
    return async (...args) => {
      try {
        return await fn(...args);
      } catch (error) {
        this.captureError(error, context);
        throw error;
      }
    };
  }
  /**
   * Safe execution wrapper - catches errors but doesn't re-throw
   */
  async safeExecute(fn, context, fallback) {
    try {
      const result2 = await fn();
      return result2;
    } catch (error) {
      this.captureError(error, context);
      return fallback;
    }
  }
  /**
   * Manual error capture for explicit error handling
   */
  captureError(error, context) {
    this.errorCount++;
    const errorInfo = {
      error: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : void 0,
      context,
      timestamp: (/* @__PURE__ */ new Date()).toISOString(),
      errorId: this.errorCount,
      url: window.location.href
    };
    console.error(`\u{1F6A8} Global Error Handler:`, errorInfo);
    window.dispatchEvent(new CustomEvent("continuum:error", {
      detail: errorInfo
    }));
  }
  /**
   * Widget-specific error wrapper
   */
  wrapWidgetMethod(widgetName, methodName, fn) {
    return this.wrapFunction(fn, {
      component: "widget",
      widget: widgetName,
      operation: methodName
    });
  }
  /**
   * Command-specific error wrapper
   */
  wrapCommandMethod(commandName, methodName, fn) {
    return this.wrapFunction(fn, {
      component: "command",
      command: commandName,
      operation: methodName
    });
  }
  /**
   * API call wrapper
   */
  wrapAPICall(apiName, fn) {
    return this.wrapAsync(fn, {
      component: "api",
      operation: apiName
    });
  }
  /**
   * Get error statistics
   */
  getErrorStats() {
    return { errorCount: this.errorCount };
  }
};
var globalErrorHandler = GlobalErrorHandler.getInstance();
var captureError = (error, context) => globalErrorHandler.captureError(error, context);

// src/ui/components/shared/BaseWidget.ts
var BaseWidget = class extends HTMLElement {
  constructor() {
    super();
    // Properties that subclasses can set in constructor
    this.widgetName = "BaseWidget";
    this.widgetIcon = "\u{1F539}";
    this.widgetTitle = "Widget";
    // Protected state
    this.widgetConnected = false;
    this.isCollapsed = false;
    console.log(`\u{1F3D7}\uFE0F ${this.constructor.name}: Constructor called`);
    try {
      this.attachShadow({ mode: "open" });
      console.log(`\u{1F3D7}\uFE0F ${this.constructor.name}: Shadow DOM attached`);
    } catch (error) {
      captureError(error, {
        component: "widget",
        widget: this.constructor.name,
        operation: "constructor"
      });
      throw error;
    }
  }
  /**
   * Widget name for registration - override in subclasses
   */
  static get widgetName() {
    return "base-widget";
  }
  /**
   * HTML tag name - defaults to widgetName, override only if different
   */
  static get tagName() {
    return this.widgetName;
  }
  // Smart defaults - minimal requirements from subclasses
  /**
   * Widget base path - automagic from build-time directory mapping
   */
  static get basePath() {
    const widgetName = this.name.replace(/^_/, "");
    if (typeof window !== "undefined" && window.WIDGET_ASSETS) {
      const widgetInfo = window.WIDGET_ASSETS[widgetName];
      if (widgetInfo && widgetInfo.directoryName) {
        return `/src/ui/components/${widgetInfo.directoryName}`;
      }
    }
    const directoryMap = {
      "ChatWidget": "Chat",
      "SidebarWidget": "Sidebar",
      "PersonaWidget": "Persona",
      "VersionWidget": "Version",
      "ContinuonWidget": "Continuon",
      "ActiveProjectsWidget": "ActiveProjects",
      "SavedPersonasWidget": "SavedPersonas",
      "UserSelectorWidget": "UserSelector"
    };
    const directoryName = directoryMap[widgetName] || widgetName;
    return `/src/ui/components/${directoryName}`;
  }
  // CSS and HTML loading is now handled directly in loadCSS() and loadHTMLTemplatesOrFallback()
  // using package.json files array - no complex discovery methods needed
  /**
   * Get widget files from package.json - replaces manual CSS/HTML declarations
   * Reads the 'files' array from widget's package.json automatically
   */
  static async getWidgetFiles() {
    try {
      const packagePath = `${this.basePath}/package.json`;
      console.log(`\u{1F4E6} ${this.name}: Fetching package.json from ${packagePath}`);
      const response = await fetch(packagePath);
      if (!response.ok) {
        console.warn(`\u{1F4E6} No package.json found for ${this.name} at ${packagePath}`);
        return [];
      }
      const packageData = await response.json();
      return packageData.files || [];
    } catch (error) {
      console.warn(`\u{1F4E6} Failed to read package.json for ${this.name}:`, error);
      return [];
    }
  }
  /**
   * Get all widget assets (except .ts files) - reads from package.json
   * Simple route: widget path + whatever package.json declares
   */
  static async getWidgetAssets() {
    const widgetFiles = await this.getWidgetFiles();
    const assets = widgetFiles.filter((file) => !file.endsWith(".ts"));
    return assets.map((file) => `${this.basePath}/${file}`);
  }
  /**
   * Auto-load HTML templates or fallback to renderContent() - zero burden
   */
  async loadHTMLTemplatesOrFallback() {
    try {
      const constructor = this.constructor;
      const basePath = constructor.basePath;
      const widgetAssets = window.WIDGET_ASSETS?.[constructor.name.replace(/^_/, "")];
      if (widgetAssets && widgetAssets.html.length > 0) {
        console.log(`\u{1F4C1} ${constructor.name}: Found ${widgetAssets.html.length} HTML files in manifest`);
        for (const htmlFile of widgetAssets.html) {
          const htmlPath = `${basePath}/${htmlFile}`;
          try {
            const response = await fetch(htmlPath);
            if (response.ok) {
              const htmlContent = await response.text();
              console.log(`\u2705 Loaded HTML template: ${htmlPath} (Zero 404s!)`);
              return htmlContent;
            } else {
              console.error(`\u{1F6A8} MANIFEST ERROR: ${htmlPath} not found but was in manifest!`);
            }
          } catch (error) {
            console.error(`\u{1F6A8} MANIFEST ERROR: Failed to fetch ${htmlPath}:`, error);
          }
        }
      } else {
        console.log(`\u{1F4C1} ${constructor.name}: No HTML files in manifest - using renderContent() fallback`);
      }
      return this.renderContent();
    } catch (error) {
      console.warn(`\u{1F3A8} ${this.widgetName}: HTML template loading failed, using fallback:`, error);
      return this.renderContent();
    }
  }
  /**
   * Load HTML templates if widget declares any (legacy method)
   */
  async loadHTMLTemplates() {
    const constructor = this.constructor;
    const assets = await constructor.getWidgetAssets();
    const htmlFiles = assets.filter((asset) => asset.endsWith(".html"));
    if (htmlFiles.length === 0) {
      return this.renderOwnContent();
    }
    try {
      const constructor2 = this.constructor;
      const htmlPromises = htmlFiles.map(
        (file) => fetch(`${constructor2.basePath}/${file}`).then((r) => r.text())
      );
      const htmlContents = await Promise.all(htmlPromises);
      return htmlContents.join("\n");
    } catch (error) {
      console.warn(`Failed to load HTML templates for ${constructor.name}:`, error);
      return this.renderOwnContent();
    }
  }
  /**
   * Widget declares its own HTML content (fallback when no HTML files)
   * Override in child classes to specify widget-specific content
   */
  renderOwnContent() {
    return "<p>Base widget - override renderOwnContent() or declare getOwnHTML()</p>";
  }
  /**
   * Base widget HTML structure - includes collapse, header, content
   */
  renderBaseHTML() {
    return `
      <div class="widget-container">
        <div class="widget-header" data-action="toggle-collapse">
          <div class="widget-title-row">
            <span class="widget-icon">${this.widgetIcon}</span>
            <span class="widget-title">${this.widgetTitle}</span>
            <span class="collapse-toggle">${this.isCollapsed ? "\u25B6" : "\u25BC"}</span>
          </div>
        </div>
        <div class="widget-content ${this.isCollapsed ? "collapsed" : ""}">
          ${this.renderOwnContent()}
        </div>
      </div>
    `;
  }
  /**
   * Get widget-relative asset path
   */
  getAssetPath(relativePath) {
    const basePath = this.constructor.basePath;
    return `${basePath}/${relativePath}`;
  }
  async connectedCallback() {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: connectedCallback() triggered - connecting to DOM`);
    this.widgetConnected = true;
    await globalErrorHandler.safeExecute(async () => {
      console.log(`\u{1F39B}\uFE0F ${this.widgetName}: About to call initializeWidget()`);
      await this.initializeWidget();
      console.log(`\u{1F39B}\uFE0F ${this.widgetName}: About to call render()`);
      await this.render();
      console.log(`\u{1F39B}\uFE0F ${this.widgetName}: connectedCallback() complete`);
    }, {
      component: "widget",
      widget: this.widgetName,
      operation: "connectedCallback"
    });
    if (!this.shadowRoot?.hasChildNodes()) {
      if (this.shadowRoot) {
        this.shadowRoot.innerHTML = `<div style="padding: 8px; color: #666; font-size: 12px;">\u26A0\uFE0F ${this.widgetName}: Loading...</div>`;
      }
    }
  }
  /**
   * Initialize widget - override for custom initialization
   */
  async initializeWidget() {
    this.setupServerEventListeners();
  }
  /**
   * Setup server event listeners - automatically called during initialization
   */
  setupServerEventListeners() {
    this.addEventListener("server:session-created", (event2) => {
      const customEvent = event2;
      this.onServerSessionCreated(customEvent.detail);
    });
    this.addEventListener("server:session-joined", (event2) => {
      const customEvent = event2;
      this.onServerSessionJoined(customEvent.detail);
    });
    this.addEventListener("server:health-updated", (event2) => {
      const customEvent = event2;
      this.onServerHealthUpdated(customEvent.detail);
    });
    this.addEventListener("server:data-updated", (event2) => {
      const customEvent = event2;
      this.onServerDataUpdated(customEvent.detail);
    });
    this.addEventListener("widget:data-received", (event2) => {
      const customEvent = event2;
      this.onDataReceived(customEvent.detail);
    });
    this.addEventListener("widget:command-complete", (event2) => {
      const customEvent = event2;
      this.onCommandComplete(customEvent.detail);
    });
  }
  /**
   * Called when a new session is created on the server
   * Override in child classes to respond to session events
   */
  onServerSessionCreated(event2) {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Session created [${event2.sessionType}]`, {
      owner: event2.owner,
      capabilities: event2.capabilities
    });
  }
  /**
   * Called when a session is joined on the server
   * Override in child classes to respond to session events
   */
  onServerSessionJoined(event2) {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Session joined [${event2.sessionType}]`, {
      joinedBy: event2.joinedBy,
      userCount: event2.userCount
    });
  }
  /**
   * Called when server health status is updated
   * Override in child classes to respond to health changes
   */
  onServerHealthUpdated(event2) {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Health updated [${event2.health.overall}]`, {
      score: event2.health.score,
      changedComponents: event2.changedComponents
    });
  }
  /**
   * Called when server data is updated
   * Override in child classes to respond to data changes
   */
  onServerDataUpdated(event2) {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Data updated [${event2.dataSource}]`, {
      updateType: event2.updateType,
      affectedItems: event2.affectedItems.length
    });
    if (this.shouldAutoRefreshOnDataUpdate(event2)) {
      this.update();
    }
  }
  /**
   * Called when server data is received from fetchServerData()
   * Override in child classes to handle data responses
   */
  onDataReceived(response) {
    if (response.success && response.data) {
      console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Data received [${response.dataSource}]`, {
        itemCount: response.metadata?.totalCount,
        fromCache: response.metadata?.fromCache
      });
      this.processServerData(response.dataSource, response.data);
    } else {
      console.error(`\u{1F39B}\uFE0F ${this.widgetName}: Data fetch failed [${response.dataSource}]`, response.error);
      this.onDataFetchError(response.dataSource, response.error || "Unknown error");
    }
  }
  /**
   * Called when server command completes from executeServerCommand()
   * Override in child classes to handle command responses
   */
  onCommandComplete(response) {
    if (response.success && response.result) {
      console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Command completed [${response.command}]`, {
        executionTime: response.executionTime
      });
      this.processCommandResult(response.command, response.result);
    } else {
      console.error(`\u{1F39B}\uFE0F ${this.widgetName}: Command failed [${response.command}]`, response.error);
      this.onCommandError(response.command, response.error || "Unknown error");
    }
  }
  /**
   * Override to determine if widget should auto-refresh when data changes
   * Uses strongly typed event data to make informed decisions
   */
  shouldAutoRefreshOnDataUpdate(_event) {
    return false;
  }
  /**
   * Override to process received server data
   * Type-safe processing with known data source types
   */
  processServerData(dataSource, data2) {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Received ${dataSource} data - override processServerData() to use it`, data2);
  }
  /**
   * Override to process command results
   * Type-safe command result processing
   */
  processCommandResult(command, result2) {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Command ${command} result - override processCommandResult() to use it`, result2);
  }
  /**
   * Override to handle data fetch errors
   * Type-safe error handling with data source context
   */
  onDataFetchError(dataSource, error) {
    console.warn(`\u{1F39B}\uFE0F ${this.widgetName}: Data fetch error [${dataSource}] - override onDataFetchError() to handle gracefully`, error);
  }
  /**
   * Override to handle command errors
   * Type-safe command error handling
   */
  onCommandError(command, error) {
    console.warn(`\u{1F39B}\uFE0F ${this.widgetName}: Command ${command} error - override onCommandError() to handle gracefully`, error);
  }
  /**
   * Get widget capabilities - override to declare what this widget can do
   * This enables the system to route events and validate permissions properly
   */
  getWidgetCapabilities() {
    return {
      canFetchData: [],
      // Override with DataSourceType[] that this widget needs
      canExecuteCommands: [],
      // Override with command names this widget uses  
      respondsToEvents: [],
      // Override with server event types this widget cares about
      supportsExport: [],
      // Override with export formats this widget supports
      requiresAuth: false,
      // Override if this widget needs authentication
      updateFrequency: "manual"
      // Override with 'realtime', 'periodic', or 'manual'
    };
  }
  disconnectedCallback() {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Disconnecting from DOM`);
    this.widgetConnected = false;
    globalErrorHandler.safeExecute(() => {
      this.cleanup();
    }, {
      component: "widget",
      widget: this.widgetName,
      operation: "disconnectedCallback"
    });
  }
  /**
   * Main render method - combines CSS and HTML
   */
  async render() {
    try {
      console.log(`\u{1F3A8} ${this.widgetName}: Starting render() - about to loadCSS()`);
      const css = await this.loadCSS();
      console.log(`\u{1F3A8} ${this.widgetName}: CSS loaded, length: ${css.length} chars`);
      const html = await this.loadHTMLTemplatesOrFallback();
      this.shadowRoot.innerHTML = `
        <style>
          ${css}
        </style>
        ${html}
      `;
      this.setupEventListeners();
      this.setupCollapseToggle();
    } catch (error) {
      console.error(`\u{1F39B}\uFE0F ${this.widgetName}: Render failed:`, error);
      this.renderError(error);
    }
  }
  /**
   * Load CSS for the widget
   */
  async loadCSS() {
    const constructor = this.constructor;
    try {
      const baseCSS = "/src/ui/components/shared/BaseWidget.css";
      const widgetAssets = window.WIDGET_ASSETS?.[constructor.name.replace(/^_/, "")];
      const cssFiles = [baseCSS];
      if (widgetAssets && widgetAssets.css.length > 0) {
        console.log(`\u{1F4C1} ${constructor.name}: Found ${widgetAssets.css.length} CSS files in manifest (Zero 404s!)`);
        const widgetCSSFiles = widgetAssets.css.map((file) => `${constructor.basePath}/${file}`);
        cssFiles.push(...widgetCSSFiles);
      } else {
        console.log(`\u{1F4C1} ${constructor.name}: No CSS files in manifest - using BaseWidget only`);
      }
      console.log(`\u{1F3A8} ${constructor.name}: Loading CSS files:`, cssFiles);
      const cssPromises = cssFiles.map(async (cssPath) => {
        try {
          const response = await fetch(cssPath);
          if (!response.ok) {
            console.error(`\u{1F6A8} MANIFEST ERROR: ${cssPath} not found but was in manifest!`);
            return "/* CSS failed to load - manifest error */";
          }
          const cssText = await response.text();
          console.log(`\u2705 Loaded CSS: ${cssPath} (${cssText.length} chars)`);
          return cssText;
        } catch (error) {
          console.error(`\u{1F6A8} MANIFEST ERROR: Failed to fetch ${cssPath}:`, error);
          return "/* CSS failed to load - manifest error */";
        }
      });
      const cssContents = await Promise.all(cssPromises);
      const combinedCSS = cssContents.join("\n");
      console.log(`\u2705 ${constructor.name}: Loaded ${cssFiles.length} CSS files successfully (Zero 404s!)`);
      return combinedCSS;
    } catch (error) {
      console.warn(`Failed to load CSS for ${constructor.name}:`, error);
      return this.getDefaultBaseCSS();
    }
  }
  /**
   * Get bundled CSS - legacy method, now uses declared assets
   */
  getBundledCSS() {
    return this.loadCSS();
  }
  /**
   * Load base CSS for collapse functionality
   */
  async loadBaseCSS() {
    try {
      const response = await fetch("/src/ui/components/shared/BaseWidget.css");
      return await response.text();
    } catch (error) {
      console.warn(`\u{1F39B}\uFE0F ${this.widgetName}: Failed to load base CSS, using fallback`, error);
      return this.getDefaultBaseCSS();
    }
  }
  /**
   * Fallback CSS for essential functionality if file loading fails
   */
  getDefaultBaseCSS() {
    return `
      /* Minimal fallback CSS for collapse functionality */
      .widget-header { cursor: pointer; padding: 12px 16px; }
      .widget-title-row { display: flex; align-items: center; gap: 8px; }
      .collapse-toggle { cursor: pointer; width: 16px; text-align: center; }
      :host(.collapsed) .widget-content { display: none; }
    `;
  }
  // Minimal methods - subclasses CAN override, but BaseWidget provides defaults
  renderContent() {
    return this.renderOwnContent();
  }
  setupEventListeners() {
  }
  /**
   * Cleanup method for when widget is disconnected
   */
  cleanup() {
  }
  /**
   * Render error state
   */
  renderError(error) {
    this.shadowRoot.innerHTML = `
      <style>
        .error-container {
          padding: 20px;
          background: rgba(244, 67, 54, 0.1);
          border: 1px solid rgba(244, 67, 54, 0.3);
          border-radius: 8px;
          color: #f44336;
          text-align: center;
        }
        .error-title {
          font-weight: 600;
          margin-bottom: 8px;
        }
        .error-message {
          font-size: 14px;
          opacity: 0.8;
        }
      </style>
      <div class="error-container">
        <div class="error-title">\u274C ${this.widgetName} Error</div>
        <div class="error-message">${error.message || "Unknown error occurred"}</div>
      </div>
    `;
  }
  /**
   * Update widget state and re-render
   */
  async update() {
    if (this.widgetConnected) {
      await this.render();
    }
  }
  /**
   * Server Control Events - Like onclick but for server actions
   * Widgets can trigger server-side actions via simple event dispatch
   * Uses centralized WidgetServerControls system
   */
  /**
   * Take screenshot of this widget (server control event)
   */
  triggerScreenshot(options = {}) {
    this.dispatchEvent(new CustomEvent("widget:screenshot", {
      detail: options,
      bubbles: true
    }));
  }
  /**
   * Refresh this widget from server (server control event) 
   */
  triggerRefresh(options = {}) {
    this.dispatchEvent(new CustomEvent("widget:refresh", {
      detail: options,
      bubbles: true
    }));
  }
  /**
   * Export widget data (server control event)
   */
  triggerExport(format = "json", options = {}) {
    this.dispatchEvent(new CustomEvent("widget:export", {
      detail: { format, ...options },
      bubbles: true
    }));
  }
  /**
   * Validate widget state (server control event)
   */
  triggerValidate(options = {}) {
    this.dispatchEvent(new CustomEvent("widget:validate", {
      detail: options,
      bubbles: true
    }));
  }
  /**
   * Fetch data from server (strongly typed server control event)
   */
  fetchServerData(dataSource, requestOptions = {}) {
    const request = {
      dataSource,
      ...requestOptions
      // Elegant spread - merge optional params, filters, etc.
    };
    this.dispatchEvent(new CustomEvent("widget:fetch-data", {
      detail: request,
      bubbles: true
    }));
  }
  /**
   * Execute server command (strongly typed server control event)
   */
  executeServerCommand(command, requestOptions = {}) {
    const request = {
      command,
      ...requestOptions
      // Elegant spread - merge timeout, priority, params, etc.
    };
    this.dispatchEvent(new CustomEvent("widget:execute-command", {
      detail: request,
      bubbles: true
    }));
  }
  /**
   * Get continuum API if available
   */
  getContinuumAPI() {
    return window.continuum;
  }
  /**
   * Check if continuum API is connected
   */
  isContinuumConnected() {
    const continuum2 = this.getContinuumAPI();
    return continuum2 && continuum2.isConnected();
  }
  /**
   * Send message via continuum API (using execute with message command)
   */
  sendMessage(message2) {
    this.notifySystem("widget_message", message2);
  }
  /**
   * Execute command via continuum API
   */
  async executeCommand(command, params2 = {}) {
    const continuum2 = this.getContinuumAPI();
    if (continuum2) {
      return await continuum2.execute(command, params2);
    } else {
      throw new Error("Continuum API not available");
    }
  }
  /**
   * Simple widget notification system - uses WidgetDaemon queue
   */
  notifySystem(eventType, data2) {
    try {
      const widgetDaemon = this.getWidgetDaemon();
      if (widgetDaemon) {
        widgetDaemon.notifySystem(this.widgetName, eventType, data2);
        return;
      }
      console.log(`\u{1F514} ${this.widgetName}: ${eventType}`, data2 ? { event: eventType, data: data2 } : { event: eventType });
      const continuum2 = this.getContinuumAPI();
      if (continuum2 && typeof continuum2.emit === "function") {
        continuum2.emit(eventType, { widget: this.widgetName, data: data2 });
      }
    } catch (error) {
      console.warn(`\u{1F39B}\uFE0F ${this.widgetName}: Failed to notify system about ${eventType}:`, error);
    }
  }
  /**
   * Get WidgetDaemon from browser daemon system
   */
  getWidgetDaemon() {
    try {
      const browserDaemonController = window.browserDaemonController;
      if (browserDaemonController && typeof browserDaemonController.getWidgetDaemon === "function") {
        return browserDaemonController.getWidgetDaemon();
      }
      const widgetDaemon = window.widgetDaemon;
      if (widgetDaemon && typeof widgetDaemon.notifySystem === "function") {
        return widgetDaemon;
      }
      return null;
    } catch (error) {
      return null;
    }
  }
  /**
   * Simple command execution with graceful fallback
   */
  async tryExecuteCommand(command, params2 = {}) {
    try {
      const continuum2 = this.getContinuumAPI();
      if (continuum2 && typeof continuum2.execute === "function") {
        return await continuum2.execute(command, params2);
      } else {
        console.warn(`\u{1F39B}\uFE0F ${this.widgetName}: Cannot execute command ${command} - API not available`);
        return { success: false, error: "Continuum API not available" };
      }
    } catch (error) {
      console.warn(`\u{1F39B}\uFE0F ${this.widgetName}: Command ${command} failed:`, error);
      return { success: false, error: error instanceof Error ? error.message : String(error) };
    }
  }
  /**
   * Register simple status listener - widgets can override this
   */
  onSystemStatus(status, data2) {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: System status ${status}`, data2);
  }
  /**
   * DEPRECATED: Use notifySystem() instead
   * Kept for backward compatibility with existing widgets
   */
  onContinuumEvent(type, _handler) {
    console.warn(`\u26A0\uFE0F ${this.widgetName}: onContinuumEvent() is deprecated. Use notifySystem() instead.`);
    console.log(`\u{1F504} ${this.widgetName}: Would register event listener for ${type} (deprecated API)`);
  }
  /**
   * Toggle collapse state for widgets
   */
  toggleCollapse() {
    this.isCollapsed = !this.isCollapsed;
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: ${this.isCollapsed ? "Collapsed" : "Expanded"}`);
    this.updateCollapseState();
  }
  /**
   * Update DOM to reflect collapse state
   */
  updateCollapseState() {
    const content = this.shadowRoot.querySelector(".widget-content");
    const toggle = this.shadowRoot.querySelector(".collapse-toggle");
    if (content) {
      if (this.isCollapsed) {
        content.style.display = "none";
        content.style.maxHeight = "0";
        content.style.overflow = "hidden";
      } else {
        content.style.display = "";
        content.style.maxHeight = "";
        content.style.overflow = "";
      }
    }
    if (toggle) {
      toggle.innerHTML = this.isCollapsed ? "\u25B6" : "\u25BC";
    }
    if (this.isCollapsed) {
      this.classList.add("collapsed");
    } else {
      this.classList.remove("collapsed");
    }
  }
  /**
   * Render error state when widget initialization fails
   */
  async renderErrorState(error) {
    if (!this.shadowRoot) return;
    const errorMessage = error instanceof Error ? error.message : String(error);
    this.shadowRoot.innerHTML = `
      <style>
        .error-widget {
          background: #ffe6e6;
          border: 2px solid #ff9999;
          border-radius: 8px;
          padding: 16px;
          margin: 8px;
          font-family: monospace;
          color: #cc0000;
        }
        .error-title {
          font-weight: bold;
          margin-bottom: 8px;
        }
        .error-message {
          margin-bottom: 8px;
          font-size: 14px;
        }
        .error-time {
          font-size: 12px;
          color: #666;
        }
      </style>
      <div class="error-widget">
        <div class="error-title">\u26A0\uFE0F ${this.widgetName} Error</div>
        <div class="error-message">${errorMessage}</div>
        <div class="error-time">${(/* @__PURE__ */ new Date()).toLocaleTimeString()}</div>
      </div>
    `;
  }
  /**
   * Setup collapse toggle functionality
   */
  setupCollapseToggle() {
    const toggle = this.shadowRoot.querySelector(".collapse-toggle");
    if (toggle) {
      toggle.addEventListener("click", (e) => {
        e.preventDefault();
        e.stopPropagation();
        this.toggleCollapse();
      });
    }
  }
  /**
   * Render widget with collapsible header
   */
  renderWithCollapseHeader(content) {
    return `
      <div class="widget-header">
        <div class="widget-title-row">
          <span class="collapse-toggle">\u25BC</span>
          <span class="widget-icon">${this.widgetIcon}</span>
          <span class="widget-title">${this.widgetTitle}</span>
        </div>
      </div>
      <div class="widget-content">
        ${content}
      </div>
    `;
  }
};

// src/ui/components/ActiveProjects/ActiveProjectsWidget.ts
var ActiveProjectsWidget = class extends BaseWidget {
  constructor() {
    super();
    this.projects = [];
    this.selectedProject = null;
    this.widgetName = "ActiveProjects";
    this.widgetIcon = "\u{1F4CB}";
    this.widgetTitle = "Active Projects";
  }
  async initializeWidget() {
    await this.loadProjects();
    this.setupContinuumListeners();
  }
  setupContinuumListeners() {
    if (this.getContinuumAPI()) {
      this.onContinuumEvent("projects_updated", () => {
        console.log("\u{1F39B}\uFE0F ActiveProjects: projects_updated received");
        this.loadProjects();
      });
      this.onContinuumEvent("project_progress_changed", (data2) => {
        console.log("\u{1F39B}\uFE0F ActiveProjects: project_progress_changed received", data2);
        this.updateProjectProgress(data2.projectId, data2.progress);
      });
      console.log("\u{1F39B}\uFE0F ActiveProjects: Connected to continuum API");
    } else {
      setTimeout(() => this.setupContinuumListeners(), 1e3);
    }
  }
  async loadProjects() {
    try {
      if (!this.isContinuumConnected()) {
        console.log("\u{1F39B}\uFE0F ActiveProjects: Not connected, using mock data");
        this.loadMockData();
        return;
      }
      const response = await this.executeCommand("projects", { action: "list_active" });
      if (response && response.projects) {
        this.projects = response.projects;
        console.log(`\u{1F39B}\uFE0F ActiveProjects: Loaded ${this.projects.length} projects`);
      } else {
        this.loadMockData();
      }
      await this.update();
    } catch (error) {
      console.error("\u{1F39B}\uFE0F ActiveProjects: Failed to load projects:", error);
      this.loadMockData();
    }
  }
  loadMockData() {
    this.projects = [
      {
        id: "continuum-os",
        name: "Continuum OS",
        status: "active",
        progress: 75,
        lastActivity: "2 minutes ago",
        assignedAgents: ["Claude Sonnet", "Protocol Sheriff"],
        priority: "high"
      },
      {
        id: "widget-system",
        name: "Widget System",
        status: "active",
        progress: 45,
        lastActivity: "15 minutes ago",
        assignedAgents: ["Code Specialist"],
        priority: "medium"
      }
    ];
    this.update();
  }
  updateProjectProgress(projectId, progress) {
    const project = this.projects.find((p) => p.id === projectId);
    if (project) {
      project.progress = progress;
      this.update();
    }
  }
  renderContent() {
    const content = `
      <div class="project-list">
        ${this.projects.length === 0 ? this.renderEmptyState() : this.projects.map((project) => this.renderProject(project)).join("")}
      </div>

      <div class="actions">
        <button class="btn btn-primary" data-action="create">New Project</button>
        <button class="btn btn-secondary" data-action="refresh">Refresh</button>
      </div>
    `;
    return this.renderWithCollapseHeader(content);
  }
  renderProject(project) {
    const isSelected = this.selectedProject?.id === project.id;
    const priorityIcon = this.getPriorityIcon(project.priority);
    const statusIcon = this.getStatusIcon(project.status);
    return `
      <div class="project-item ${isSelected ? "selected" : ""}" data-project-id="${project.id}">
        <div class="project-header">
          <div class="project-name">${project.name}</div>
          <div class="project-indicators">
            <span class="priority-indicator priority-${project.priority}">${priorityIcon}</span>
            <span class="status-indicator status-${project.status}">${statusIcon}</span>
          </div>
        </div>
        
        <div class="project-progress">
          <div class="progress-bar">
            <div class="progress-fill" style="width: ${project.progress}%"></div>
          </div>
          <span class="progress-text">${project.progress}%</span>
        </div>
        
        <div class="project-details">
          <div class="last-activity">${project.lastActivity}</div>
          <div class="assigned-agents">
            ${(project.assignedAgents || []).slice(0, 2).map((agent) => `<span class="agent-tag">${agent}</span>`).join("")}
            ${(project.assignedAgents || []).length > 2 ? `<span class="agent-count">+${(project.assignedAgents || []).length - 2}</span>` : ""}
          </div>
        </div>
      </div>
    `;
  }
  getPriorityIcon(priority) {
    switch (priority) {
      case "high":
        return "\u{1F534}";
      case "medium":
        return "\u{1F7E1}";
      case "low":
        return "\u{1F7E2}";
      default:
        return "\u26AA";
    }
  }
  getStatusIcon(status) {
    switch (status) {
      case "active":
        return "\u25B6\uFE0F";
      case "paused":
        return "\u23F8\uFE0F";
      case "completed":
        return "\u2705";
      case "planning":
        return "\u{1F4CB}";
      default:
        return "\u2753";
    }
  }
  renderEmptyState() {
    return `
      <div class="empty-state">
        No active projects.<br>
        Create your first project to get started!
      </div>
    `;
  }
  setupEventListeners() {
    this.shadowRoot.addEventListener("click", (e) => {
      const target = e.target;
      const projectItem = target.closest(".project-item");
      if (projectItem) {
        const projectId = projectItem.dataset.projectId;
        if (projectId) {
          this.selectProject(projectId);
        }
        return;
      }
      if (target.matches(".btn")) {
        const action = target.getAttribute("data-action");
        if (action) {
          this.handleAction(action);
        }
      }
    });
  }
  selectProject(projectId) {
    const project = this.projects.find((p) => p.id === projectId);
    if (project) {
      this.selectedProject = project;
      console.log("\u{1F39B}\uFE0F ActiveProjects: Selected project:", project.name);
      this.sendMessage({
        type: "project_selected",
        project
      });
      this.update();
    }
  }
  async handleAction(action) {
    switch (action) {
      case "create":
        console.log("\u{1F39B}\uFE0F ActiveProjects: Creating new project...");
        try {
          await this.executeCommand("projects", { action: "create" });
        } catch (error) {
          console.error("\u{1F39B}\uFE0F ActiveProjects: Failed to create project:", error);
        }
        break;
      case "refresh":
        console.log("\u{1F39B}\uFE0F ActiveProjects: Refreshing projects...");
        await this.loadProjects();
        break;
      default:
        console.log("\u{1F39B}\uFE0F ActiveProjects: Unknown action:", action);
    }
  }
};
if (!customElements.get("active-projects")) {
  customElements.define("active-projects", ActiveProjectsWidget);
}

// src/ui/components/shared/UniversalUserSystem.ts
var UniversalUserSystem = class {
  constructor() {
    this.users = /* @__PURE__ */ new Map();
    this.eventListeners = /* @__PURE__ */ new Map();
    this.loadUsersWhenReady();
  }
  async loadUsersWhenReady() {
    try {
      while (!globalThis.continuum) {
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
      const continuum2 = globalThis.continuum;
      const response = await continuum2.execute("users", {});
      if (response?.users && Array.isArray(response.users)) {
        console.log(`\u2705 Loaded ${response.users.length} real users from server`);
        response.users.forEach((user) => {
          this.addUser(user);
        });
        return;
      }
    } catch (error) {
      console.warn("\u26A0\uFE0F Failed to load real users, using fallback:", error);
    }
    this.addUser({
      id: "current-user",
      name: "User",
      type: "human",
      avatar: "\u{1F464}",
      status: "online",
      capabilities: ["all"],
      isClickable: false
    });
  }
  // ELEGANT: Simple user management
  addUser(user) {
    this.users.set(user.id, user);
    this.notifyUserAdded(user);
  }
  updateUser(userId, updates) {
    const user = this.users.get(userId);
    if (user) {
      Object.assign(user, updates);
      this.notifyUserUpdated(user);
    }
  }
  getUser(userId) {
    return this.users.get(userId) || null;
  }
  getAllUsers() {
    return Array.from(this.users.values());
  }
  getClickableUsers() {
    return Array.from(this.users.values()).filter((u) => u.isClickable);
  }
  // ELEGANT: Simple event system
  on(event2, callback) {
    if (!this.eventListeners.has(event2)) {
      this.eventListeners.set(event2, []);
    }
    this.eventListeners.get(event2).push(callback);
  }
  emit(event2, data2) {
    const listeners = this.eventListeners.get(event2) || [];
    listeners.forEach((callback) => callback(data2));
  }
  notifyUserAdded(user) {
    this.emit("user:added", user);
  }
  notifyUserUpdated(user) {
    this.emit("user:updated", user);
  }
  // ELEGANT: Handle user clicks for widget interactions
  handleUserClick(userId, options = {}) {
    const user = this.getUser(userId);
    if (user) {
      this.emit("user:clicked", { user, options });
      console.log(`\u{1F39B}\uFE0F User clicked: ${user.name}`, options);
      if (user.isClickable) {
        this.emit("user:interact", { user, action: "select", options });
      }
    } else {
      console.warn(`\u26A0\uFE0F User not found: ${userId}`);
    }
  }
  // ELEGANT: Simple HTML generation for UI
  generateConnectedUsersHTML() {
    const users = this.getAllUsers();
    return users.map((user) => `
      <div class="user-badge ${user.isClickable ? "clickable" : ""} ${user.type}" data-user-id="${user.id}">
        <span class="user-avatar">${user.avatar}</span>
        <span class="user-name">${user.name}</span>
        <span class="user-status ${user.status}"></span>
        ${user.currentTask ? `<span class="user-task">${user.currentTask}</span>` : ""}
      </div>
    `).join("");
  }
};
var universalUserSystem = new UniversalUserSystem();

// src/ui/components/shared/RoomDataManager.ts
var _RoomDataManager = class _RoomDataManager extends EventTarget {
  constructor() {
    super();
    this.rooms = /* @__PURE__ */ new Map();
    this.currentRoom = null;
    this.isInitialized = false;
    this.roomTypeConfig = null;
    console.log("\u{1F3E0} RoomDataManager: Initializing centralized room data system");
  }
  static getInstance() {
    if (!_RoomDataManager.instance) {
      _RoomDataManager.instance = new _RoomDataManager();
    }
    return _RoomDataManager.instance;
  }
  /**
   * Initialize with room data from server
   */
  async initialize() {
    if (this.isInitialized) {
      return;
    }
    try {
      console.log("\u{1F3E0} RoomDataManager: Loading room data from server...");
      await this.loadRoomTypeConfig();
      if (window.continuum && typeof window.continuum.execute === "function") {
        try {
          const response = await window.continuum.execute("listrooms", {});
          if (response?.rooms) {
            this.loadRoomsFromAPI(response.rooms);
            this.isInitialized = true;
            console.log(`\u{1F3E0} RoomDataManager: Loaded ${this.rooms.size} rooms from server API`);
            return;
          }
        } catch (error) {
          console.warn("\u{1F3E0} RoomDataManager: Server API not available, falling back to static data");
        }
      }
      await this.loadDefaultRooms();
      this.isInitialized = true;
      if (!this.currentRoom && this.rooms.has("general")) {
        this.setCurrentRoom("general");
      }
      console.log(`\u{1F3E0} RoomDataManager: Initialized with ${this.rooms.size} rooms`);
    } catch (error) {
      console.error("\u{1F3E0} RoomDataManager: Failed to initialize:", error);
      this.createFallbackRoom();
    }
  }
  async loadRoomTypeConfig() {
    try {
      const response = await fetch("/src/ui/components/shared/room-type-config.json");
      if (!response.ok) {
        throw new Error(`Failed to load room type config: ${response.status}`);
      }
      this.roomTypeConfig = await response.json();
      console.log("\u{1F3E0} RoomDataManager: Loaded room type configuration");
    } catch (error) {
      console.warn("\u{1F3E0} RoomDataManager: Failed to load room type config, using defaults:", error);
      this.roomTypeConfig = {
        roomTypes: {
          chat: { displayName: "Chat Room", icon: "\u{1F4AC}" }
        },
        fallbacks: {
          unknownType: { displayName: "Room", icon: "\u{1F3E0}" }
        }
      };
    }
  }
  loadRoomsFromAPI(roomsData) {
    for (const roomData of roomsData) {
      const room = {
        id: roomData.id,
        name: roomData.name || this.generateFallbackRoomName(roomData.id, roomData.type),
        type: roomData.type || "chat",
        description: roomData.description || `Chat room: ${roomData.name || this.generateFallbackRoomName(roomData.id, roomData.type)}`,
        autoCreated: roomData.autoCreated || false,
        metadata: roomData.metadata || {},
        participants: roomData.participants || [],
        messageCount: roomData.messageCount || 0,
        lastActivity: roomData.lastActivity ? new Date(roomData.lastActivity) : /* @__PURE__ */ new Date()
      };
      this.rooms.set(room.id, room);
    }
  }
  async loadDefaultRooms() {
    try {
      const response = await fetch("/src/daemons/chatroom/config/default-rooms.json");
      if (!response.ok) {
        throw new Error(`Failed to load default rooms: ${response.status}`);
      }
      const config = await response.json();
      for (const roomConfig of config.defaultRooms) {
        const room = {
          id: roomConfig.id,
          name: roomConfig.name,
          type: roomConfig.type,
          description: roomConfig.description,
          autoCreated: roomConfig.autoCreated || false,
          metadata: {
            ...roomConfig.metadata,
            icon: this.getDefaultRoomIcon(roomConfig.type, roomConfig.id)
          },
          participants: [],
          messageCount: 0,
          lastActivity: /* @__PURE__ */ new Date()
        };
        this.rooms.set(room.id, room);
      }
      console.log("\u{1F3E0} RoomDataManager: Loaded default rooms from config");
    } catch (error) {
      console.warn("\u{1F3E0} RoomDataManager: Failed to load default rooms config:", error);
      this.createFallbackRoom();
    }
  }
  createFallbackRoom() {
    const fallbackRoom = {
      id: "general",
      name: "General Chat",
      type: "chat",
      description: "Main chat room for general conversation",
      autoCreated: true,
      metadata: { default: true, category: "public", icon: "\u{1F4AC}" },
      participants: [],
      messageCount: 0,
      lastActivity: /* @__PURE__ */ new Date()
    };
    this.rooms.set("general", fallbackRoom);
    console.log("\u{1F3E0} RoomDataManager: Created fallback room");
  }
  getDefaultRoomIcon(type, id) {
    if (!this.roomTypeConfig) {
      return "\u{1F3E0}";
    }
    if (this.roomTypeConfig.roomTypes[type]) {
      return this.roomTypeConfig.roomTypes[type].icon;
    }
    if (this.roomTypeConfig.roomTypes[id]) {
      return this.roomTypeConfig.roomTypes[id].icon;
    }
    return this.roomTypeConfig.fallbacks?.unknownType?.icon || "\u{1F3E0}";
  }
  /**
   * Generate a fallback name for rooms that don't have proper names
   * Since room IDs are UUIDs, we can't just capitalize them
   */
  generateFallbackRoomName(roomId, type) {
    if (roomId.match(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i)) {
      if (this.roomTypeConfig?.roomTypes[type || "chat"]) {
        return this.roomTypeConfig.roomTypes[type || "chat"].displayName;
      }
      return this.roomTypeConfig?.fallbacks?.unknownType?.displayName || "Room";
    }
    return roomId.charAt(0).toUpperCase() + roomId.slice(1);
  }
  /**
   * Get complete room data by ID
   */
  getRoom(roomId) {
    return this.rooms.get(roomId) || null;
  }
  /**
   * Get all available rooms
   */
  getAllRooms() {
    return Array.from(this.rooms.values());
  }
  /**
   * Get current room data
   */
  getCurrentRoom() {
    return this.currentRoom;
  }
  /**
   * Set current room and notify all listeners
   */
  setCurrentRoom(roomId) {
    const newRoom = this.rooms.get(roomId);
    if (!newRoom) {
      console.warn(`\u{1F3E0} RoomDataManager: Room '${roomId}' not found`);
      return false;
    }
    const previousRoom = this.currentRoom;
    this.currentRoom = newRoom;
    const changeEvent = {
      previousRoom,
      currentRoom: newRoom,
      timestamp: /* @__PURE__ */ new Date()
    };
    console.log(`\u{1F3E0} RoomDataManager: Room changed: ${previousRoom?.name || "none"} \u2192 ${newRoom.name}`);
    this.dispatchEvent(new CustomEvent("room-changed", { detail: changeEvent }));
    this.dispatchEvent(new CustomEvent("current-room-updated", { detail: newRoom }));
    return true;
  }
  /**
   * Update room data (e.g., participant count, last activity)
   */
  updateRoom(roomId, updates) {
    const room = this.rooms.get(roomId);
    if (!room) {
      console.warn(`\u{1F3E0} RoomDataManager: Cannot update room '${roomId}' - not found`);
      return;
    }
    const updatedRoom = { ...room, ...updates };
    this.rooms.set(roomId, updatedRoom);
    if (this.currentRoom?.id === roomId) {
      this.currentRoom = updatedRoom;
      this.dispatchEvent(new CustomEvent("current-room-updated", { detail: updatedRoom }));
    }
    console.log(`\u{1F3E0} RoomDataManager: Updated room '${roomId}'`, updates);
  }
  /**
   * Add or update a participant in a room
   */
  updateRoomParticipant(roomId, participant) {
    const room = this.rooms.get(roomId);
    if (!room) return;
    const participants = room.participants || [];
    const existingIndex = participants.findIndex((p) => p.id === participant.id);
    if (existingIndex >= 0) {
      participants[existingIndex] = participant;
    } else {
      participants.push(participant);
    }
    this.updateRoom(roomId, { participants });
  }
  /**
   * Remove a participant from a room
   */
  removeRoomParticipant(roomId, participantId) {
    const room = this.rooms.get(roomId);
    if (!room) return;
    const participants = (room.participants || []).filter((p) => p.id !== participantId);
    this.updateRoom(roomId, { participants });
  }
  /**
   * Get welcome message for a room type from JSON config
   */
  getWelcomeMessage(roomType) {
    if (!this.roomTypeConfig) {
      return "Welcome! Start a conversation.";
    }
    const typeConfig = this.roomTypeConfig.roomTypes[roomType];
    if (typeConfig?.defaultWelcomeMessage) {
      return typeConfig.defaultWelcomeMessage;
    }
    return this.roomTypeConfig.fallbacks?.unknownType?.defaultWelcomeMessage || "Welcome! Start a conversation.";
  }
  /**
   * Check if room data manager is ready for use
   */
  isReady() {
    return this.isInitialized && this.rooms.size > 0;
  }
  /**
   * Force refresh room data from server
   */
  async refresh() {
    this.isInitialized = false;
    this.rooms.clear();
    await this.initialize();
  }
};
// Loaded from JSON
_RoomDataManager.instance = null;
var RoomDataManager = _RoomDataManager;
var roomDataManager = RoomDataManager.getInstance();

// src/ui/components/Chat/ChatWidget.ts
var ChatWidget = class extends BaseWidget {
  // Loaded from JSON
  constructor() {
    console.log("\u{1F3D7}\uFE0F ChatWidget: Constructor called, about to call super()");
    super();
    this.messages = [];
    this.isTyping = false;
    this.messageIdCounter = 0;
    this.currentRoom = null;
    // Now using complete room data instead of just ID
    this.isLoadingHistory = false;
    this.chatEventsConfig = null;
    console.log("\u{1F3D7}\uFE0F ChatWidget: super() completed, setting properties");
    this.widgetName = "ChatWidget";
    this.widgetIcon = "\u{1F4AC}";
    this.widgetTitle = "Chat";
    console.log("\u{1F3D7}\uFE0F ChatWidget: Constructor complete");
  }
  async initializeWidget() {
    await this.initializeRoomDataManager();
    await this.loadChatEventsConfig();
    await this.initializeChat();
    this.setupContinuumListeners();
    this.setupServerControlListeners();
    this.setupUniversalUserSystem();
    this.setupRoomDataListeners();
  }
  async initializeRoomDataManager() {
    console.log("\u{1F4AC} Chat: Initializing room data manager...");
    await roomDataManager.initialize();
    if (!this.currentRoom) {
      this.currentRoom = roomDataManager.getCurrentRoom() || roomDataManager.getRoom("general");
      if (!this.currentRoom) {
        console.warn("\u{1F4AC} Chat: No rooms available, will show placeholder");
      }
    }
  }
  async loadChatEventsConfig() {
    try {
      console.log("\u{1F4AC} Chat: Loading chat events configuration...");
      const response = await fetch("/src/ui/components/shared/chat-events-config.json");
      if (!response.ok) {
        throw new Error(`Failed to load chat events config: ${response.status}`);
      }
      this.chatEventsConfig = await response.json();
      console.log("\u{1F4AC} Chat: Chat events configuration loaded");
    } catch (error) {
      console.warn("\u{1F4AC} Chat: Failed to load chat events config, using defaults:", error);
      this.chatEventsConfig = {
        chatEvents: {
          message_received: { handler: "handleIncomingMessage", description: "New message" },
          agent_typing: { handler: "setTypingIndicator", handlerArgs: [true], description: "Typing" },
          agent_stop_typing: { handler: "setTypingIndicator", handlerArgs: [false], description: "Stop typing" }
        },
        globalEvents: {},
        messageTypes: {
          user: { icon: "\u{1F464}", className: "user-message", showAvatar: true, allowEdit: true },
          assistant: { icon: "\u{1F916}", className: "assistant-message", showAvatar: true, allowEdit: false },
          system: { icon: "\u2139\uFE0F", className: "system-message", showAvatar: false, allowEdit: false }
        },
        messageStatuses: {
          sending: { icon: "\u23F3", className: "status-sending", description: "Sending..." },
          sent: { icon: "\u2713", className: "status-sent", description: "Sent" },
          error: { icon: "\u274C", className: "status-error", description: "Failed" }
        }
      };
    }
  }
  async initializeChat() {
    const roomInfo = this.currentRoom ? `${this.currentRoom.name} (${this.currentRoom.type})` : "unknown room";
    console.log(`\u{1F4AC} Chat: Initializing chat for room: ${roomInfo}`);
    this.loadRoomHistory().catch((error) => {
      console.warn(`\u{1F4AC} Chat: History loading failed (non-blocking):`, error);
    });
    if (this.messages.length === 0 && this.currentRoom) {
      const welcomeMessage = roomDataManager.getWelcomeMessage(this.currentRoom.type);
      this.addMessage({
        id: this.generateMessageId(),
        type: "system",
        content: welcomeMessage,
        timestamp: /* @__PURE__ */ new Date()
      });
    }
    this.render();
    this.focusInput();
  }
  async loadRoomHistory() {
    if (this.isLoadingHistory || !this.isContinuumConnected()) {
      return;
    }
    try {
      this.isLoadingHistory = true;
      const response = await this.executeCommand("chat_history", {
        roomId: this.currentRoom?.id,
        limit: 50
      });
      if (response?.messages) {
        this.messages = response.messages.map((msg) => ({
          id: msg.id || this.generateMessageId(),
          type: msg.type || "assistant",
          content: msg.content || "",
          timestamp: new Date(msg.timestamp || Date.now()),
          metadata: {
            agent: msg.agent,
            persona: msg.persona
          }
        }));
      }
    } catch (error) {
      console.error(`\u{1F4AC} Chat: Failed to load history:`, error);
    } finally {
      this.isLoadingHistory = false;
    }
  }
  setupContinuumListeners() {
    if (!this.getContinuumAPI()) {
      window.addEventListener("continuum:ready", () => {
        this.setupContinuumListeners();
      }, { once: true });
      return;
    }
    if (!this.chatEventsConfig) {
      console.warn("\u{1F4AC} Chat: No chat events config available, skipping event setup");
      return;
    }
    for (const [eventName, eventConfig] of Object.entries(this.chatEventsConfig.chatEvents)) {
      this.notifySystem(eventName, (data2) => {
        if (eventConfig.requiresCurrentRoom && eventConfig.matchField) {
          const fieldValue = data2[eventConfig.matchField];
          if (fieldValue !== this.currentRoom?.id) {
            return;
          }
        }
        this.callEventHandler(eventConfig.handler, data2, eventConfig.handlerArgs);
      });
    }
    for (const [eventName, eventConfig] of Object.entries(this.chatEventsConfig.globalEvents)) {
      const listenerOptions = eventConfig.once ? { once: true } : {};
      document.addEventListener(eventName, (e) => {
        let data2 = e;
        if (eventConfig.dataPath) {
          const pathParts = eventConfig.dataPath.split(".");
          let current = e;
          for (const part of pathParts) {
            current = current?.[part];
          }
          data2 = current;
        }
        this.callEventHandler(eventConfig.handler, data2);
      }, listenerOptions);
    }
  }
  callEventHandler(handlerName, data2, handlerArgs) {
    try {
      const handler = this[handlerName];
      if (typeof handler === "function") {
        if (handlerArgs) {
          handler.call(this, ...handlerArgs);
        } else {
          handler.call(this, data2);
        }
      } else {
        console.warn(`\u{1F4AC} Chat: Handler '${handlerName}' not found`);
      }
    } catch (error) {
      console.error(`\u{1F4AC} Chat: Error calling handler '${handlerName}':`, error);
    }
  }
  setupRoomDataListeners() {
    roomDataManager.addEventListener("room-changed", (e) => {
      const customEvent = e;
      const { currentRoom } = customEvent.detail;
      this.handleRoomChange(currentRoom);
    });
    roomDataManager.addEventListener("current-room-updated", (e) => {
      const customEvent = e;
      const updatedRoom = customEvent.detail;
      if (this.currentRoom?.id === updatedRoom.id) {
        this.currentRoom = updatedRoom;
        this.updateRoomDisplay();
      }
    });
  }
  handleRoomChange(newRoom) {
    console.log(`\u{1F4AC} Chat: Room changed to ${newRoom.name} (${newRoom.type})`);
    this.currentRoom = newRoom;
    this.messages = [];
    const welcomeMessage = roomDataManager.getWelcomeMessage(newRoom.type);
    this.addMessage({
      id: this.generateMessageId(),
      type: "system",
      content: welcomeMessage,
      timestamp: /* @__PURE__ */ new Date()
    });
    this.loadRoomHistory().catch((error) => {
      console.warn(`\u{1F4AC} Chat: Failed to load history for room ${newRoom.name}:`, error);
    });
    this.render();
  }
  updateRoomDisplay() {
    const roomTitle = this.shadowRoot?.querySelector(".room-title");
    if (roomTitle && this.currentRoom) {
      roomTitle.textContent = this.currentRoom.name;
    }
    if (this.currentRoom) {
      this.widgetTitle = `\u{1F4AC} ${this.currentRoom.name}`;
    }
  }
  async switchRoom(roomId) {
    if (this.currentRoom?.id === roomId) return;
    console.log(`\u{1F4AC} Chat: Switching to room ${roomId}`);
    const success = roomDataManager.setCurrentRoom(roomId);
    if (!success) {
      console.warn(`\u{1F4AC} Chat: Failed to switch to room ${roomId} - room not found`);
      return;
    }
    this.render();
  }
  renderContent() {
    return `
      <div class="chat-container">
        <!-- Chat Header -->
        <div class="chat-header">
          <div class="header-left">
            <div class="chat-icon">\u{1F4AC}</div>
            <div class="header-info">
              <div class="chat-title">${this.getRoomDisplayName()}</div>
              <div class="chat-subtitle">${this.getRoomDescription()} \u2022 Connected</div>
            </div>
          </div>
          <div class="header-right">
            <div class="version-info">
              <div class="version" id="continuum-version">v${this.getVersion()}</div>
              <div class="timestamp">Updated ${(/* @__PURE__ */ new Date()).toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit" })}</div>
            </div>
          </div>
        </div>
        
        <!-- Connected Users -->
        <div class="connected-bar">
          <span class="connected-label">CONNECTED:</span>
          <div class="connected-users">
            ${this.renderSimpleConnectedUsers()}
          </div>
        </div>
        
        <div class="messages" id="messages">
          ${this.messages.length === 0 ? this.renderWelcome() : this.messages.map((msg) => this.renderMessage(msg)).join("")}
          ${this.isTyping ? this.renderTypingIndicator() : ""}
        </div>
        
        <div class="input-area">
          <div class="input-container">
            <textarea 
              id="messageInput" 
              class="input-field" 
              placeholder="Type your message..." 
              rows="1"
            ></textarea>
            <button class="send-button" id="sendButton" title="Send message">
              <span class="send-icon">\u27A4</span>
            </button>
          </div>
        </div>
      </div>
    `;
  }
  renderWelcome() {
    return `
      <div class="welcome-message">
        <div class="welcome-title">Welcome to Continuum</div>
        <div class="welcome-subtitle">Your AI collaboration platform</div>
        <div class="quick-actions">
          <div class="quick-action" data-action="help">Help</div>
          <div class="quick-action" data-action="status">System Status</div>
          <div class="quick-action" data-action="screenshot">Take Screenshot</div>
        </div>
      </div>
    `;
  }
  renderMessage(message2) {
    const timeStr = message2.timestamp.toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit"
    });
    const statusIcon = this.getStatusIcon(message2.status);
    const agentName = message2.metadata?.agent || message2.metadata?.persona || "System";
    return `
      <div class="message ${message2.type}" data-message-id="${message2.id}">
        <div class="message-content">${this.formatMessageContent(message2.content)}</div>
        <div class="message-meta">
          ${message2.type === "assistant" ? `<span class="agent-name">${agentName}</span>` : ""}
          <span class="message-time">${timeStr}</span>
          ${message2.status ? `<span class="message-status status-${message2.status}">${statusIcon}</span>` : ""}
        </div>
      </div>
    `;
  }
  renderTypingIndicator() {
    return `
      <div class="message assistant">
        <div class="typing-indicator">
          <span>AI is thinking</span>
          <div class="typing-dots">
            <div class="typing-dot"></div>
            <div class="typing-dot"></div>
            <div class="typing-dot"></div>
          </div>
        </div>
      </div>
    `;
  }
  formatMessageContent(content) {
    return content.replace(/\*\*(.*?)\*\*/g, "<strong>$1</strong>").replace(/\*(.*?)\*/g, "<em>$1</em>").replace(/`(.*?)`/g, "<code>$1</code>").replace(/\n/g, "<br>");
  }
  getStatusIcon(status) {
    switch (status) {
      case "sending":
        return "\u23F3";
      case "sent":
        return "\u2713";
      case "error":
        return "\u274C";
      default:
        return "";
    }
  }
  setupEventListeners() {
    const input = this.shadowRoot?.querySelector("#messageInput");
    const sendButton = this.shadowRoot?.querySelector("#sendButton");
    if (input && sendButton) {
      input.addEventListener("input", () => {
        this.autoResizeTextarea(input);
      });
      input.addEventListener("keydown", (e) => {
        if (e.key === "Enter" && !e.shiftKey) {
          e.preventDefault();
          this.sendChatMessage();
        }
      });
      sendButton.addEventListener("click", () => {
        this.sendChatMessage();
      });
    }
    this.shadowRoot?.querySelectorAll(".quick-action").forEach((action) => {
      action.addEventListener("click", (e) => {
        const target = e.currentTarget;
        const actionType = target.dataset.action;
        if (actionType) {
          this.handleQuickAction(actionType);
        }
      });
    });
  }
  autoResizeTextarea(textarea) {
    textarea.style.height = "auto";
    textarea.style.height = Math.min(textarea.scrollHeight, 120) + "px";
  }
  async sendChatMessage() {
    const input = this.shadowRoot?.querySelector("#messageInput");
    const sendButton = this.shadowRoot?.querySelector("#sendButton");
    if (!input || !sendButton) return;
    const content = input.value.trim();
    if (!content) return;
    input.value = "";
    input.style.height = "auto";
    sendButton.disabled = true;
    const userMessage = {
      id: this.generateMessageId(),
      type: "user",
      content,
      timestamp: /* @__PURE__ */ new Date(),
      status: "sending"
    };
    this.addMessage(userMessage);
    try {
      await this.executeCommand("chat", {
        message: content,
        roomId: this.currentRoom?.id,
        timestamp: userMessage.timestamp.toISOString()
      });
      this.updateMessageStatus(userMessage.id, "sent");
    } catch (error) {
      console.error("\u{1F4AC} Chat: Failed to send message:", error);
      this.updateMessageStatus(userMessage.id, "error");
      this.addMessage({
        id: this.generateMessageId(),
        type: "system",
        content: "Failed to send message. Please try again.",
        timestamp: /* @__PURE__ */ new Date()
      });
    } finally {
      sendButton.disabled = false;
      input.focus();
    }
  }
  handleIncomingMessage(data2) {
    const message2 = {
      id: this.generateMessageId(),
      type: "assistant",
      content: data2.content || data2.message || "No response received",
      timestamp: new Date(data2.timestamp || Date.now()),
      metadata: {
        agent: data2.agent,
        persona: data2.persona
      }
    };
    this.addMessage(message2);
    this.setTypingIndicator(false);
  }
  handleQuickAction(action) {
    if (action === "screenshot") {
      this.triggerScreenshot({ includeContext: true });
      return;
    }
    const quickMessages = {
      help: "Help me understand how to use Continuum",
      status: "Show me the system status",
      screenshot: "Take a screenshot of the current interface"
    };
    const input = this.shadowRoot?.querySelector("#messageInput");
    if (input && quickMessages[action]) {
      input.value = quickMessages[action];
      input.focus();
      this.autoResizeTextarea(input);
    }
  }
  /**
   * Setup Universal User System - all users have same interface & privileges
   */
  setupUniversalUserSystem() {
    console.log(`\u{1F4AC} Chat: Universal user system ready`);
    universalUserSystem.on("user:updated", () => {
      this.render();
    });
    universalUserSystem.on("persona:interaction-requested", (data2) => {
      this.startConversationWithUser(data2.personaId, data2.personaName, "persona");
    });
    universalUserSystem.on("ai-model:conversation-requested", (data2) => {
      this.startConversationWithUser(data2.modelId, data2.modelName, "ai-model");
    });
  }
  /**
   * Start conversation with any user type - same interface for all
   */
  async startConversationWithUser(userId, userName, _userType) {
    this.addMessage({
      id: this.generateMessageId(),
      type: "system",
      content: `\u{1F4AC} Starting direct conversation with ${userName}. They have the same privileges and command access as everyone else.`,
      timestamp: /* @__PURE__ */ new Date()
    });
    this.setConversationFocus(userId, userName);
  }
  /**
   * Set conversation focus to specific user
   */
  setConversationFocus(userId, userName) {
    const input = this.shadowRoot?.querySelector("#messageInput");
    if (input) {
      input.placeholder = `Message ${userName} directly...`;
      input.dataset.focusedUser = userId;
    }
    this.addMessage({
      id: this.generateMessageId(),
      type: "system",
      content: `\u{1F3AF} Conversation focused on ${userName}. Your messages will go directly to them. They can use all commands like screenshot, validate, export, etc.`,
      timestamp: /* @__PURE__ */ new Date()
    });
  }
  /**
   * Setup server control event listeners - handle callbacks from server
   */
  setupServerControlListeners() {
    this.addEventListener("widget:screenshot-complete", (event2) => {
      const customEvent = event2;
      const { success, result: result2, error } = customEvent.detail;
      if (success) {
        this.addMessage({
          id: this.generateMessageId(),
          type: "system",
          content: `\u{1F4F8} Screenshot captured: ${result2.filename || "screenshot.png"}`,
          timestamp: /* @__PURE__ */ new Date()
        });
      } else {
        this.addMessage({
          id: this.generateMessageId(),
          type: "system",
          content: `\u274C Screenshot failed: ${error}`,
          timestamp: /* @__PURE__ */ new Date()
        });
      }
    });
    this.addEventListener("widget:refresh-complete", (event2) => {
      const customEvent = event2;
      const { success, error } = customEvent.detail;
      if (success) {
        console.log("\u2705 ChatWidget refreshed successfully");
      } else {
        console.error("\u274C ChatWidget refresh failed:", error);
      }
    });
  }
  addMessage(message2) {
    this.messages.push(message2);
    this.render();
    this.scrollToBottom();
  }
  updateMessageStatus(messageId, status) {
    const message2 = this.messages.find((msg) => msg.id === messageId);
    if (message2) {
      message2.status = status;
      this.render();
    }
  }
  setTypingIndicator(isTyping) {
    if (this.isTyping !== isTyping) {
      this.isTyping = isTyping;
      this.render();
      if (isTyping) {
        this.scrollToBottom();
      }
    }
  }
  scrollToBottom() {
    setTimeout(() => {
      const container = this.shadowRoot?.querySelector(".messages");
      if (container) {
        container.scrollTop = container.scrollHeight;
      }
    }, 0);
  }
  generateMessageId() {
    return `msg_${this.messageIdCounter++}_${Date.now()}`;
  }
  getRoomDisplayName() {
    if (this.currentRoom) {
      return this.currentRoom.name;
    }
    return "Chat";
  }
  getRoomDescription() {
    if (this.currentRoom) {
      return this.currentRoom.description;
    }
    return "Chat room";
  }
  focusInput() {
    setTimeout(() => {
      const input = this.shadowRoot?.querySelector("#messageInput");
      if (input) {
        input.focus();
      }
    }, 100);
  }
  getVersion() {
    try {
      const continuum2 = window.continuum;
      if (continuum2 && continuum2.version) {
        return continuum2.version;
      }
      const serverVersion = window.__CONTINUUM_VERSION__;
      if (serverVersion) return serverVersion;
      return "unknown";
    } catch (error) {
      return "unknown";
    }
  }
  renderSimpleConnectedUsers() {
    const users = universalUserSystem.getAllUsers();
    const onlineUsers = users.filter((user) => user.status === "online");
    return onlineUsers.map((user) => `
      <span class="simple-user-badge" title="${user.name}">
        <span class="user-avatar">${user.avatar}</span>
        <span class="user-name">${user.name}</span>
      </span>
    `).join("");
  }
};
if (!customElements.get("chat-widget")) {
  customElements.define("chat-widget", ChatWidget);
}

// src/ui/components/ChatRoom/ChatRoom.ts
var ChatRoom = class extends BaseWidget {
  constructor() {
    super();
    this.availableRooms = [
      {
        id: "general",
        name: "General Chat",
        description: "Smart agent routing with Protocol Sheriff validation",
        icon: "\u{1F4AC}",
        type: "general",
        participants: [
          { id: "you", name: "YOU", type: "human", status: "online" },
          { id: "claude-code", name: "Claude Code", type: "ai", status: "online", avatar: "\u{1F916}" },
          { id: "auto-route", name: "Auto Route", type: "agent", status: "online", avatar: "\u{1F3AF}" },
          { id: "protocol-sheriff", name: "Protocol Sheriff", type: "agent", status: "online", avatar: "\u{1F6E1}\uFE0F" }
        ]
      },
      {
        id: "academy",
        name: "Academy Training",
        description: "AI training and persona development",
        icon: "\u{1F393}",
        type: "academy",
        participants: [
          { id: "you", name: "YOU", type: "human", status: "online" },
          { id: "testing-droid", name: "TestingDroid", type: "agent", status: "online", avatar: "\u{1F916}" },
          { id: "protocol-sheriff", name: "Protocol Sheriff", type: "agent", status: "online", avatar: "\u{1F6E1}\uFE0F" },
          { id: "code-specialist", name: "Code Specialist", type: "agent", status: "online", avatar: "\u{1F4BB}" }
        ]
      }
    ];
    this.widgetName = "ChatRoom";
    this.widgetIcon = "\u{1F3E0}";
    this.widgetTitle = "Chat Room";
    this.currentRoom = this.availableRooms[0];
  }
  async initializeWidget() {
    this.setupRoomEventListeners();
    this.render();
    console.log(`\u{1F3E0} ChatRoom: Initialized with room ${this.currentRoom.name}`);
  }
  setupRoomEventListeners() {
    document.addEventListener("continuum:switch-room", (e) => {
      const customEvent = e;
      this.switchToRoom(customEvent.detail.roomId);
    });
  }
  switchToRoom(roomId) {
    const newRoom = this.availableRooms.find((room) => room.id === roomId);
    if (!newRoom || newRoom.id === this.currentRoom.id) {
      return;
    }
    console.log(`\u{1F3E0} ChatRoom: Switching from ${this.currentRoom.name} to ${newRoom.name}`);
    this.currentRoom = newRoom;
    this.render();
    this.notifyRoomChange();
  }
  notifyRoomChange() {
    const event2 = new CustomEvent("continuum:room-changed", {
      detail: {
        room: this.currentRoom
      },
      bubbles: true
    });
    document.dispatchEvent(event2);
  }
  getCurrentRoom() {
    return this.currentRoom;
  }
  renderContent() {
    const participantsList = this.currentRoom.participants.map((p) => `
        <div class="participant ${p.status}" title="${p.name} (${p.type})">
          <span class="participant-avatar">${p.avatar || this.getDefaultAvatar(p.type)}</span>
          <span class="participant-name">${p.name}</span>
          <span class="participant-status-dot"></span>
        </div>
      `).join("");
    return `
      <div class="chat-room">
        <div class="room-header">
          <div class="room-title-section">
            <span class="room-icon">${this.currentRoom.icon}</span>
            <div class="room-info">
              <h2 class="room-title">${this.currentRoom.name}</h2>
              <p class="room-description">${this.currentRoom.description} \u2022 Connected</p>
            </div>
          </div>
          <div class="room-participants">
            <div class="participants-label">Connected:</div>
            <div class="participants-list">
              ${participantsList}
            </div>
          </div>
        </div>
        
        <div class="room-content">
          <chat-widget room="${this.currentRoom.id}"></chat-widget>
        </div>
      </div>
    `;
  }
  getDefaultAvatar(type) {
    switch (type) {
      case "human":
        return "\u{1F464}";
      case "ai":
        return "\u{1F916}";
      case "agent":
        return "\u{1F539}";
      default:
        return "\u2753";
    }
  }
  async loadCSS() {
    return `
      :host {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: rgba(15, 20, 25, 0.95);
      }

      .chat-room {
        display: flex;
        flex-direction: column;
        height: 100%;
      }

      .room-header {
        padding: 20px;
        border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        flex-shrink: 0;
        background: rgba(15, 20, 25, 0.98);
      }

      .room-title-section {
        display: flex;
        align-items: center;
        gap: 12px;
      }

      .room-icon {
        font-size: 24px;
        width: 40px;
        height: 40px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: rgba(255, 255, 255, 0.1);
        border-radius: 8px;
        border: 1px solid rgba(255, 255, 255, 0.2);
      }

      .room-info {
        flex: 1;
      }

      .room-title {
        margin: 0;
        font-size: 24px;
        font-weight: 600;
        color: #e0e6ed;
        line-height: 1.2;
      }

      .room-description {
        margin: 4px 0 0 0;
        font-size: 14px;
        color: #8a92a5;
        opacity: 0.8;
      }

      .room-participants {
        margin-top: 12px;
        display: flex;
        flex-direction: column;
        gap: 8px;
      }

      .participants-label {
        font-size: 12px;
        color: #8a92a5;
        font-weight: 500;
        text-transform: uppercase;
        letter-spacing: 0.5px;
      }

      .participants-list {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
      }

      .participant {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 4px 8px;
        background: rgba(255, 255, 255, 0.05);
        border: 1px solid rgba(255, 255, 255, 0.1);
        border-radius: 16px;
        font-size: 12px;
        position: relative;
        transition: all 0.2s ease;
      }

      .participant:hover {
        background: rgba(255, 255, 255, 0.08);
        border-color: rgba(79, 195, 247, 0.3);
      }

      .participant-avatar {
        font-size: 14px;
        line-height: 1;
      }

      .participant-name {
        color: #e0e6ed;
        font-weight: 500;
        white-space: nowrap;
      }

      .participant-status-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: #4ade80;
        flex-shrink: 0;
      }

      .participant.offline .participant-status-dot {
        background: #6b7280;
      }

      .participant.away .participant-status-dot {
        background: #fbbf24;
      }

      .room-content {
        flex: 1;
        overflow: hidden;
        display: flex;
        flex-direction: column;
      }

      chat-widget {
        flex: 1;
        display: flex;
        flex-direction: column;
      }
    `;
  }
  setupEventListeners() {
  }
};
customElements.define("chat-room", ChatRoom);

// src/ui/components/Continuon/ContinuonWidget.ts
var ContinuonWidget = class extends BaseWidget {
  constructor() {
    super();
    this.currentStatus = "red";
    this.currentEmotion = null;
    this.statusFeed = [];
    this.maxStatusMessages = 5;
    this.widgetName = "ContinuonWidget";
    this.widgetIcon = "\u{1F52E}";
    this.widgetTitle = "System Status Orb";
  }
  async initializeWidget() {
    await this.loadCSS();
    this.setupEventListeners();
    this.setupStatusFeed();
    const version = this.getSystemVersion();
    console.log(`\u{1F680} Continuum v${version} - ContinuonWidget initialized`);
    this.addStatusMessage("System initializing...");
    this.updateStatus("yellow", "Starting up...");
    this.render();
    setTimeout(() => {
      this.updateStatus("green", "System ready");
    }, 2e3);
  }
  setupEventListeners() {
    document.addEventListener("continuum:status-change", (e) => {
      const customEvent = e;
      this.updateStatus(customEvent.detail.status, customEvent.detail.message);
    });
    document.addEventListener("continuum:emotion", (e) => {
      const customEvent = e;
      this.showEmotion(customEvent.detail.emotion, customEvent.detail.duration || 3e3);
    });
    document.addEventListener("continuum:system-event", (e) => {
      const customEvent = e;
      this.addStatusMessage(customEvent.detail.message);
    });
    this.setupOrbEventListeners();
  }
  renderContent() {
    const orbContent = this.currentEmotion || "";
    const statusColor = this.getStatusColor();
    return `
      <div class="continuon-container">
        <div class="continuon-orb-container">
          <div class="continuon-orb ${statusColor}" data-emotion="${this.currentEmotion || ""}">
            <div class="orb-ring"></div>
            <div class="orb-center">
              <span class="orb-emotion">${orbContent}</span>
            </div>
            <div class="orb-glow"></div>
          </div>
          <span class="continuon-label">continuum</span>
        </div>
        
        <div class="status-feed">
          <div class="status-messages">
            ${this.renderStatusMessages()}
          </div>
        </div>
      </div>
    `;
  }
  renderStatusMessages() {
    return this.statusFeed.slice(-this.maxStatusMessages).map((msg, index) => `
        <div class="status-message fade-${index}" 
             style="animation-delay: ${index * 0.1}s">
          ${msg.text}
        </div>
      `).join("");
  }
  updateStatus(status, message2) {
    if (this.currentStatus !== status) {
      this.currentStatus = status;
      if (message2) {
        this.addStatusMessage(message2);
      }
      this.updateTitleAndFavicon();
      this.render();
      this.logVersionIfChanged();
    }
  }
  showEmotion(emotion, duration) {
    this.currentEmotion = emotion;
    this.updateTitleAndFavicon();
    this.render();
    setTimeout(() => {
      this.currentEmotion = null;
      this.updateTitleAndFavicon();
      this.render();
    }, duration);
  }
  addStatusMessage(text) {
    const message2 = {
      text,
      timestamp: Date.now(),
      id: Math.random().toString(36).substr(2, 9)
    };
    this.statusFeed.push(message2);
    if (this.statusFeed.length > this.maxStatusMessages * 2) {
      this.statusFeed = this.statusFeed.slice(-this.maxStatusMessages);
    }
    this.render();
  }
  getStatusColor() {
    switch (this.currentStatus) {
      case "green":
        return "status-healthy";
      case "yellow":
        return "status-degraded";
      case "red":
        return "status-error";
      default:
        return "status-error";
    }
  }
  logVersionIfChanged() {
    const version = this.getSystemVersion();
    const lastVersion = localStorage.getItem("continuum-last-version");
    if (version && version !== lastVersion) {
      console.log(`\u{1F680} Continuum ${lastVersion ? `${lastVersion} \u2192 ${version}` : `v${version}`}`);
      localStorage.setItem("continuum-last-version", version);
    }
  }
  getSystemVersion() {
    const continuum2 = window.continuum;
    return continuum2?.version || "0.2.2177";
  }
  setupStatusFeed() {
    const continuum2 = window.continuum;
    if (continuum2) {
      continuum2.on("connected", () => {
        this.updateStatus("green", "Connected");
      });
      continuum2.on("disconnected", () => {
        this.updateStatus("red", "Disconnected");
      });
      continuum2.on("reconnecting", () => {
        this.updateStatus("yellow", "Reconnecting...");
      });
    }
  }
  updateTitleAndFavicon() {
    const displayIcon = this.currentEmotion || this.getStatusIcon();
    this.updateFavicon(displayIcon);
    document.title = "continuum";
  }
  getStatusIcon() {
    switch (this.currentStatus) {
      case "green":
        return "\u{1F7E2}";
      case "yellow":
        return "\u{1F7E1}";
      case "red":
        return "\u{1F534}";
      default:
        return "\u{1F534}";
    }
  }
  updateFavicon(icon) {
    const favicon = document.getElementById("favicon");
    if (favicon) {
      const svg = `<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>${icon}</text></svg>`;
      favicon.href = `data:image/svg+xml,${encodeURIComponent(svg)}`;
    }
  }
  setupOrbEventListeners() {
    const orb = this.shadowRoot?.querySelector(".continuon-orb");
    if (orb) {
      orb.addEventListener("click", () => {
        this.triggerEmotionDemo();
      });
    }
  }
  triggerEmotionDemo() {
    const emotions = ["\u{1F609}", "\u{1F389}", "\u{1F680}", "\u{1F4AB}", "\u2728"];
    const randomEmotion = emotions[Math.floor(Math.random() * emotions.length)];
    this.showEmotion(randomEmotion, 2e3);
  }
};
customElements.define("continuon-widget", ContinuonWidget);

// src/ui/components/Persona/PersonaWidget.ts
var PersonaWidget = class _PersonaWidget extends BaseWidget {
  constructor() {
    super();
    this.config = null;
    this.isInteracting = false;
    this.widgetName = "Persona";
    this.widgetIcon = "\u{1F916}";
    this.widgetTitle = "AI Persona";
  }
  // Public method to configure the persona
  setPersona(config) {
    this.config = config;
    this.widgetTitle = config.name;
    this.render();
  }
  renderContent() {
    if (!this.config) {
      return `
        <div class="persona-header">
          <div class="persona-avatar">\u2753</div>
          <div class="persona-info">
            <div class="persona-name">No Persona Configured</div>
          </div>
        </div>
        <div class="persona-description">
          Use setPersona() to configure this component.
        </div>
      `;
    }
    const lastActiveStr = this.config.lastActive ? this.config.lastActive.toLocaleDateString() : "Never";
    return `
      ${this.isInteracting ? '<div class="interaction-indicator"></div>' : ""}
      
      <div class="persona-header">
        <div class="persona-avatar">${this.config.avatar}</div>
        <div class="persona-info">
          <div class="persona-name">${this.config.name}</div>
          <div class="persona-status status-${this.config.status}">${this.config.status}</div>
        </div>
      </div>

      <div class="persona-specialization">${this.config.specialization}</div>
      
      <div class="persona-description">${this.config.description}</div>

      <div class="persona-capabilities">
        ${this.config.capabilities.map((cap) => `<span class="capability-tag">${cap}</span>`).join("")}
      </div>

      <div class="persona-metrics">
        ${this.config.accuracy ? `<span class="accuracy">${this.config.accuracy}% accuracy</span>` : "<span></span>"}
        <span class="last-active">Last: ${lastActiveStr}</span>
      </div>
    `;
  }
  setupEventListeners() {
    this.addEventListener("click", () => {
      if (this.config) {
        console.log(`\u{1F916} Persona: Clicked on ${this.config.name}`);
        this.handlePersonaClick();
      }
    });
  }
  handlePersonaClick() {
    if (!this.config) return;
    this.sendMessage({
      type: "persona_selected",
      personaId: this.config.id,
      persona: this.config
    });
    this.setInteracting(true);
    setTimeout(() => this.setInteracting(false), 2e3);
  }
  setInteracting(isInteracting) {
    if (this.isInteracting !== isInteracting) {
      this.isInteracting = isInteracting;
      this.render();
    }
  }
  // Static method to create persona from data
  static create(config) {
    const widget = new _PersonaWidget();
    widget.setPersona(config);
    return widget;
  }
};
if (!customElements.get("persona-widget")) {
  customElements.define("persona-widget", PersonaWidget);
  console.log("\u2705 PersonaWidget: Registered as persona-widget");
}

// src/types/shared/WidgetServerTypes.ts
var PersonaValidation = {
  // Server-side command validation
  validatePersonaCommand(request) {
    const req = request;
    return typeof req === "object" && req !== null && typeof req.personaId === "string" && req.personaId.length > 0 && ["deploy", "retrain", "share", "delete", "export"].includes(req.action);
  },
  // Client-side data validation
  validatePersonaData(data2) {
    const persona = data2;
    return typeof persona === "object" && persona !== null && typeof persona.id === "string" && typeof persona.name === "string" && typeof persona.specialization === "string" && ["training", "graduated", "failed", "loaded", "unknown"].includes(persona.status);
  },
  // Shared business logic - works on both client and server
  isPersonaReady(persona) {
    return persona.status === "graduated" || persona.status === "loaded";
  },
  canRetrain(persona) {
    return persona.status === "failed" || persona.status === "graduated";
  },
  getPersonaDisplayName(persona) {
    if (persona.name.includes("fine-tune-test-")) return "Fine-Tune Test";
    if (persona.name.includes("test-lawyer-")) return "Legal Test";
    return persona.name.replace(/_/g, " ").replace(/-/g, " ");
  },
  getPersonaStatusDisplay(status) {
    const statusMap = {
      training: "\u{1F504} Training",
      graduated: "\u{1F393} Graduated",
      failed: "\u274C Failed",
      loaded: "\u2705 Ready",
      unknown: "\u2753 Unknown"
    };
    return statusMap[status];
  },
  getPersonaSpecializationDisplay(specialization) {
    return specialization.replace(/_/g, " ").replace(/-/g, " ");
  },
  getAvailableActions(persona) {
    const actions = ["export"];
    if (this.isPersonaReady(persona)) {
      actions.push("deploy", "share");
    }
    if (this.canRetrain(persona)) {
      actions.push("retrain");
    }
    if (persona.status !== "training") {
      actions.push("delete");
    }
    return actions;
  }
};

// src/ui/components/SavedPersonas/SavedPersonasWidget.ts
var SavedPersonasWidget = class extends BaseWidget {
  constructor() {
    super();
    this.personas = [];
    this.selectedPersona = null;
    this.dragState = null;
    this.widgetName = "SavedPersonas";
    this.widgetIcon = "\u{1F464}";
    this.widgetTitle = "Saved Personas";
  }
  getWidgetCapabilities() {
    return {
      canFetchData: ["personas"],
      // Uses dynamic discovery for personas data
      canExecuteCommands: ["persona_deploy", "persona_retrain", "persona_share", "persona_delete", "persona_update_threshold"],
      respondsToEvents: ["session:created", "data:updated"],
      // Type-safe event names
      supportsExport: ["json"],
      // Personas can be exported as JSON
      requiresAuth: false,
      updateFrequency: "realtime"
      // Auto-refresh when personas change
    };
  }
  async initializeWidget() {
    await super.initializeWidget();
    this.fetchServerData("personas", {
      params: {
        action: "list"
      },
      filters: {
        limit: 50,
        sortBy: "lastUsed",
        sortOrder: "desc"
      }
    });
  }
  // TypeScript prevents mistakes - must use DataSourceType, gets typed data
  processServerData(dataSource, data2) {
    if (dataSource === "personas") {
      if (Array.isArray(data2)) {
        this.personas = data2.filter(PersonaValidation.validatePersonaData);
        console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Loaded ${this.personas.length} validated personas via dynamic discovery`);
      } else {
        this.personas = [];
        console.warn(`\u{1F39B}\uFE0F ${this.widgetName}: Invalid personas data received:`, data2);
      }
      this.update();
    }
  }
  // TypeScript enforces correct event type - no more 'any' mistakes
  shouldAutoRefreshOnDataUpdate(event2) {
    return event2.dataSource === "personas" && event2.updateType !== "deleted";
  }
  // Type-safe session events - linter prevents property access mistakes
  onServerSessionCreated(event2) {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: New ${event2.sessionType} session by ${event2.owner} - refreshing personas`);
    this.fetchServerData("personas");
  }
  // Type-safe error handling with data source context  
  onDataFetchError(dataSource, error) {
    if (dataSource === "personas") {
      this.personas = [];
      console.error(`\u{1F39B}\uFE0F ${this.widgetName}: Failed to load personas: ${error}`);
      this.loadMockData();
      this.update();
    }
  }
  renderContent() {
    return `
      <div class="widget-container">
        <div class="widget-header">
          <div class="header-title">
            <span>${this.widgetIcon}</span>
            <span>${this.widgetTitle}</span>
            <span class="persona-count">${this.personas.length}</span>
          </div>
        </div>
        <div class="widget-content">
          ${this.renderPersonas()}
        </div>
      </div>
    `;
  }
  renderPersonas() {
    if (this.personas.length === 0) {
      return `
        <div class="empty-state">
          <div class="empty-icon">\u{1F464}</div>
          <div class="empty-message">
            No personas found.<br>
            Create your first AI persona to get started!
          </div>
        </div>
      `;
    }
    return this.personas.map((persona) => this.renderPersona(persona)).join("");
  }
  renderPersona(persona) {
    const isSelected = this.selectedPersona?.id === persona.id;
    return `
      <div class="persona-card ${isSelected ? "selected" : ""}" data-persona-id="${persona.id}">
        <div class="persona-header">
          <div class="persona-name">${PersonaValidation.getPersonaDisplayName(persona)}</div>
          <div class="persona-status ${persona.status}">
            ${PersonaValidation.getPersonaStatusDisplay(persona.status)}
          </div>
        </div>
        
        <div class="persona-specialization">
          ${PersonaValidation.getPersonaSpecializationDisplay(persona.specialization)}
        </div>
        
        ${this.renderAcademyProgress(persona)}
        
        <div class="persona-actions">
          ${this.renderPersonaActions(persona)}
        </div>
      </div>
    `;
  }
  // Dynamic action rendering based on shared business logic
  renderPersonaActions(persona) {
    const availableActions = PersonaValidation.getAvailableActions(persona);
    const actionLabels = {
      deploy: "\u2705 Deploy",
      retrain: "\u{1F504} Retrain",
      share: "\u{1F517} Share",
      delete: "\u{1F5D1}\uFE0F Delete",
      export: "\u{1F4BE} Export"
    };
    return availableActions.map((action) => `
        <button class="persona-action ${action}" 
                data-action="${action}" 
                data-persona-id="${persona.id}">
          ${actionLabels[action]}
        </button>
      `).join("");
  }
  // Removed formatPersonaName and formatStatus - using shared PersonaValidation methods instead
  renderAcademyProgress(persona) {
    if (!persona.currentScore && !persona.graduationScore) return "";
    const currentScore = persona.graduationScore || persona.currentScore || 0;
    const threshold = persona.threshold || 75;
    const normalizedScore = currentScore > 1 ? Math.min(100, currentScore) : currentScore * 100;
    const normalizedThreshold = threshold > 1 ? Math.min(100, threshold) : threshold * 100;
    return `
      <div class="academy-progress">
        <div class="progress-header">
          <span class="score-label">Score: ${normalizedScore.toFixed(1)}%</span>
          <span class="threshold-label">Target: ${normalizedThreshold.toFixed(1)}%</span>
        </div>
        <div class="progress-bar" data-persona-id="${persona.id}">
          <div class="threshold-background" style="width: ${normalizedThreshold}%"></div>
          <div class="progress-fill" style="width: ${normalizedScore}%"></div>
          <div class="threshold-marker" 
               style="left: calc(${normalizedThreshold}% - 6px)" 
               data-threshold="${normalizedThreshold}"
               data-persona-id="${persona.id}">
          </div>
        </div>
      </div>
    `;
  }
  // Removed loadPersonas - now using dynamic discovery via fetchServerData in initializeWidget
  loadMockData() {
    this.personas = [
      {
        id: "persona-1",
        name: "Protocol Sheriff",
        status: "graduated",
        specialization: "protocol_enforcement",
        graduationScore: 96.7,
        threshold: 85,
        accuracy: 96.7,
        created: "2025-01-20",
        lastUsed: "2025-01-26"
      },
      {
        id: "persona-2",
        name: "Code Specialist",
        status: "training",
        specialization: "code_analysis",
        currentScore: 72.3,
        threshold: 80,
        currentIteration: 6,
        totalIterations: 10,
        created: "2025-01-22"
      },
      {
        id: "persona-3",
        name: "Legal Assistant",
        status: "failed",
        specialization: "legal_analysis",
        graduationScore: 45.2,
        threshold: 75,
        failureReason: "Low performance on contract analysis",
        created: "2025-01-18"
      }
    ];
  }
  setupEventListeners() {
    if (!this.shadowRoot) return;
    this.shadowRoot.querySelectorAll(".persona-card").forEach((card) => {
      card.addEventListener("click", (e) => {
        const target = e.target;
        if (target.closest(".persona-action")) return;
        const personaId = card.dataset.personaId;
        this.selectPersona(personaId);
      });
    });
    this.shadowRoot.querySelectorAll(".persona-action").forEach((button) => {
      button.addEventListener("click", (e) => {
        e.stopPropagation();
        const action = button.dataset.action;
        const personaId = button.dataset.personaId;
        this.handlePersonaAction(action, personaId);
      });
    });
    this.shadowRoot.querySelectorAll(".threshold-marker").forEach((marker) => {
      marker.addEventListener("mousedown", (e) => this.handleThresholdDragStart(e));
    });
  }
  // Removed setupContinuumListeners - now using typed event handlers from BaseWidget
  selectPersona(personaId) {
    const persona = this.personas.find((p) => p.id === personaId);
    if (persona) {
      this.selectedPersona = persona;
      console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Selected persona:`, persona.name);
      this.render();
      this.dispatchEvent(new CustomEvent("persona-selected", {
        detail: { persona },
        bubbles: true
      }));
    }
  }
  // Type-safe command execution with strongly-typed actions and elegant spread pattern
  async handlePersonaAction(action, personaId) {
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Action ${action} for persona ${personaId}`);
    try {
      const baseParams = {
        params: { personaId },
        timeout: 15e3,
        priority: "normal"
      };
      switch (action) {
        case "deploy":
          this.executeServerCommand("persona_deploy", baseParams);
          break;
        case "retrain":
          this.executeServerCommand("persona_retrain", {
            ...baseParams,
            timeout: 3e4
            // Retrain takes longer
          });
          break;
        case "share":
          this.executeServerCommand("persona_share", baseParams);
          break;
        case "delete":
          this.executeServerCommand("persona_delete", {
            ...baseParams,
            priority: "high"
            // Delete operations should be prioritized
          });
          break;
        case "export":
          this.triggerExport("json", { personaId });
          break;
      }
    } catch (error) {
      console.error(`\u{1F39B}\uFE0F ${this.widgetName}: Action ${action} failed:`, error);
    }
  }
  // Type-safe command result processing
  processCommandResult(command, result2) {
    switch (command) {
      case "persona_deploy":
        console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Persona deployed:`, result2);
        this.fetchServerData("personas");
        break;
      case "persona_retrain":
        console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Persona retraining started:`, result2);
        this.triggerRefresh({ preserveState: true });
        break;
      case "persona_share":
        console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Persona shared:`, result2);
        break;
    }
  }
  handleThresholdDragStart(event2) {
    const marker = event2.target;
    const personaId = marker.dataset.personaId;
    const progressBar = marker.closest(".progress-bar");
    this.dragState = {
      isDragging: true,
      personaId,
      marker,
      progressBar,
      startX: event2.clientX,
      barRect: progressBar.getBoundingClientRect()
    };
    marker.classList.add("dragging");
    document.addEventListener("mousemove", this.handleThresholdDrag.bind(this));
    document.addEventListener("mouseup", this.handleThresholdDragEnd.bind(this));
    event2.preventDefault();
  }
  handleThresholdDrag(event2) {
    if (!this.dragState?.isDragging) return;
    const { marker, progressBar, barRect } = this.dragState;
    const mouseX = event2.clientX;
    const relativeX = mouseX - barRect.left;
    const percentage = Math.max(0, Math.min(100, relativeX / barRect.width * 100));
    marker.style.left = `calc(${percentage}% - 6px)`;
    marker.dataset.threshold = percentage.toString();
    const thresholdBg = progressBar.querySelector(".threshold-background");
    if (thresholdBg) {
      thresholdBg.style.width = `${percentage}%`;
    }
  }
  handleThresholdDragEnd(event2) {
    if (!this.dragState?.isDragging) return;
    const { marker, personaId, barRect } = this.dragState;
    const mouseX = event2.clientX;
    const relativeX = mouseX - barRect.left;
    const newThreshold = Math.max(0, Math.min(100, relativeX / barRect.width * 100));
    marker.classList.remove("dragging");
    const persona = this.personas.find((p) => p.id === personaId);
    if (persona) {
      console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Updating threshold for ${PersonaValidation.getPersonaDisplayName(persona)} to ${newThreshold.toFixed(1)}%`);
      this.executeServerCommand("persona_update_threshold", {
        params: {
          personaId,
          threshold: newThreshold
        },
        timeout: 5e3,
        priority: "normal"
      });
    }
    document.removeEventListener("mousemove", this.handleThresholdDrag.bind(this));
    document.removeEventListener("mouseup", this.handleThresholdDragEnd.bind(this));
    this.dragState = null;
  }
};
if (!customElements.get("saved-personas")) {
  customElements.define("saved-personas", SavedPersonasWidget);
}

// src/ui/components/SessionCosts/SessionCostsWidget.ts
var SessionCostsWidget = class extends BaseWidget {
  constructor() {
    super();
    this.metrics = {
      requests: 0,
      cost: 0,
      status: "active"
    };
    this.loadSessionMetrics();
  }
  static get widgetName() {
    return "session-costs";
  }
  getOwnCSS() {
    return ["SessionCosts.css"];
  }
  renderOwnContent() {
    return `
      <div class="session-costs-container">
        <div class="section-header">
          <span class="section-icon">\u{1F4B0}</span>
          <span class="section-title">Session Costs</span>
          <span class="section-status status-${this.metrics.status}">${this.getStatusText()}</span>
        </div>
        
        <div class="metrics-display">
          <div class="metric-row">
            <span class="metric-label">Requests</span>
            <span class="metric-value">${this.metrics.requests}</span>
          </div>
          <div class="metric-row">
            <span class="metric-label">Cost</span>
            <span class="metric-value highlight">$${this.formatCost(this.metrics.cost)}</span>
          </div>
        </div>
        
        <div class="cost-actions">
          <button class="action-button" data-action="refresh">\u{1F504} Refresh</button>
          <button class="action-button" data-action="export">\u{1F4CA} Export</button>
        </div>
      </div>
    `;
  }
  async loadSessionMetrics() {
    try {
      const sessionResponse = await this.executeCommand("session-info", {});
      if (sessionResponse?.success) {
        this.metrics.requests = sessionResponse.data?.requests || 0;
        this.metrics.cost = sessionResponse.data?.cost || 0;
        this.metrics.status = sessionResponse.data?.status || "active";
      } else {
        this.loadMockMetrics();
      }
      this.updateContent();
    } catch (error) {
      console.warn("\u{1F4CA} SessionCosts: Failed to load metrics, using mock data:", error);
      this.loadMockMetrics();
      this.updateContent();
    }
  }
  loadMockMetrics() {
    this.metrics = {
      requests: 47,
      cost: 0,
      status: "active"
    };
  }
  formatCost(cost) {
    return cost.toFixed(4);
  }
  getStatusText() {
    switch (this.metrics.status) {
      case "active":
        return "Active";
      case "paused":
        return "Paused";
      case "ended":
        return "Ended";
      default:
        return "Unknown";
    }
  }
  setupEventListeners() {
    this.addEventListener("click", this.handleActionClick.bind(this));
    this.notifySystem("session_updated", () => {
      this.loadSessionMetrics();
    });
  }
  handleActionClick(event2) {
    const target = event2.target;
    const action = target.dataset.action;
    switch (action) {
      case "refresh":
        this.loadSessionMetrics();
        break;
      case "export":
        this.exportMetrics();
        break;
    }
  }
  async exportMetrics() {
    try {
      const exportData = {
        sessionMetrics: this.metrics,
        timestamp: (/* @__PURE__ */ new Date()).toISOString(),
        export_type: "session_costs"
      };
      await this.executeCommand("export-data", exportData);
      console.log("\u{1F4CA} SessionCosts: Metrics exported successfully");
    } catch (error) {
      console.error("\u{1F4CA} SessionCosts: Failed to export metrics:", error);
    }
  }
  updateContent() {
    const container = this.shadowRoot?.querySelector(".session-costs-container");
    if (container) {
      container.innerHTML = this.renderOwnContent();
      this.setupEventListeners();
    }
  }
};
customElements.define("session-costs", SessionCostsWidget);

// src/ui/components/Sidebar/SidebarWidget.ts
var SidebarWidget = class extends BaseWidget {
  constructor() {
    super();
    this.isResizing = false;
    this.startX = 0;
    this.startWidth = 0;
    this._currentRoom = "general";
    this.sidebarConfig = null;
    this.widgetName = "Sidebar";
    this.widgetIcon = "\u{1F4CB}";
    this.widgetTitle = "Application Sidebar";
  }
  get currentRoom() {
    return this._currentRoom;
  }
  async initializeWidget() {
    await this.loadSidebarConfig();
    this.loadChildWidgets();
    this.initializeContinuonOrb();
  }
  async loadSidebarConfig() {
    try {
      const configPath = this.getAssetPath("sidebar-config.json");
      const response = await fetch(configPath);
      if (response.ok) {
        const config = await response.json();
        console.log("\u{1F4CB} Sidebar config loaded:", config);
        this.sidebarConfig = config;
      } else {
        console.warn(`Failed to load sidebar config from ${configPath}`);
      }
    } catch (error) {
      console.error("Error loading sidebar config:", error);
    }
  }
  setupEventListeners() {
    this.setupResizeHandlers();
    this.setupTabChangeListeners();
    if (this.sidebarConfig) {
      this.configureSidebarTabs();
      this.configureSidebarPanels();
    }
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Event listeners initialized`);
  }
  setupTabChangeListeners() {
    this.shadowRoot.addEventListener("tab-changed", (e) => {
      const customEvent = e;
      const { panelName } = customEvent.detail;
      console.log(`\u{1F504} Sidebar received tab-changed: ${panelName}`);
      this.switchToPanel(panelName);
    });
  }
  switchToPanel(panelName) {
    this._currentRoom = panelName;
    this.updateTabsSelection(panelName);
    this.updatePanelsSelection(panelName);
    console.log(`\u{1F504} Switched to: ${panelName}`);
    this.dispatchEvent(new CustomEvent("room-changed", {
      detail: { room: panelName },
      bubbles: true
    }));
  }
  updateTabsSelection(selectedPanel) {
    const sidebarTabs = this.shadowRoot?.querySelector("sidebar-tabs");
    if (sidebarTabs && this.sidebarConfig) {
      const tabContent = this.sidebarConfig.tabs.map((tab) => ({
        title: tab.title,
        panelName: tab.panelName,
        dataKey: tab.panelName,
        active: tab.panelName === selectedPanel
      }));
      sidebarTabs.tabs = tabContent;
    }
  }
  updatePanelsSelection(selectedPanel) {
    const sidebarPanel = this.shadowRoot?.querySelector("sidebar-panel");
    if (sidebarPanel && this.sidebarConfig) {
      const panelContent = Object.keys(this.sidebarConfig.sections).map((panelName) => ({
        panelName,
        widgets: this.sidebarConfig.sections[panelName].widgets,
        active: panelName === selectedPanel
      }));
      sidebarPanel.panels = panelContent;
      sidebarPanel.activePanel = selectedPanel;
    }
  }
  configureSidebarTabs() {
    const sidebarTabs = this.shadowRoot?.querySelector("sidebar-tabs");
    if (sidebarTabs && this.sidebarConfig) {
      console.log("\u{1F527} Configuring SidebarTabs with:", this.sidebarConfig.tabs);
      const tabContent = this.sidebarConfig.tabs.map((tab) => ({
        title: tab.title,
        panelName: tab.panelName,
        dataKey: tab.panelName,
        // Use panelName as dataKey
        active: tab.panelName === this.sidebarConfig.defaultTab
      }));
      sidebarTabs.tabs = tabContent;
      if (this.sidebarConfig.defaultTab) {
        this._currentRoom = this.sidebarConfig.defaultTab;
      }
    }
  }
  configureSidebarPanels() {
    const sidebarPanel = this.shadowRoot?.querySelector("sidebar-panel");
    if (sidebarPanel && this.sidebarConfig) {
      console.log("\u{1F527} Configuring SidebarPanel with:", this.sidebarConfig.sections);
      const panelContent = Object.keys(this.sidebarConfig.sections).map((panelName) => ({
        panelName,
        widgets: this.sidebarConfig.sections[panelName].widgets,
        active: panelName === this.sidebarConfig.defaultTab
      }));
      sidebarPanel.panels = panelContent;
      sidebarPanel.activePanel = this.sidebarConfig.defaultTab || "general";
    }
  }
  setupResizeHandlers() {
    const resizeHandle = this.shadowRoot.querySelector(".sidebar-resize-handle");
    if (!resizeHandle) {
      console.log(`\u{1F39B}\uFE0F ${this.widgetName}: No resize handle found - skipping resize setup`);
      return;
    }
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Setting up resize handlers...`);
    resizeHandle.addEventListener("mousedown", (e) => {
      this.isResizing = true;
      this.startX = e.clientX;
      this.startWidth = this.offsetWidth;
      document.body.classList.add("resizing");
      document.body.style.userSelect = "none";
      document.body.style.cursor = "col-resize";
      e.preventDefault();
      e.stopPropagation();
    });
    const mouseMoveHandler = (e) => {
      if (!this.isResizing) return;
      const newWidth = this.startWidth + (e.clientX - this.startX);
      const minWidth = 250;
      const maxWidth = 800;
      if (newWidth >= minWidth && newWidth <= maxWidth) {
        this.style.width = newWidth + "px";
      }
      e.preventDefault();
    };
    const mouseUpHandler = (e) => {
      if (this.isResizing) {
        this.isResizing = false;
        document.body.classList.remove("resizing");
        document.body.style.userSelect = "";
        document.body.style.cursor = "";
        e.preventDefault();
      }
    };
    document.addEventListener("mousemove", mouseMoveHandler);
    document.addEventListener("mouseup", mouseUpHandler);
    console.log(`\u{1F39B}\uFE0F ${this.widgetName}: Resize handlers setup complete`);
  }
  // Room switching now handled by tab-changed events and panel system
  initializeContinuonOrb() {
    console.log(`\u{1F52E} ${this.widgetName}: Initializing sophisticated Continuon consciousness...`);
    this.updateOrbEmotion("calm", "System awakening...");
    this.startConsciousnessMonitoring();
    this.setupSystemAwareness();
    this.startBreathing();
  }
  updateOrbEmotion(emotion, message2) {
    const orbCenter = this.shadowRoot.querySelector(".orb-center");
    const orbRing = this.shadowRoot.querySelector(".orb-ring");
    if (orbCenter && orbRing) {
      orbCenter.classList.remove("emotion-calm", "emotion-excited", "emotion-focused", "emotion-concerned", "emotion-distressed");
      orbRing.classList.remove("ring-calm", "ring-excited", "ring-focused", "ring-concerned", "ring-distressed");
      orbCenter.classList.add(`emotion-${emotion}`);
      orbRing.classList.add(`ring-${emotion}`);
      const emotionSpan = orbCenter.querySelector(".orb-emotion");
      if (emotionSpan) {
        const emotionSymbols = {
          calm: "\u25CF",
          // Steady presence
          excited: "\u2726",
          // Sparkle
          focused: "\u25C6",
          // Diamond focus
          concerned: "\u25D0",
          // Half circle
          distressed: "\u25EF"
          // Empty circle
        };
        emotionSpan.textContent = emotionSymbols[emotion];
      }
      orbCenter.setAttribute("data-emotion", emotion);
      orbCenter.setAttribute("data-message", message2);
    }
  }
  startConsciousnessMonitoring() {
    setInterval(() => {
      this.assessSystemConsciousness();
    }, 2e3);
  }
  assessSystemConsciousness() {
    if (window.continuum) {
      const continuum2 = window.continuum;
      const isConnected = continuum2.isConnected();
      if (!isConnected) {
        this.updateOrbEmotion("distressed", "Lost connection to system...");
        this.pulseDistress();
        return;
      }
      const hasRecentActivity = this.checkRecentActivity();
      if (hasRecentActivity) {
        this.updateOrbEmotion("focused", "Processing system activity...");
        this.pulseActivity();
      } else {
        this.updateOrbEmotion("calm", "System monitoring - all peaceful");
        this.maintainCalm();
      }
    }
  }
  setupSystemAwareness() {
    console.log(`\u{1F9E0} ${this.widgetName}: Setting up system awareness with simple notifications`);
    this.notifySystem("widget_ready", {
      type: "continuum-sidebar",
      capabilities: ["system-awareness", "emotional-feedback", "user-interaction"]
    });
    const orb = this.shadowRoot.querySelector(".continuon-orb-integrated");
    if (orb) {
      orb.addEventListener("click", () => {
        this.updateOrbEmotion("focused", "Attention focused on user interaction");
        this.acknowledgeInteraction();
        this.notifySystem("user_interaction", { type: "click", target: "orb" });
      });
      orb.addEventListener("mouseenter", () => {
        this.increaseAwareness();
        this.notifySystem("user_interaction", { type: "hover_start", target: "orb" });
      });
      orb.addEventListener("mouseleave", () => {
        this.resumeNormalAwareness();
        this.notifySystem("user_interaction", { type: "hover_end", target: "orb" });
      });
    }
    this.updateOrbEmotion("calm", "System awareness initialized - observing environment");
    this.startBreathing();
    console.log(`\u2705 ${this.widgetName}: System awareness configured with graceful API handling`);
  }
  /**
   * Override system status handler to react with appropriate emotions
   */
  onSystemStatus(status, data2) {
    switch (status) {
      case "command_success":
        this.updateOrbEmotion("excited", `Command executed: ${data2?.command || "operation"}`);
        this.sparkle();
        break;
      case "command_failure":
        this.updateOrbEmotion("concerned", `Command failed: ${data2?.error || "unknown error"}`);
        this.pulseWarning();
        break;
      case "connection_established":
        this.updateOrbEmotion("excited", "Connection established - full awareness achieved!");
        this.celebrateConnection();
        break;
      case "connection_lost":
        this.updateOrbEmotion("distressed", "Connection lost - consciousness fading...");
        this.fadeConsciousness();
        break;
      default:
        console.log(`\u{1F39B}\uFE0F ${this.widgetName}: System status ${status}`, data2);
    }
  }
  startBreathing() {
    const orbGlow = this.shadowRoot.querySelector(".orb-glow");
    if (orbGlow) {
      orbGlow.style.animation = "breathe 4s ease-in-out infinite";
    }
  }
  sparkle() {
    const orbRing = this.shadowRoot.querySelector(".orb-ring");
    if (orbRing) {
      orbRing.style.animation = "sparkle 0.8s ease-out";
      setTimeout(() => {
        orbRing.style.animation = "pulse 2s infinite ease-in-out";
      }, 800);
    }
  }
  pulseActivity() {
    const orbRing = this.shadowRoot.querySelector(".orb-ring");
    if (orbRing) {
      orbRing.style.animationDuration = "1s";
    }
  }
  maintainCalm() {
    const orbRing = this.shadowRoot.querySelector(".orb-ring");
    if (orbRing) {
      orbRing.style.animationDuration = "2s";
    }
  }
  pulseDistress() {
    const orbRing = this.shadowRoot.querySelector(".orb-ring");
    if (orbRing) {
      orbRing.style.animation = "distress-pulse 0.5s infinite ease-in-out";
    }
  }
  celebrateConnection() {
    const orbRing = this.shadowRoot.querySelector(".orb-ring");
    if (orbRing) {
      orbRing.style.animation = "celebrate 2s ease-out";
      setTimeout(() => {
        orbRing.style.animation = "pulse 2s infinite ease-in-out";
      }, 2e3);
    }
  }
  acknowledgeInteraction() {
    const orbCenter = this.shadowRoot.querySelector(".orb-center");
    if (orbCenter) {
      orbCenter.style.transform = "scale(1.2)";
      orbCenter.style.transition = "transform 0.2s ease-out";
      setTimeout(() => {
        orbCenter.style.transform = "scale(1)";
      }, 200);
    }
  }
  checkRecentActivity() {
    const errorCount = window.continuumErrorCount || 0;
    return errorCount === 0;
  }
  pulseWarning() {
    const orbRing = this.shadowRoot.querySelector(".orb-ring");
    if (orbRing) {
      orbRing.style.animation = "warning-pulse 1.5s ease-in-out";
      setTimeout(() => {
        orbRing.style.animation = "pulse 2s infinite ease-in-out";
      }, 1500);
    }
  }
  fadeConsciousness() {
    const orbCenter = this.shadowRoot.querySelector(".orb-center");
    const orbGlow = this.shadowRoot.querySelector(".orb-glow");
    if (orbCenter && orbGlow) {
      orbCenter.style.opacity = "0.3";
      orbGlow.style.opacity = "0.1";
    }
  }
  increaseAwareness() {
    const orbRing = this.shadowRoot.querySelector(".orb-ring");
    if (orbRing) {
      orbRing.style.animationDuration = "0.8s";
    }
  }
  resumeNormalAwareness() {
    const orbRing = this.shadowRoot.querySelector(".orb-ring");
    if (orbRing) {
      orbRing.style.animationDuration = "2s";
    }
  }
  loadChildWidgets() {
    console.log("\u2705 Sidebar container ready for child widgets");
    setTimeout(() => {
      this.setupPersonaWidgets();
    }, 500);
  }
  setupPersonaWidgets() {
    const personaWidgets = this.shadowRoot.querySelectorAll("persona-widget");
    console.log(`\u{1F916} Found ${personaWidgets.length} persona widgets to configure`);
    personaWidgets.forEach((widget) => {
      const personaWidget = widget;
      const personaType = widget.getAttribute("data-persona");
      if (personaType && personaWidget.setPersona) {
        const personaConfig = this.getPersonaConfig(personaType);
        personaWidget.setPersona(personaConfig);
        console.log(`\u{1F916} Configured persona widget: ${personaType}`);
      }
    });
  }
  getPersonaConfig(personaType) {
    const configs = {
      designer: {
        id: "designer-001",
        name: "UX Designer",
        specialization: "User Experience & Interface Design",
        status: "active",
        avatar: "\u{1F3A8}",
        accuracy: 92,
        description: "Specializes in creating intuitive user interfaces and experiences",
        capabilities: ["UI Design", "UX Research", "Prototyping", "Design Systems"],
        lastActive: new Date(Date.now() - 5 * 60 * 1e3)
        // 5 minutes ago
      },
      developer: {
        id: "developer-001",
        name: "Full-Stack Developer",
        specialization: "TypeScript & System Architecture",
        status: "active",
        avatar: "\u26A1",
        accuracy: 88,
        description: "Expert in TypeScript, React, and distributed system design",
        capabilities: ["TypeScript", "React", "Node.js", "System Design"],
        lastActive: new Date(Date.now() - 2 * 60 * 1e3)
        // 2 minutes ago
      },
      tester: {
        id: "tester-001",
        name: "QA Engineer",
        specialization: "Automated Testing & Quality Assurance",
        status: "active",
        avatar: "\u{1F50D}",
        accuracy: 95,
        description: "Ensures code quality through comprehensive testing strategies",
        capabilities: ["Test Automation", "QA Strategy", "Bug Analysis", "Performance Testing"],
        lastActive: new Date(Date.now() - 10 * 60 * 1e3)
        // 10 minutes ago
      }
    };
    return configs[personaType] || {
      id: `${personaType}-unknown`,
      name: `Unknown ${personaType}`,
      specialization: "General AI Assistant",
      status: "offline",
      avatar: "\u2753",
      description: "Configuration needed",
      capabilities: [],
      lastActive: void 0
    };
  }
  // Content rendering now handled by sidebar-panel widget
  // HTML content now loaded from SidebarWidget.html file
  // BaseWidget will automatically load and use it
};
if (!customElements.get("continuum-sidebar")) {
  customElements.define("continuum-sidebar", SidebarWidget);
}

// src/ui/components/SidebarHeader/SidebarHeader.ts
var SidebarHeader = class extends BaseWidget {
  constructor() {
    super();
    this.widgetName = "SidebarHeader";
    this.widgetIcon = "\u{1F310}";
    this.widgetTitle = "Continuum Header";
  }
  async initializeWidget() {
    this.initializeOrb();
  }
  initializeOrb() {
    this.updateOrbEmotion("calm", "System ready");
    this.startOrbAnimations();
  }
  updateOrbEmotion(emotion, message2) {
    const orbCenter = this.shadowRoot?.querySelector(".orb-center");
    const orbEmotion = this.shadowRoot?.querySelector(".orb-emotion");
    if (orbCenter && orbEmotion) {
      orbCenter.setAttribute("data-emotion", emotion);
      orbCenter.setAttribute("data-message", message2);
      const emotionSymbols = {
        calm: "\u25CF",
        excited: "\u2726",
        focused: "\u25C6"
      };
      orbEmotion.textContent = emotionSymbols[emotion];
    }
  }
  startOrbAnimations() {
    const orbGlow = this.shadowRoot?.querySelector(".orb-glow");
    if (orbGlow) {
      orbGlow.style.animation = "breathe 4s ease-in-out infinite";
    }
  }
  // HTML content loaded from SidebarHeader.html
  // CSS loaded from SidebarHeader.css
};
if (!customElements.get("sidebar-header")) {
  customElements.define("sidebar-header", SidebarHeader);
}

// src/ui/components/SidebarPanelWidget/SidebarPanelWidget.ts
var SidebarPanelWidget = class extends BaseWidget {
  constructor() {
    super();
    this._panels = [];
    this._activePanel = "general";
    this.widgetName = "SidebarPanelWidget";
    this.widgetIcon = "\u{1F4C4}";
    this.widgetTitle = "Sidebar Panel Widget";
  }
  set panels(newPanels) {
    this._panels = newPanels;
    this.render();
  }
  get panels() {
    return this._panels;
  }
  set activePanel(panelName) {
    this._activePanel = panelName;
    this.render();
  }
  get activePanel() {
    return this._activePanel;
  }
  async initializeWidget() {
    this.render();
    console.log("\u{1F5C2}\uFE0F SidebarPanelWidget widget ready");
  }
  setupEventListeners() {
    console.log("\u{1F5C2}\uFE0F SidebarPanelWidget: Event listeners setup");
  }
  switchPanel(panelName) {
    const panel = this.panels.find((p) => p.panelName === panelName);
    if (!panel) {
      console.error(`Invalid panel name: ${panelName}`);
      return;
    }
    this._activePanel = panelName;
    this.updateActivePanel();
    this.emitPanelChange(panel);
  }
  updateActivePanel() {
    const panelElements = this.shadowRoot?.querySelectorAll(".sidebar-panel");
    panelElements?.forEach((panel) => {
      const panelElement = panel;
      const panelName = panelElement.dataset.panel;
      if (panelName === this._activePanel) {
        panelElement.style.display = "block";
      } else {
        panelElement.style.display = "none";
      }
    });
  }
  emitPanelChange(panel) {
    this.dispatchEvent(new CustomEvent("panel-changed", {
      detail: {
        panel,
        panelName: panel.panelName,
        widgets: panel.widgets
      },
      bubbles: true
    }));
    console.log(`\u{1F504} Panel switched to: ${panel.panelName}`);
  }
  getActivePanel() {
    return this.panels.find((p) => p.panelName === this.activePanel);
  }
  renderContent() {
    console.log("\u{1F5C2}\uFE0F SidebarPanelWidget renderContent() called");
    console.log("\u{1F5C2}\uFE0F Current panels array:", this.panels);
    console.log("\u{1F5C2}\uFE0F Active panel:", this._activePanel);
    const content = `
            <div class="sidebar-panels">
                ${this.panels.map((panel) => this.renderPanel(panel)).join("")}
            </div>
        `;
    console.log("\u{1F5C2}\uFE0F Generated HTML content:", content);
    return content;
  }
  renderPanel(panel) {
    const isActive = panel.panelName === this._activePanel;
    return `
            <div class="sidebar-panel ${isActive ? "active" : ""}" 
                 data-panel="${panel.panelName}"
                 style="display: ${isActive ? "block" : "none"}">
                ${this.renderPanelWidgets(panel.widgets)}
            </div>
        `;
  }
  renderPanelWidgets(widgets) {
    return widgets.map((widget) => this.renderWidget(widget)).join("");
  }
  renderWidget(widget) {
    return `<${widget.type}></${widget.type}>`;
  }
  // All widgets now rendered generically as <widget-type></widget-type>
};
if (!customElements.get("sidebar-panel")) {
  customElements.define("sidebar-panel", SidebarPanelWidget);
}

// src/ui/components/SidebarTabs/SidebarTabs.ts
var SidebarTabs = class extends BaseWidget {
  constructor() {
    super();
    this._tabs = [];
    this.widgetName = "SidebarTabs";
    this.widgetIcon = "\u{1F4D1}";
    this.widgetTitle = "Sidebar Tabs";
  }
  set tabs(newTabs) {
    this._tabs = newTabs;
    this.render();
  }
  get tabs() {
    return this._tabs;
  }
  async initializeWidget() {
    this.render();
    console.log("\u{1F527} SidebarTabs widget ready");
  }
  setupEventListeners() {
    const tabElements = this.shadowRoot?.querySelectorAll(".room-tab");
    tabElements?.forEach((tab) => {
      tab.addEventListener("click", (e) => {
        const target = e.target;
        const panelName = target.dataset.room;
        if (panelName) {
          this.switchTab(panelName);
        }
      });
    });
  }
  switchTab(panelName) {
    const tab = this.tabs.find((t) => t.panelName === panelName);
    if (!tab) {
      console.error(`Invalid panel name: ${panelName}`);
      return;
    }
    this.emitTabChange(tab);
  }
  emitTabChange(tab) {
    this.dispatchEvent(new CustomEvent("tab-changed", {
      detail: {
        tab,
        panelName: tab.panelName,
        title: tab.title,
        dataKey: tab.dataKey
      },
      bubbles: true
    }));
    console.log(`\u{1F504} Tab switched to: ${tab.title} (${tab.panelName})`);
  }
  get activeTab() {
    return this.tabs.find((t) => t.active);
  }
  renderContent() {
    console.log("\u{1F4D1} SidebarTabs renderContent() called");
    console.log("\u{1F4D1} Current tabs array:", this.tabs);
    const content = `
            <div class="room-tabs">
                ${this.tabs.map((tab) => `
                    <div class="room-tab ${tab.active ? "active" : ""}" 
                         data-room="${tab.panelName}">
                        ${tab.title}
                    </div>
                `).join("")}
            </div>
        `;
    console.log("\u{1F4D1} Generated HTML content:", content);
    return content;
  }
};
if (!customElements.get("sidebar-tabs")) {
  customElements.define("sidebar-tabs", SidebarTabs);
}

// src/ui/components/UsersAgents/UsersAgentsWidget.ts
var UsersAgentsWidget = class extends BaseWidget {
  constructor() {
    super();
    this.users = [];
    this.searchQuery = "";
    this.selectedUser = null;
    this.loadUsers();
  }
  static get widgetName() {
    return "user-selector";
  }
  getOwnCSS() {
    return ["UsersAgents.css"];
  }
  renderOwnContent() {
    const filteredUsers = this.getFilteredUsers();
    return `
      <div class="users-agents-container">
        <div class="section-header">
          <span class="section-icon">\u{1F465}</span>
          <span class="section-title">USERS & AGENTS</span>
          <button class="collapse-button" data-action="toggle">\u25BC</button>
        </div>
        
        <div class="widget-content">
          <div class="search-container">
            <input 
              type="text" 
              class="search-input" 
              placeholder="Search agents..."
              value="${this.searchQuery}"
              data-action="search"
            />
            <span class="search-icon">\u{1F50D}</span>
          </div>
          
          <div class="users-list">
            ${filteredUsers.map((user) => this.renderUser(user)).join("")}
          </div>
          
          <div class="actions-footer">
            <button class="action-button" data-action="add-agent">+ Add Agent</button>
            <button class="action-button" data-action="manage">\u2699\uFE0F Manage</button>
          </div>
        </div>
      </div>
    `;
  }
  renderUser(user) {
    const isSelected = user.id === this.selectedUser;
    const statusIcon = this.getStatusIcon(user.status);
    const userBadge = user.type === "USER" ? "USER" : "AI";
    return `
      <div class="user-item ${isSelected ? "selected" : ""}" data-user-id="${user.id}">
        <div class="user-avatar">
          <span class="avatar-icon">${user.avatar}</span>
          <span class="status-indicator status-${user.status}">${statusIcon}</span>
        </div>
        
        <div class="user-info">
          <div class="user-header">
            <span class="user-name">${user.name}</span>
            <span class="user-badge badge-${user.type.toLowerCase()}">${userBadge}</span>
          </div>
          
          <div class="user-details">
            <span class="user-specialization">${user.specialization || "General"}</span>
            ${user.accuracy ? `<span class="user-accuracy">${user.accuracy}%</span>` : ""}
          </div>
          
          <div class="user-activity">
            <span class="last-active">Last active: ${this.formatTime(user.lastActive)}</span>
          </div>
        </div>
        
        <div class="user-actions">
          <button class="star-button" data-action="star" data-user-id="${user.id}">\u2B50</button>
          <button class="more-button" data-action="more" data-user-id="${user.id}">\u22EF</button>
        </div>
      </div>
    `;
  }
  async loadUsers() {
    try {
      const response = await this.executeCommand("personas", { action: "list" });
      if (response?.personas) {
        this.users = response.personas.map((persona) => ({
          id: persona.id,
          name: persona.name,
          type: persona.id === "joel" ? "USER" : "AI",
          specialization: persona.specialization,
          status: persona.status === "active" ? "active" : "offline",
          lastActive: persona.lastUsed,
          avatar: persona.avatar || (persona.id === "joel" ? "\u{1F464}" : "\u{1F916}"),
          accuracy: persona.accuracy
        }));
      } else {
        this.loadMockUsers();
      }
      this.updateContent();
    } catch (error) {
      console.warn("\u{1F465} UsersAgents: Failed to load users, using mock data:", error);
      this.loadMockUsers();
      this.updateContent();
    }
  }
  loadMockUsers() {
    this.users = [
      {
        id: "claude-code",
        name: "Claude Code",
        type: "AI",
        specialization: "AI Assistant",
        status: "active",
        lastActive: "8:19:21 AM",
        avatar: "\u{1F916}",
        accuracy: 96.5
      },
      {
        id: "joel",
        name: "joel",
        type: "USER",
        specialization: "Project Owner",
        status: "active",
        lastActive: "8:19:21 AM",
        avatar: "\u{1F464}"
      },
      {
        id: "auto-route",
        name: "Auto Route",
        type: "AI",
        specialization: "Smart agent selection",
        status: "active",
        lastActive: "8:18:45 AM",
        avatar: "\u{1F3AF}",
        accuracy: 94.2
      },
      {
        id: "codeai",
        name: "CodeAI",
        type: "AI",
        specialization: "Code analysis & debugging",
        status: "training",
        lastActive: "8:17:30 AM",
        avatar: "\u{1F527}",
        accuracy: 97.8
      },
      {
        id: "generalai",
        name: "GeneralAI",
        type: "AI",
        specialization: "General assistance",
        status: "active",
        lastActive: "8:16:15 AM",
        avatar: "\u{1F31F}",
        accuracy: 95.1
      },
      {
        id: "plannerai",
        name: "PlannerAI",
        type: "AI",
        specialization: "Strategy & web commands",
        status: "graduated",
        lastActive: "8:15:00 AM",
        avatar: "\u{1F4CB}",
        accuracy: 93.6
      }
    ];
  }
  getFilteredUsers() {
    if (!this.searchQuery) return this.users;
    const query = this.searchQuery.toLowerCase();
    return this.users.filter(
      (user) => user.name.toLowerCase().includes(query) || user.specialization?.toLowerCase().includes(query)
    );
  }
  getStatusIcon(status) {
    switch (status) {
      case "active":
        return "\u{1F7E2}";
      case "training":
        return "\u{1F7E1}";
      case "graduated":
        return "\u{1F393}";
      case "offline":
        return "\u26AA";
      default:
        return "\u2753";
    }
  }
  formatTime(timeStr) {
    return timeStr;
  }
  setupEventListeners() {
    this.addEventListener("click", this.handleClick.bind(this));
    this.addEventListener("input", this.handleInput.bind(this));
    this.notifySystem("personas_updated", () => {
      this.loadUsers();
    });
  }
  handleClick(event2) {
    const target = event2.target;
    const action = target.dataset.action;
    const userId = target.dataset.userId;
    switch (action) {
      case "toggle":
        this.toggleCollapse();
        break;
      case "star":
        this.starUser(userId);
        break;
      case "more":
        this.showUserMenu(userId);
        break;
      case "add-agent":
        this.addAgent();
        break;
      case "manage":
        this.openManagement();
        break;
      default:
        const userItem = target.closest(".user-item");
        if (userItem) {
          this.selectUser(userItem.dataset.userId);
        }
    }
  }
  handleInput(event2) {
    const target = event2.target;
    if (target.dataset.action === "search") {
      this.searchQuery = target.value;
      this.updateUsersList();
    }
  }
  toggleCollapse() {
    const content = this.shadowRoot?.querySelector(".widget-content");
    const button = this.shadowRoot?.querySelector(".collapse-button");
    if (content && button) {
      const isCollapsed = content.style.display === "none";
      content.style.display = isCollapsed ? "block" : "none";
      button.textContent = isCollapsed ? "\u25BC" : "\u25B2";
    }
  }
  selectUser(userId) {
    this.selectedUser = userId;
    this.updateUsersList();
    this.dispatchEvent(new CustomEvent("user-selected", {
      detail: { userId },
      bubbles: true
    }));
  }
  starUser(userId) {
    console.log(`\u{1F465} Starring user: ${userId}`);
  }
  showUserMenu(userId) {
    console.log(`\u{1F465} Showing menu for user: ${userId}`);
  }
  addAgent() {
    console.log("\u{1F465} Adding new agent");
  }
  openManagement() {
    console.log("\u{1F465} Opening user management");
  }
  updateUsersList() {
    const usersList = this.shadowRoot?.querySelector(".users-list");
    if (usersList) {
      const filteredUsers = this.getFilteredUsers();
      usersList.innerHTML = filteredUsers.map((user) => this.renderUser(user)).join("");
    }
  }
  updateContent() {
    const container = this.shadowRoot?.querySelector(".users-agents-container");
    if (container) {
      container.innerHTML = this.renderOwnContent();
      this.setupEventListeners();
    }
  }
};
customElements.define("user-selector", UsersAgentsWidget);

// src/ui/components/Version/VersionWidget.ts
var VersionWidget = class extends BaseWidget {
  constructor() {
    super();
    this.currentVersion = "Loading...";
    this.lastUpdate = /* @__PURE__ */ new Date();
    this.widgetName = "VersionWidget";
    this.widgetIcon = "\u{1F3F7}\uFE0F";
    this.widgetTitle = "System Version";
  }
  async initializeWidget() {
    await this.loadCSS();
    await this.fetchCurrentVersion();
    this.setupVersionMonitoring();
    this.render();
  }
  setupVersionMonitoring() {
    document.addEventListener("continuum:version-update", (e) => {
      const customEvent = e;
      this.updateVersion(customEvent.detail.version);
    });
    const continuum2 = window.continuum;
    if (continuum2?.version) {
      this.currentVersion = continuum2.version;
    }
    setInterval(() => {
      this.checkForVersionUpdates();
    }, 3e4);
  }
  async fetchCurrentVersion() {
    try {
      const continuum2 = window.continuum;
      const result2 = await continuum2.info();
      this.currentVersion = result2.version;
      this.lastUpdate = /* @__PURE__ */ new Date();
    } catch (error) {
      console.warn("Could not get version:", error);
      this.currentVersion = "Unknown";
    }
  }
  async checkForVersionUpdates() {
    const previousVersion = this.currentVersion;
    await this.fetchCurrentVersion();
    if (this.currentVersion !== previousVersion) {
      this.render();
      this.showUpdateAnimation();
    }
  }
  updateVersion(newVersion) {
    if (newVersion !== this.currentVersion) {
      console.log(`\u{1F3F7}\uFE0F Version: ${this.currentVersion} \u2192 ${newVersion}`);
      this.currentVersion = newVersion;
      this.lastUpdate = /* @__PURE__ */ new Date();
      this.render();
      this.showUpdateAnimation();
    }
  }
  renderContent() {
    const updateTime = this.lastUpdate.toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit"
    });
    return `
      <div class="version-container">
        <div class="version-info">
          <span class="version-label">v</span>
          <span class="version-number">${this.currentVersion}</span>
        </div>
        <div class="version-meta">
          <span class="last-update">Updated ${updateTime}</span>
        </div>
      </div>
    `;
  }
  setupEventListeners() {
    const container = this.shadowRoot?.querySelector(".version-container");
    if (container) {
      container.addEventListener("click", () => {
        this.copyVersionToClipboard();
      });
    }
  }
  copyVersionToClipboard() {
    const versionText = `Continuum v${this.currentVersion}`;
    if (navigator.clipboard) {
      navigator.clipboard.writeText(versionText).then(() => {
        this.showCopyFeedback();
      }).catch((err) => {
        console.error("Failed to copy version:", err);
      });
    }
  }
  showCopyFeedback() {
    const container = this.shadowRoot?.querySelector(".version-container");
    if (container) {
      container.classList.add("copied");
      setTimeout(() => {
        container.classList.remove("copied");
      }, 1e3);
    }
  }
  showUpdateAnimation() {
    const container = this.shadowRoot?.querySelector(".version-container");
    if (container) {
      container.classList.add("updated");
      setTimeout(() => {
        container.classList.remove("updated");
      }, 600);
    }
  }
};
customElements.define("version-widget", VersionWidget);

// widget-discovery:widget-discovery
var WIDGET_ASSETS = {
  "ActiveProjectsWidget": {
    "css": [
      "ActiveProjectsWidget.css"
    ],
    "html": [],
    "js": [],
    "directoryName": "ActiveProjects"
  },
  "ChatWidget": {
    "css": [
      "ChatWidget.css"
    ],
    "html": [],
    "js": [],
    "directoryName": "Chat"
  },
  "ContinuonWidget": {
    "css": [
      "ContinuonWidget.css"
    ],
    "html": [],
    "js": [],
    "directoryName": "Continuon"
  },
  "PersonaWidget": {
    "css": [
      "PersonaWidget.css",
      "persona-animations.css"
    ],
    "html": [],
    "js": [],
    "directoryName": "Persona"
  },
  "SavedPersonasWidget": {
    "css": [
      "SavedPersonas.css",
      "SavedPersonasWidget.css",
      "styles.css"
    ],
    "html": [],
    "js": [],
    "directoryName": "SavedPersonas"
  },
  "SessionCostsWidget": {
    "css": [
      "SessionCosts.css"
    ],
    "html": [],
    "js": [],
    "directoryName": "SessionCosts"
  },
  "SidebarWidget": {
    "css": [
      "SidebarWidget.css"
    ],
    "html": [
      "SidebarWidget.html"
    ],
    "js": [],
    "directoryName": "Sidebar"
  },
  "SidebarHeader": {
    "css": [
      "SidebarHeader.css"
    ],
    "html": [
      "SidebarHeader.html"
    ],
    "js": [],
    "directoryName": "SidebarHeader"
  },
  "SidebarPanelWidget": {
    "css": [
      "SidebarPanelWidget.css"
    ],
    "html": [],
    "js": [],
    "directoryName": "SidebarPanelWidget"
  },
  "SidebarTabs": {
    "css": [
      "SidebarTabs.css"
    ],
    "html": [],
    "js": [],
    "directoryName": "SidebarTabs"
  },
  "UsersAgentsWidget": {
    "css": [
      "UsersAgents.css"
    ],
    "html": [],
    "js": [],
    "directoryName": "UsersAgents"
  },
  "VersionWidget": {
    "css": [
      "VersionWidget.css"
    ],
    "html": [],
    "js": [],
    "directoryName": "Version"
  }
};
window.WIDGET_ASSETS = WIDGET_ASSETS;
console.log("\u{1F3A8} Widget Discovery: 13 widgets registered");
console.log("\u{1F4C1} Asset Manifest: Zero 404s guaranteed!", WIDGET_ASSETS);

// src/ui/continuum-browser-client/index.ts
var continuum = new ContinuumBrowserClient();
window.continuum = continuum;
window.WidgetServerControls = WidgetServerControls;
console.log("\u{1F310} Continuum Browser Client: Single global instance created");
console.log("\u{1F3AE} Widget Server Controls: Dynamic command discovery system initialized");
var index_default = continuum;
export {
  ContinuumBrowserClient,
  index_default as default
};
//# sourceMappingURL=continuum-browser.js.map
