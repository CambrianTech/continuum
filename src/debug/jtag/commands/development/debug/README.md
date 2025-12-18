# JTAG Debug Commands

**Elegant debugging tools for Claude Code development - replacing raw `exec` commands with specialized, reusable debugging infrastructure.**

## 🎯 Purpose

These debug commands provide Claude with sophisticated debugging capabilities without resorting to raw JavaScript execution. Each command is:

- **Specialized**: Purpose-built for specific debugging needs
- **Reusable**: Can be called repeatedly with different parameters  
- **Type-safe**: Full TypeScript typing with clear interfaces
- **Elegant**: Clean, consistent API following JTAG patterns
- **Comprehensive**: Rich debugging information in structured format

## 📋 Available Debug Commands

### `widget-events` - Widget Event System Debugging
**Usage**: `./jtag debug/widget-events --widgetSelector="chat-widget"`

**Purpose**: Deep inspection of widget event listeners and event system connectivity
- ✅ Event emitter analysis (Map structure, handler counts)
- ✅ Event dispatcher status (server→widget connectivity) 
- ✅ DOM event listener detection
- ✅ Server event connectivity testing
- ✅ Handler registration verification

**Replaces**: Raw `exec` commands for event debugging

**Example Output**:
```json
{
  "success": true,
  "eventSystem": {
    "eventEmitterSize": 3,
    "eventTypes": ["chat:message-received", "chat:participant-joined"],
    "dispatcherTypes": ["chat:message-received"]
  },
  "connectivity": {
    "serverEventsWorking": true,
    "dispatcherWorking": true
  }
}
```

### `widget-state` - Widget State & Data Inspection
**Usage**: `./jtag debug/widget-state --includeMessages=true`

**Purpose**: Comprehensive widget state analysis
- ✅ Widget instance discovery (Shadow DOM navigation)
- ✅ Method enumeration and analysis
- ✅ Data connectivity testing
- ✅ Message history inspection  
- ✅ JTAG system connectivity

**Replaces**: Raw `exec` commands for widget state inspection

### `logs` - System Log Analysis & Error Investigation
**Usage**: `./jtag debug/logs --tailLines=50 --includeErrorsOnly=true`

**Purpose**: Current user session log inspection with proper JTAG file system access
- ✅ Multi-source log discovery (server, browser, system logs)
- ✅ Current session auto-detection
- ✅ Error pattern analysis and critical issue identification
- ✅ Structured log parsing (JSON + text formats)
- ✅ Time-based and pattern filtering
- ✅ System status analysis

**Replaces**: Raw bash commands like `tail`, `grep`, `cat` for log inspection

**Example Output**:
```json
{
  "success": true,
  "currentSession": "0de15c54-7bf6-4bca-af3e-227b0bd9e612",
  "errorSummary": {
    "totalErrors": 15,
    "criticalIssues": [
      "ChatWidget sendMessage failing with undefined error",
      "Real-time event system broken"
    ]
  },
  "logEntries": [
    {
      "timestamp": "2025-09-10T23:15:42.123Z",
      "level": "error",
      "message": "❌ ChatWidget: Send failed: undefined"
    }
  ]
}
```

### `html-inspector` - DOM Structure Analysis
**Usage**: `./jtag debug/html-inspector --selector="body"`

**Purpose**: Deep HTML structure and CSS inspection
- ✅ Shadow DOM traversal
- ✅ CSS style computation
- ✅ Element hierarchy analysis
- ✅ Attribute and property inspection
- ✅ Event listener detection

**Replaces**: Raw `exec` commands for HTML interrogation

### `scroll-test` - Animated Scroll Testing & Intersection Observer Debugging
**Usage**: `./jtag debug/scroll-test --target=top --captureMetrics=true --waitTime=1000`

**Purpose**: Animated scroll testing for debugging intersection observers and infinite scroll behaviors
- ✅ Smooth, instant, or auto scroll behaviors with precise control
- ✅ Shadow DOM traversal for chat-widget and deep component access
- ✅ Metrics capture (scroll dimensions, message counts, sentinel visibility)
- ✅ Multiple targets: `top`, `bottom`, or specific `position`
- ✅ Custom selector support with fallback to chat container
- ✅ Wait time support for testing async scroll completion
- ✅ **NEW**: Preset shortcuts for common debugging scenarios
- ✅ **NEW**: Repeat functionality for intersection observer stress testing

**Preset Shortcuts**:
```bash
./jtag debug/scroll-test --preset=chat-top      # Quick scroll to top with metrics
./jtag debug/scroll-test --preset=chat-bottom   # Quick scroll to bottom with metrics
./jtag debug/scroll-test --preset=instant-top   # Instant scroll to top for testing
```

**Replaces**: Raw `exec` commands for scroll testing and intersection observer debugging

**Example Output**:
```json
{
  "success": true,
  "scrollPerformed": true,
  "targetElement": "chat-widget",
  "initialPosition": 1250,
  "finalPosition": 0,
  "scrollDuration": 847,
  "metrics": {
    "scrollHeight": 2500,
    "clientHeight": 400,
    "messagesCount": 24,
    "sentinelVisible": false
  }
}
```

**Critical for debugging**:
- **Intersection Observer Issues**: Test negative rootMargin fixes with precise scroll triggers
- **Infinite Scroll Problems**: Verify sentinel positioning and visibility detection
- **Chat Position Bugs**: Test scroll restoration and newest-message positioning
- **EntityScroller Timing**: Debug async loading vs scroll positioning timing issues
- **Sort Order Problems**: Debug chronological message ordering and cursor pagination gaps
- **Wrong Direction Detection**: Test if scroll up/down triggers correct intersection observers

**Debugging Sort Order Issues**:
```bash
# Test multiple scroll actions to see message ordering
./jtag debug/scroll-test --preset=chat-top --repeat=3
./jtag debug/scroll-test --preset=chat-bottom --repeat=2

# Capture detailed metrics to track message count changes
./jtag debug/scroll-test --target=top --captureMetrics=true --waitTime=3000
```

## 🔧 How to Use Debug Commands

### Basic Usage
```bash
# Widget event debugging
./jtag debug/widget-events --widgetSelector="chat-widget"

# Widget state inspection
./jtag debug/widget-state --includeMessages=true --roomId="general"

# HTML structure analysis
./jtag debug/html-inspector --selector=".message-container"

# Animated scroll testing (intersection observer debugging)
./jtag debug/scroll-test --target=top --captureMetrics=true --waitTime=1000
```

### Advanced Parameters
```bash
# Comprehensive widget events analysis with server event testing
./jtag debug/widget-events \
  --widgetSelector="chat-widget" \
  --testServerEvents=true \
  --includeHandlers=true

# Deep widget state analysis with data connectivity testing
./jtag debug/widget-state \
  --widgetSelector="chat-widget" \
  --includeMessages=true \
  --testDataConnectivity=true \
  --roomId="general"

# Advanced scroll testing with custom selector and full metrics capture
./jtag debug/scroll-test \
  --target=position \
  --position=500 \
  --behavior=smooth \
  --selector="main-widget .content" \
  --captureMetrics=true \
  --waitTime=2000

# Stress test intersection observers with repeated scrolls
./jtag debug/scroll-test \
  --preset=chat-top \
  --repeat=5 \
  --waitTime=5000
```

## 🎨 Debug Command Architecture

Each debug command follows the universal JTAG pattern:

```
commands/debug/{command-name}/
├── shared/
│   └── {CommandName}DebugTypes.ts    # Type definitions
├── browser/
│   └── {CommandName}BrowserCommand.ts # Browser-side logic
├── server/ 
│   └── {CommandName}ServerCommand.ts  # Server-side logic (usually routes to browser)
└── README.md                          # Command-specific documentation
```

## 📊 Output Format Standards

All debug commands return structured results with:

```typescript
interface DebugResult {
  success: boolean;           // Command execution success
  [dataFields]: any;         // Command-specific data
  debugging: {               // Debugging metadata
    logs: string[];          // Execution logs
    warnings: string[];      // Non-fatal issues
    errors: string[];        // Error messages
  };
  error?: string;            // Fatal error message
}
```

## 🚀 Creating New Debug Commands

To add a new debug command:

1. **Create directory**: `commands/debug/{new-command}/`
2. **Add types**: `shared/{NewCommand}DebugTypes.ts`
3. **Implement browser**: `browser/{NewCommand}BrowserCommand.ts`
4. **Add server**: `server/{NewCommand}ServerCommand.ts`
5. **Document**: Add to this README

### Template Structure
```typescript
// shared/NewDebugTypes.ts
export interface NewDebugParams {
  selector?: string;
  includeDetails?: boolean;
}

export interface NewDebugResult {
  success: boolean;
  [specific_data]: any;
  debugging: {
    logs: string[];
    warnings: string[];
    errors: string[];
  };
  error?: string;
}
```

## 🎯 Benefits for Claude Development

### Before (Raw Exec)
```bash
./jtag exec --code="
const widget = document.querySelector('continuum-widget')?.shadowRoot?.querySelector('main-widget')?.shadowRoot?.querySelector('chat-widget');
return {
  eventEmitter: widget?.eventEmitter?.size || 0,
  // ... complex debugging logic
};
" --environment="browser"
```

### After (Elegant Debug Commands)  
```bash
./jtag debug/widget-events --widgetSelector="chat-widget"
```

**Advantages**:
- ✅ **Reusable**: Same command across debugging sessions
- ✅ **Type-safe**: Full IntelliSense and error checking
- ✅ **Comprehensive**: Rich, structured debugging data
- ✅ **Maintainable**: Easy to enhance and extend
- ✅ **Elegant**: Clean, professional debugging interface

## 🔍 Planned Debug Commands

### `css-inspector` - CSS Debugging
- Style computation analysis
- CSS cascade debugging  
- Shadow DOM CSS isolation testing
- Theme and variable inspection

### `event-tracer` - Real-time Event Monitoring
- Live event stream monitoring
- Event propagation visualization
- Handler execution timing
- Cross-environment event tracing

### `widget-lifecycle` - Widget Lifecycle Debugging
- Initialization sequence analysis
- Cleanup verification
- Memory leak detection
- Performance profiling

### `data-flow` - Data Flow Analysis
- Command execution tracing
- Database operation monitoring  
- Cache hit/miss analysis
- Cross-widget data sharing

## 📚 Integration with CLAUDE.md

These debug commands integrate with the core debugging methodology in CLAUDE.md:

1. **Visual-first debugging**: Commands support screenshot integration
2. **Log-first analysis**: All commands provide structured logging
3. **Systematic methodology**: Commands follow established debugging patterns
4. **Scientific approach**: Commands provide data-driven debugging insights

**Usage Pattern**:
```bash
# 1. Understand system state
./jtag debug/widget-state

# 2. Analyze specific issues  
./jtag debug/widget-events

# 3. Visual validation
./jtag screenshot --querySelector="chat-widget"

# 4. Iterate and validate
./jtag debug/widget-events --testServerEvents=true
```

---

**This is the future of AI-driven development debugging - elegant, systematic, and endlessly reusable.**