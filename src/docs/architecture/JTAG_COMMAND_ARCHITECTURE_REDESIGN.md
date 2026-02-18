# JTAG Command Architecture Redesign

**Document Version**: 1.0
**Date**: 2025-09-26
**Status**: Proposed

## Executive Summary

This document outlines a comprehensive redesign of the JTAG command architecture to create a **composable, Unix-philosophy toolkit** that transforms JTAG from a debugging tool into a powerful development platform. The redesign emphasizes **general-purpose building blocks** that combine to solve complex problems.

## Current State Analysis

### Existing Command Structure (39 commands)
```
commands/
├── click/
├── compile-typescript/
├── data/
│   ├── clear/
│   ├── create/
│   ├── delete/
│   ├── list/
│   ├── read/
│   ├── schema/
│   ├── truncate/
│   └── update/
├── debug/
│   ├── crud-sync/
│   ├── error/
│   ├── html-inspector/
│   ├── logs/
│   ├── scroll-test/
│   ├── widget-events/
│   ├── widget-interact/
│   └── widget-state/
├── exec/
├── file/
│   ├── append/
│   ├── load/
│   └── save/
├── get-text/
├── indicator/
├── list/
├── navigate/
├── ping/
├── process-registry/
├── proxy-navigate/
├── screenshot/
├── scroll/
├── session/
│   ├── create/
│   └── destroy/
├── test/
│   └── routing-chaos/
├── theme/
│   ├── get/
│   ├── list/
│   └── set/
├── type/
└── wait-for-element/
```

### Problems with Current Architecture
1. **Limited Composability**: Commands are monolithic and don't chain well
2. **JSON Truncation**: Large outputs get cut off at ~8KB limit
3. **Domain Mixing**: Debugging tools mixed with data operations
4. **No General Processing**: Every operation needs custom commands
5. **Discovery Issues**: Hard to find related commands across categories

## Proposed Architecture: Composable Command Toolkit

### Design Principles
1. **Unix Philosophy**: Each command does one thing well
2. **Composability**: Commands chain together via pipes
3. **Domain Separation**: Clear boundaries between categories
4. **General Purpose**: Core commands work with any data
5. **Extensibility**: Easy to add new commands in logical places

### New Directory Structure

#### Phase 1: Core Data Processing (General Purpose)
```
commands/
├── filter/                    # Data filtering & querying (NEW)
│   ├── json/                 # JSON path filtering
│   ├── grep/                 # Text pattern matching
│   ├── count/                # Counting operations
│   └── extract/              # Data extraction
├── tree/                     # Data visualization (NEW)
│   ├── json/                 # JSON structure visualization
│   ├── collapse/             # Collapsible tree view
│   └── diff/                 # Tree comparisons
└── pipe/                     # Command composition (NEW)
    ├── chain/                # Sequential piping
    ├── parallel/             # Parallel execution
    └── merge/                # Result merging
```

#### Phase 2: Monitoring & Observability
```
commands/
├── watch/                    # Real-time monitoring (NEW)
│   ├── changes/              # Change detection
│   ├── condition/            # Condition-based watching
│   └── diff/                 # Differential monitoring
└── trace/                    # Flow tracking & debugging (NEW)
    ├── events/               # Event flow tracing
    ├── data/                 # Data lifecycle tracking
    └── commands/             # Command execution tracing
```

#### Phase 3: Enhanced Domain Commands
```
commands/
├── debug/                    # Enhanced debugging (EXPANDED)
│   ├── crud-sync/           # (existing)
│   ├── error/               # (existing)
│   ├── html-inspector/      # (existing)
│   ├── logs/                # (existing)
│   ├── scroll-test/         # (existing)
│   ├── widget-events/       # (existing)
│   ├── widget-interact/     # (existing)
│   ├── widget-state/        # (existing)
│   ├── widget-filter/       # Widget-specific filtering (NEW)
│   ├── widget-watch/        # Widget change monitoring (NEW)
│   └── message-trace/       # Message lifecycle debugging (NEW)
├── data/                    # Keep existing structure
└── [other existing commands remain unchanged]
```

## Implementation Phases

### Phase 1: Foundation (Weeks 1-2)
**Goal**: Create core data processing commands

**New Commands**:
```bash
# Filter commands
./jtag filter/json --path="messages[*].id" --contains="123"
./jtag filter/grep --pattern="error" --count-only
./jtag filter/extract --field="id,content" --format="csv"

# Tree visualization
./jtag tree/json --max-depth=3 --collapse-strings
./jtag tree/collapse --hide="templateCSS,longFields"

# Basic piping
./jtag pipe/chain --commands="cmd1|cmd2|cmd3"
```

**Success Metrics**:
- Solve JSON truncation issues
- Enable basic command composition
- Improve debugging workflow efficiency by 50%

### Phase 2: Monitoring (Weeks 3-4)
**Goal**: Add real-time monitoring and tracing

**New Commands**:
```bash
# Watch commands
./jtag watch/changes --command="debug/widget-state --widgetSelector=chat-widget"
./jtag watch/condition --condition="count > 5" --timeout=30000

# Trace commands
./jtag trace/data --entity-type="ChatMessage" --follow-through="database,events,widgets"
./jtag trace/events --pattern="data:*:created" --real-time
```

**Success Metrics**:
- Enable real-time debugging
- Provide clear visibility into event propagation
- Reduce debugging time for timing issues by 70%

### Phase 3: Domain Enhancement (Weeks 5-6)
**Goal**: Enhance existing domain commands with new capabilities

**Enhanced Commands**:
```bash
# Widget-specific enhancements
./jtag debug/widget-filter --widgetSelector="chat-widget" --filterPath="messages"
./jtag debug/widget-watch --widgetSelector="chat-widget" --watchFor="new-messages"
./jtag debug/message-trace --messageId="123" --show-lifecycle
```

**Success Metrics**:
- Achieve 100% CRUD test success rate
- Provide comprehensive widget debugging toolkit
- Enable complex debugging workflows

### Phase 4: MCP Integration & Claude Code Integration (Weeks 7-8)
**Goal**: Create AI-accessible interface with full CLI symmetry and direct Claude Code integration

#### **4.1: MCP Server Implementation**
**MCP Layer Features**:
```typescript
// MCP Tool Definitions (AI-accessible)
interface JTAGMCPTools {
  // Core data processing
  jtag_filter_json: (path: string, data?: any) => MCPResult;
  jtag_tree_json: (maxDepth?: number, data?: any) => MCPResult;
  jtag_pipe_chain: (commands: string[]) => MCPResult;

  // Monitoring & observability
  jtag_watch_changes: (command: string, timeout?: number) => MCPResult;
  jtag_trace_data: (entityType: string, entityId: string) => MCPResult;

  // Domain-specific debugging
  jtag_debug_widget_state: (widgetSelector: string) => MCPResult;
  jtag_debug_widget_watch: (widgetSelector: string, watchFor: string) => MCPResult;
  jtag_debug_message_trace: (messageId: string) => MCPResult;
}

// Symmetric CLI mapping
./jtag filter/json --path="$.messages[*]" ↔ jtag_filter_json(path: "$.messages[*]")
./jtag debug/widget-state --widgetSelector="chat" ↔ jtag_debug_widget_state(widgetSelector: "chat")
```

#### **4.2: Claude Code Direct Integration**
**MCP Server Configuration**:
```json
// Claude Code MCP configuration
{
  "mcpServers": {
    "jtag": {
      "command": "node",
      "args": ["./src/debug/jtag/mcp-server.js"],
      "cwd": "/path/to/continuum/src/debug/jtag",
      "env": {
        "JTAG_MODE": "mcp",
        "LOG_LEVEL": "info"
      }
    }
  }
}
```

**Claude Code Tool Auto-Discovery**:
```typescript
// Claude Code automatically discovers these tools
interface ClaudeCodeJTAGTools {
  // Zero-configuration tool access
  jtag_filter_json: (path: string, data?: string) => Promise<MCPResult>;
  jtag_tree_json: (maxDepth?: number, data?: string) => Promise<MCPResult>;
  jtag_debug_widget_state: (widgetSelector: string) => Promise<MCPResult>;
  jtag_debug_message_trace: (messageId: string) => Promise<MCPResult>;
  jtag_watch_changes: (command: string, timeout?: number) => Promise<MCPResult>;
  // ... all 39+ commands available as tools
}
```

#### **4.3: Autonomous Development Workflows**
**Interactive Debugging Session**:
```markdown
Developer: "The chat widget isn't showing new messages"

Claude Code: Let me debug this systematically:

1. First, checking widget state...
   *[calls jtag_debug_widget_state(widgetSelector: "chat-widget")]*

2. I see 10 messages but let me monitor for new ones...
   *[calls jtag_watch_changes(command: "debug/widget-state --widgetSelector=chat-widget", timeout: 30000)]*

3. Found the issue: Messages created but not propagating. Tracing lifecycle...
   *[calls jtag_trace_data(entityType: "ChatMessage", operation: "create")]*

4. Root cause: DELETE events aren't triggering widget refreshes properly.
```

**Autonomous Issue Resolution**:
```markdown
Claude Code: Detected CRUD test failure. Automatically investigating...

*[Chains multiple tools automatically]:*
1. jtag_trace_data(entityType: "ChatMessage", operation: "create")
2. jtag_filter_json(path: "$.events[?(@.type=='created')]")
3. jtag_debug_widget_watch(widgetSelector: "chat-widget", watchFor: "new-messages")

Analysis complete: ChatMessage CREATE works, DELETE event propagation delayed by 3 seconds.
Recommended fix: Increase DELETE delay in widget event handlers.
```

#### **4.4: Integration Benefits**
**Zero Configuration**:
- Works with Claude Code's existing MCP infrastructure
- Automatic tool discovery and type inference
- No additional setup beyond MCP server registration

**Real-time Development Partnership**:
- Claude Code monitors development process continuously
- Automatically detects and debugs issues as they occur
- Provides contextual suggestions based on command output patterns
- Learns effective debugging sequences for future use

**Seamless Workflow Integration**:
```markdown
Developer: "Run the CRUD test and fix any issues"

Claude Code:
1. Running CRUD test... ✅ 8/9 tests passing (88.9%)
2. ChatMessage DELETE failing - investigating automatically...
   *[runs diagnostic sequence without human intervention]*
3. Issue identified: Widget refresh timing in DELETE operations
4. Suggested fix: Increase WIDGET_VERIFICATION_DELAY for ChatMessage DELETE
5. Would you like me to implement this fix?
```

**Unified Help System**:
```bash
# CLI help (for human developers)
./jtag help filter/json
./jtag help --examples filter/json
./jtag help --composition-examples

# MCP help (accessible to Claude Code)
jtag_help(command: "filter/json", includeExamples: true)
jtag_help_composition(category: "debugging", scenario: "widget-issues")
```

#### **4.5: Advanced Claude Code Capabilities**
**Learning & Optimization**:
- Claude Code learns effective debugging sequences
- Builds context about specific codebase patterns
- Suggests preventive measures based on failure patterns
- Optimizes command combinations for better performance

**Continuous System Health**:
- Monitors system health using watch commands
- Automatically runs diagnostic sequences on failures
- Maintains knowledge base of common issues and solutions
- Provides proactive maintenance recommendations

**Success Metrics**:
- 100% CLI/MCP command symmetry maintained
- Claude Code can autonomously resolve 80%+ of common debugging issues
- Zero-configuration integration with existing Claude Code installations
- Real-world AI debugging scenarios validated and optimized
- Developer productivity improved 3x through AI partnership

## Usage Examples

### Current Debugging Workflow (Problematic)
```bash
# Multiple manual steps, prone to errors
./jtag debug/widget-state --widgetSelector="chat-widget" > output.json
# Manually parse 8KB+ JSON file to find specific message
# Limited visibility into why message isn't appearing
```

### New Composable Workflow (Powerful)
```bash
# One-liner to find specific message
./jtag debug/widget-state --widgetSelector="chat-widget" | \
./jtag filter/json --path="messages[?(@.id=='123')]" | \
./jtag tree/json --compact

# Monitor widget changes in real-time
./jtag watch/changes --command="debug/widget-state --widgetSelector=chat-widget" \
  --diff --highlight="added,removed"

# Trace complete message lifecycle
./jtag trace/data --entity-id="123" --from="data/create" \
  --through="database,events,widgets" --show-timing
```

### Solving Our ChatMessage CREATE Issue
```bash
# Current approach: Manual investigation, timing guesswork
# New approach: Systematic tracing
./jtag trace/data --entity-type="ChatMessage" --operation="create" \
  --watch-for="widget-propagation" --timeout=10000

# Or monitor widget during CRUD test
./jtag watch/condition --command="debug/widget-state --widgetSelector=chat-widget" \
  --condition="contains:test-message-id" --show-diff
```

### AI-Driven Debugging Workflows (Phase 4)
```typescript
// AI Assistant automatically debugging widget issues
const debugResult = await ai.callTool('jtag_debug_widget_state', {
  widgetSelector: 'chat-widget'
});

if (!debugResult.success) {
  // AI can automatically trace the issue
  const trace = await ai.callTool('jtag_trace_data', {
    entityType: 'ChatMessage',
    entityId: lastCreatedId
  });

  // AI can filter and analyze the results
  const filtered = await ai.callTool('jtag_filter_json', {
    path: '$.events[?(@.type=="created")]',
    data: trace.result
  });
}
```

**AI Capabilities**:
- **Autonomous Debugging**: AI can run diagnostic sequences without human intervention
- **Pattern Recognition**: AI can identify common failure patterns across command outputs
- **Workflow Optimization**: AI can suggest better command combinations based on success patterns
- **Real-time Assistance**: AI can monitor long-running processes and alert on anomalies

## Technical Specifications

### Command Interface Standards
```typescript
// All commands follow consistent interface
interface CommandParams {
  // Input/output handling
  input?: 'stdin' | 'file' | 'direct';
  output?: 'stdout' | 'file' | 'json';
  format?: 'json' | 'text' | 'csv' | 'tree';

  // Common options
  timeout?: number;
  verbose?: boolean;
  help?: boolean;
}

// Pipe-compatible commands implement
interface PipeableCommand {
  acceptsStdin(): boolean;
  producesStdout(): boolean;
  getOutputFormat(): string;
}
```

### JSON Truncation Solution
- **Problem**: Current 8KB limit breaks large widget state queries
- **Solution**: Streaming JSON processing in `filter/` commands
- **Implementation**: Use JSONPath queries to extract before size limits hit

### Error Handling & Logging
- Consistent error formats across all commands
- Structured logging for debugging command chains
- Graceful degradation when pipes break

## Migration Strategy

### Backward Compatibility
- **All existing commands remain unchanged** during transition
- New commands are additive, not replacement
- Gradual migration of existing functionality to new patterns

### User Communication
- Document new commands with examples
- Provide migration guides for common workflows
- Add completion hints for command discovery

### Testing Strategy
- Unit tests for each new command
- Integration tests for command chains
- Performance tests for large data processing
- Regression tests for existing functionality

## Success Metrics

### Phase 1 Metrics
- [ ] JSON truncation eliminated
- [ ] Basic command chaining functional
- [ ] 3 filter commands implemented
- [ ] Developer productivity improved (subjective)

### Phase 2 Metrics
- [ ] Real-time monitoring capabilities
- [ ] Event tracing implemented
- [ ] ChatMessage CREATE issue debugged
- [ ] 5 watch/trace commands implemented

### Phase 3 Metrics
- [ ] 100% CRUD test success rate achieved
- [ ] Complete widget debugging toolkit
- [ ] All 39 existing commands remain functional
- [ ] Developer feedback positive

### Long-term Metrics
- Command usage analytics (which commands are used most)
- Developer time saved (measured through productivity surveys)
- Bug resolution time improvement
- Community adoption of new patterns

## Risk Assessment

### Low Risk
- **Backward compatibility**: No existing functionality affected
- **Incremental deployment**: Can roll out command by command
- **Familiar patterns**: Unix-like composition is well-understood

### Medium Risk
- **Learning curve**: Developers need to learn new composition patterns
- **Command proliferation**: May become overwhelming without good organization
- **Performance**: Piped commands may be slower than monolithic ones

### Mitigation Strategies
- Comprehensive documentation with examples
- Progressive disclosure in help systems
- Performance benchmarking during development
- User feedback loops during each phase

## Future Opportunities

### Advanced Features (Post-Phase 3)
- **AI-Powered Debugging**: Use command patterns to train debugging assistants
- **Workflow Automation**: Save and replay common command chains
- **Performance Profiling**: Built-in timing and resource usage tracking
- **Remote Debugging**: Extend commands to work across network boundaries

### Integration Opportunities
- **IDE Integration**: Command completion and execution in editors
- **CI/CD Pipeline**: Use commands for automated testing and deployment
- **Monitoring Systems**: Export metrics to external monitoring platforms
- **Documentation Generation**: Auto-generate docs from command usage patterns

## Conclusion

This redesign transforms JTAG from a specialized debugging tool into a **comprehensive development platform** built on composable, general-purpose commands. The phased approach ensures safe migration while delivering immediate value.

The key innovation is **separation of concerns**: domain-specific commands (like `debug/widget-state`) generate data that general-purpose commands (like `filter/json`, `tree/json`) process. This creates exponential possibilities from linear command additions.

**Expected Outcomes**:
- **Developer Productivity**: 2-3x improvement in debugging efficiency
- **System Understanding**: Better visibility into complex behaviors
- **Extensibility**: Easy to add new capabilities as system grows
- **Reliability**: Robust testing and monitoring capabilities

The composable architecture positions JTAG as a foundational tool that will scale with the project's complexity and team's needs.