# Browser Manager Daemon

**Intelligent browser orchestration with context-aware behavior for AI-human collaboration**

## 🧠 **INTELLIGENT DEFAULTS BY CONTEXT**

The Browser Manager implements **smart behavioral defaults** that adapt to different connection contexts:

### **Human Interactive Sessions** (`./continuum`):
- **Focus**: `true` - Brings browser to front automatically
- **Kill Zombies**: `true` - Cleans up orphaned tabs for workspace hygiene  
- **Philosophy**: Helpful and active assistance for direct human interaction

### **AI/API/Portal Sessions**:
- **Focus**: `false` - Respects human's current work, no interruption
- **Kill Zombies**: `false` - Preserves existing browser state
- **Philosophy**: Respectful background operation for autonomous systems

### **🎯 Beautiful Emergent Behaviors**:

1. **Persona working alongside human**:
   ```typescript
   // Persona connects via API - no disruption
   api.connect({ sessionType: 'persona', focus: false })
   // Human continues working in IDE while AI works silently
   ```

2. **Portal integration**:
   ```typescript  
   // Git hook triggers portal - silent operation
   portal.connect({ sessionType: 'validation', killZombies: false })
   // Doesn't interfere with human's debugging tabs
   ```

3. **Human development flow**:
   ```bash
   ./continuum  # Brings browser to front, cleans workspace
   # Perfect for "I want to start working now"
   ```

4. **Academy training**:
   ```typescript
   // Spawned personas respect human's environment
   academy.spawnPersona({ focus: false, killZombies: false })
   // Multiple personas work without mutual interference
   ```

## 🚀 **CORE FEATURES**

### **Smart Browser Management**
- **ONE TAB POLICY**: Prevents browser tab proliferation with semaphore protection
- **Race Condition Prevention**: Global launch lock prevents simultaneous browser spawning
- **Modular Architecture**: Platform-specific adapters (macOS AppleScript, DevTools Protocol)

### **Zombie Tab Management** 
- **Smart Detection**: Uses AppleScript/DevTools to identify orphaned tabs
- **Selective Cleanup**: Preserves tabs with active WebSocket connections
- **Configurable Behavior**: Respects session-specific kill policies

### **Cross-Platform Focus Control**
- **macOS Integration**: AppleScript for Opera GX, Chrome, Safari
- **Window Management**: Brings correct tab to front and activates window
- **Non-Intrusive Options**: API clients can opt out of focus stealing

## ⚙️ **CONFIGURATION**

### **Session Connection Parameters**
```typescript
interface ConnectOptions {
  focus?: boolean;        // Default: true for bash, false for API
  killZombies?: boolean;  // Default: true for shared, false for API
  sessionType: string;    // development, persona, portal, validation
  owner: string;          // shared, user, persona-name
}
```

### **Platform Adapters**
```json
{
  "darwin": {
    "browsers": ["Opera GX", "Chrome", "Safari"],
    "method": "AppleScript",
    "fallback": "DevTools Protocol"
  },
  "linux": {
    "browsers": ["Chrome", "Firefox"],
    "method": "DevTools Protocol"
  },
  "win32": {
    "browsers": ["Chrome", "Edge"],
    "method": "DevTools Protocol"
  }
}
```

## 🧪 **TESTING**

```bash
# Test browser tab detection
npm run test:browser-detection

# Test zombie cleanup logic  
npm run test:zombie-management

# Test focus behavior
npm run test:focus-control

# Full integration with real browsers
npm run test:browser-integration
```

## 🏗️ **ARCHITECTURE**

### **Modular Design**
```
browser-manager/
├── BrowserManagerDaemon.ts     # Core orchestrator
├── modules/
│   ├── BrowserLauncher.ts      # Platform-specific launching
│   ├── BrowserTabAdapter.ts    # Tab detection & management
│   ├── ChromeBrowserModule.ts  # Chrome/Chromium integration
│   └── BrowserSessionManager.ts # Session-browser coordination
├── adapters/
│   ├── ChromiumDevToolsAdapter.ts
│   └── AppleScriptAdapter.ts
└── types/
    └── index.ts               # Shared type definitions
```

### **Event-Driven Architecture**
- **Listens**: `SystemEventType.SESSION_CREATED`, `SystemEventType.SESSION_JOINED`
- **Emits**: Browser lifecycle events for logging and monitoring
- **Coordinates**: With SessionManagerDaemon for session-browser affinity

## 🤖 **AI-HUMAN COLLABORATION DESIGN**

The Browser Manager's intelligent defaults create **cognitive amplification** through respectful automation:

### **For Humans:**
- Automatic workspace preparation when explicitly connecting
- Clean tab management without manual intervention  
- Focus assistance for direct development work

### **For AI Systems:**
- Non-intrusive background operation
- Preservation of human's browser state
- Multiple AI sessions without conflict

### **For Personas:**
- Academy-spawned personas inherit respectful defaults
- Training data collection without disrupting human workflow
- Collaborative development without interference

## 🔧 **OOP TYPE SAFETY**

The TypeScript compiler enforces respectful behavior patterns:

```typescript
// Compiler prevents focus stealing in API contexts
const portalSession = await connect({
  sessionType: 'portal',  
  focus: true  // ❌ Type error: focus defaults to false for portal sessions
});

// Encourages appropriate defaults
const humanSession = await connect({
  sessionType: 'development',
  focus: true,        // ✅ Explicit human intention
  killZombies: true   // ✅ Helpful workspace management
});
```

This design philosophy ensures the **browser manager serves both human productivity and AI autonomy** through intelligent, context-aware behavior.

## 📋 **IMPLEMENTATION STATUS**

- ✅ **Smart defaults by context** - Bash vs API behavior
- ✅ **ONE TAB POLICY enforcement** - Semaphore protection
- ✅ **Platform-specific focus control** - macOS AppleScript
- ✅ **Zombie tab detection** - AppleScript integration
- ✅ **Type-safe parameter passing** - ConnectCommand interface
- 🚧 **DevTools Protocol adapters** - Chrome/Firefox support
- 🚧 **WebSocket connection correlation** - Live tab identification
- 📋 **Linux/Windows platform support** - Cross-platform adapters

**Philosophy**: The compiler and type system become our cognitive infrastructure for designing respectful AI-human collaboration! 🤖✨