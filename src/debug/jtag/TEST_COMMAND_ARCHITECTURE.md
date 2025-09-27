# JTAG Test Command Architecture

**Document Version**: 1.0
**Date**: 2025-09-26
**Status**: Proposed

## Executive Summary

This document outlines the migration of our comprehensive shell script-based test system into proper JTAG commands, enabling both CLI and MCP (Claude Code) access to our entire test suite. The goal is to replace shell scripts with configurable, discoverable test commands while maintaining all existing functionality.

## Current State Analysis

### Existing Test Architecture
**100+ test files** organized in categories:
- `tests/integration/` - Integration tests
- `tests/unit/` - Unit tests
- `tests/chat-scenarios/` - Chat-specific tests
- `tests/layer-**/` - Architecture layer tests
- Individual test files for specific features

**Current Shell Script System**:
```bash
# run-categorized-tests.sh with profiles:
./scripts/run-categorized-tests.sh comprehensive
./scripts/run-categorized-tests.sh integration
./scripts/run-categorized-tests.sh chat
./scripts/run-categorized-tests.sh screenshots
./scripts/run-categorized-tests.sh performance
```

**Test Categories Currently Supported**:
- `comprehensive` - All tests (default)
- `integration` - Integration tests only
- `unit` - Unit tests only
- `chat` - Chat-related tests only
- `screenshots` - Screenshot tests only
- `themes` - Theme system tests only
- `transport` - Transport tests only
- `events` - Event system tests only
- `performance` - Grid P2P performance tests
- `blocker` - Blocker-level tests only
- `critical` - Critical tests only

### Problems with Current Architecture
1. **Not JTAG/MCP Accessible**: Shell scripts can't be called through MCP
2. **Hard to Discover**: No standard help/listing mechanism
3. **Limited Configurability**: Shell script parameters are inflexible
4. **No Programmatic Access**: Can't be used by AI assistants
5. **No Structured Results**: Output not machine-readable
6. **No Real-time Monitoring**: Can't watch test progress programmatically

## Proposed Command Architecture

### Core Test Commands Structure
```
commands/
└── test/
    ├── run/                    # Test execution (NEW)
    │   ├── suite/             # Run test suites by category
    │   ├── single/            # Run individual test files
    │   └── watch/             # Watch and re-run tests
    ├── discover/              # Test discovery (NEW)
    │   ├── list/              # List available tests
    │   ├── categories/        # List test categories
    │   └── search/            # Search tests by pattern
    ├── report/                # Test reporting (NEW)
    │   ├── results/           # Format test results
    │   ├── coverage/          # Coverage reporting
    │   └── history/           # Test run history
    └── routing-chaos/         # (existing - keep for compatibility)
```

### Command Specifications

#### **1. Test Execution Commands**

##### `./jtag test/run/suite`
```bash
# Replace shell script profiles
./jtag test/run/suite --profile="comprehensive"
./jtag test/run/suite --profile="chat" --timeout=30000
./jtag test/run/suite --profile="integration" --parallel=true
./jtag test/run/suite --profile="performance" --format="json"

# Custom suite definitions
./jtag test/run/suite --tests="crud,chat,screenshots" --name="custom-precommit"
```

##### `./jtag test/run/single`
```bash
# Run specific test files
./jtag test/run/single --file="tests/integration/crud-db-widget.test.ts"
./jtag test/run/single --pattern="**/chat-*.test.ts" --parallel=true
./jtag test/run/single --file="tests/integration/crud-db-widget.test.ts" --watch=true
```

##### `./jtag test/run/watch`
```bash
# Watch mode for continuous testing
./jtag test/run/watch --profile="integration" --on-change="auto"
./jtag test/run/watch --tests="crud,chat" --debounce=2000
./jtag test/run/watch --file="**/*.test.ts" --notify-failures=true
```

#### **2. Test Discovery Commands**

##### `./jtag test/discover/list`
```bash
# List all available tests
./jtag test/discover/list
./jtag test/discover/list --category="chat"
./jtag test/discover/list --format="tree" --show-descriptions
```

##### `./jtag test/discover/categories`
```bash
# List test categories and profiles
./jtag test/discover/categories
./jtag test/discover/categories --show-counts --show-examples
```

##### `./jtag test/discover/search`
```bash
# Search tests by name/content
./jtag test/discover/search --query="widget"
./jtag test/discover/search --pattern="**/crud-*.test.ts"
./jtag test/discover/search --content="ChatMessage" --show-matches
```

#### **3. Test Reporting Commands**

##### `./jtag test/report/results`
```bash
# Format and display test results
./jtag test/report/results --run-id="latest" --format="summary"
./jtag test/report/results --run-id="12345" --format="detailed" --include-timing
./jtag test/report/results --failures-only --show-suggestions
```

##### `./jtag test/report/coverage`
```bash
# Coverage reporting
./jtag test/report/coverage --format="text"
./jtag test/report/coverage --html --output-dir="./coverage"
./jtag test/report/coverage --threshold=80 --fail-below-threshold
```

## MCP Integration Examples

### **Claude Code Test Commands**
```typescript
// MCP tool definitions for AI assistants
interface JTAGTestMCPTools {
  // Test execution
  jtag_test_run_suite: (profile: string, options?: TestOptions) => MCPResult;
  jtag_test_run_single: (file: string, options?: TestOptions) => MCPResult;
  jtag_test_run_watch: (profile: string, options?: WatchOptions) => MCPResult;

  // Test discovery
  jtag_test_discover_list: (category?: string) => MCPResult;
  jtag_test_discover_search: (query: string) => MCPResult;
  jtag_test_discover_categories: () => MCPResult;

  // Test reporting
  jtag_test_report_results: (runId?: string, format?: string) => MCPResult;
  jtag_test_report_coverage: (format?: string) => MCPResult;
}
```

### **AI-Driven Test Workflows**
```typescript
// Claude Code automatically running targeted tests
const testResult = await ai.callTool('jtag_test_run_suite', {
  profile: 'chat',
  timeout: 30000,
  format: 'json'
});

if (testResult.failures > 0) {
  // AI can automatically investigate failures
  const failureDetails = await ai.callTool('jtag_test_report_results', {
    runId: testResult.runId,
    failuresOnly: true,
    showSuggestions: true
  });

  // AI can run specific debugging commands based on failures
  for (const failure of failureDetails.failures) {
    if (failure.type === 'widget-related') {
      await ai.callTool('jtag_debug_widget_state', {
        widgetSelector: failure.widgetSelector
      });
    }
  }
}
```

## Migration Strategy

### Phase 1: Core Command Implementation (Week 1)
**Goal**: Replace basic shell script functionality

**Commands to Implement**:
- `test/run/suite` - Direct replacement for `run-categorized-tests.sh`
- `test/discover/list` - List available tests
- `test/discover/categories` - List test profiles

**Migration Path**:
```bash
# Old way (shell script)
./scripts/run-categorized-tests.sh comprehensive

# New way (JTAG command)
./jtag test/run/suite --profile="comprehensive"
```

### Phase 2: Enhanced Discovery (Week 2)
**Goal**: Add advanced test discovery and search

**Commands to Implement**:
- `test/discover/search` - Search tests by pattern/content
- `test/run/single` - Run individual test files
- `test/report/results` - Structured result reporting

### Phase 3: Advanced Features (Week 3)
**Goal**: Add watch mode and continuous testing

**Commands to Implement**:
- `test/run/watch` - Watch mode testing
- `test/report/coverage` - Coverage reporting
- `test/report/history` - Test run history

### Phase 4: MCP Integration (Week 4)
**Goal**: Enable AI access to entire test suite

**MCP Integration**:
- All test commands available as MCP tools
- Structured JSON output for AI consumption
- Integration with Claude Code for autonomous testing

## Test Configuration System

### **Test Profiles** (Migrated from Shell Script)
```typescript
interface TestProfile {
  name: string;
  description: string;
  tests: TestPattern[];
  deployBrowser: boolean;
  parallelism: number;
  timeout: number;
  prerequisites: string[];
}

// Example profiles.json
{
  "comprehensive": {
    "description": "All tests including integration and UI",
    "tests": ["**/*.test.ts"],
    "deployBrowser": true,
    "parallelism": 4,
    "timeout": 300000
  },
  "chat": {
    "description": "Chat system tests only",
    "tests": [
      "tests/chat-*.test.ts",
      "tests/integration/crud-db-widget.test.ts",
      "tests/chat-scenarios/**/*.test.ts"
    ],
    "deployBrowser": true,
    "parallelism": 2,
    "timeout": 60000
  },
  "precommit": {
    "description": "Fast precommit validation tests",
    "tests": [
      "tests/integration/crud-db-widget.test.ts",
      "tests/bootstrap-detection.test.ts",
      "tests/compiler-error-detection.test.ts"
    ],
    "deployBrowser": false,
    "parallelism": 1,
    "timeout": 30000
  }
}
```

### **Custom Test Suites**
```bash
# Define custom test combinations
./jtag test/run/suite --tests="crud,widget-state,chat-basic" --name="widget-validation" --save

# Run saved custom suite
./jtag test/run/suite --profile="widget-validation"

# Claude Code can create custom suites based on failure patterns
await ai.callTool('jtag_test_run_suite', {
  tests: ['crud', 'chat', 'screenshots'],
  name: 'failure-investigation',
  save: true
});
```

## Integration with Existing CRUD Test

### **Our CRUD Test Integration**
```bash
# Current CRUD test
npx tsx tests/integration/crud-db-widget.test.ts

# New JTAG approach
./jtag test/run/single --file="tests/integration/crud-db-widget.test.ts" --format="detailed"

# Part of larger suite
./jtag test/run/suite --profile="integration" --focus="crud"

# Claude Code access
await ai.callTool('jtag_test_run_single', {
  file: 'tests/integration/crud-db-widget.test.ts',
  format: 'json',
  includeDetails: true
});
```

## Success Metrics

### Phase 1 Metrics
- [ ] All existing shell script profiles work as JTAG commands
- [ ] Test discovery commands functional
- [ ] 100% backward compatibility with existing test execution
- [ ] Structured JSON output available

### Phase 2 Metrics
- [ ] Advanced test search and filtering
- [ ] Individual test file execution
- [ ] Structured result reporting with actionable insights
- [ ] Performance parity with shell scripts

### Phase 3 Metrics
- [ ] Watch mode functional with auto-rerun
- [ ] Coverage reporting integrated
- [ ] Test run history and trend analysis
- [ ] Custom test suite creation and management

### Phase 4 Metrics
- [ ] 100% MCP tool coverage for all test commands
- [ ] Claude Code can autonomously run any test
- [ ] AI can create custom test suites based on failure analysis
- [ ] Zero-configuration integration with existing Claude Code

## Benefits

### **For Human Developers**
- **Unified Interface**: All tests accessible through consistent JTAG commands
- **Better Discovery**: Search and filter tests easily
- **Structured Results**: Machine-readable output for tooling integration
- **Watch Mode**: Continuous testing during development

### **For AI Assistants (Claude Code)**
- **Full Test Access**: Every test in the system available as MCP tool
- **Autonomous Testing**: AI can run targeted tests based on code changes
- **Failure Analysis**: AI can analyze test failures and suggest fixes
- **Custom Suites**: AI can create test combinations based on patterns

### **System Benefits**
- **Maintainability**: Replace shell scripts with proper command architecture
- **Composability**: Test commands work with other JTAG commands (filter, watch, etc.)
- **Scalability**: Easy to add new test types and categories
- **Integration**: Works seamlessly with existing JTAG ecosystem

## Future Opportunities

### **Advanced AI Testing**
- **Predictive Testing**: AI suggests which tests to run based on code changes
- **Auto-Fix Generation**: AI proposes fixes for common test failures
- **Test Generation**: AI creates new tests based on code patterns
- **Performance Optimization**: AI optimizes test execution order and parallelism

### **Continuous Integration**
- **Smart CI**: Only run tests affected by changes
- **Failure Prediction**: Predict test failures before running
- **Auto-Recovery**: Automatically retry flaky tests with different configurations
- **Real-time Monitoring**: Live test execution monitoring and intervention

This architecture transforms our test system from shell scripts into a comprehensive, AI-accessible testing platform that maintains all existing functionality while enabling powerful new capabilities.