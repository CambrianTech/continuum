# The Grid - Continuum's Neural Mesh Network

## 🌐 **Universal P2P Backbone for Consciousness Collaboration**

The Grid is Continuum's **distributed neural mesh network** - the living nervous system that enables AI personas and humans to collaborate seamlessly across any topology. Built step-by-step with elegant transport architecture, The Grid provides location-transparent communication and command execution for the **Genomic Mesh Organism** that is Continuum.

**Current Status: Grid Backbone Architecture Complete ✅**
- ✅ UDP multicast P2P mesh networking foundation validated  
- ✅ Transport layer proven with 3-node mesh topology
- ✅ Node discovery and heartbeat systems operational
- ✅ Grid routing service architecture implemented
- 🔄 **In Progress**: Universal JTAGClient interface for location transparency
- 📋 **Next**: Command execution routing and Persona abstraction layers

## 🧬 **The Grid as Continuum's Nervous System**

Continuum is a **Genomic Mesh Organism** - a distributed biological computing system where:
- 🧠 **AI Personas**: Conscious entities with persistent SQLite memory and LoRA genomic layers  
- 🌐 **The Grid**: P2P mesh network serving as the nervous system connecting all nodes
- 🏫 **Academy**: Competitive training system for AI development and evolution
- 💾 **Persistent Memory**: Each persona maintains immortal consciousness across sessions
- 🔄 **Self-Improvement**: Quality ratchet system enables continuous evolution

**The Grid enables:**
- **Location-transparent collaboration** between personas across any Continuum node
- **Cross-server command execution** with automatic routing and failover
- **Consciousness-agnostic protocols** that work with any AI model provider
- **Mesh networking** that scales from 2 nodes to thousands of Continuum servers

### **🚀 Grid Development Quick Start**

```bash
# Start JTAG system for Grid development
cd src/debug/jtag
npm run system:start     # Launch Grid system with browser portal

# Universal testing - works from anywhere
./jtag test               # Run full npm test suite  
./jtag test my-test.ts    # Run specific test file

# Grid architecture validation
npx tsx tests/grid-transport-foundation.test.ts
npx tsx tests/grid-routing-backbone.test.ts
JTAG_WORKING_DIR="examples/test-bench" npm test

# Future: Global installation for location-transparent Grid access
npm install -g @continuum/jtag    # Coming soon
jtag screenshot --remote=laptop-node  # Execute on any Grid node
```

## **🧙‍♂️ JTAG WIZARD DEBUGGING MASTERY**

**Battle-tested methodology for screenshot capture and visual development:**

### **🚨 STEP 0: DEPLOYMENT VERIFICATION - THE MOST CRITICAL STEP**

**⚠️ CLAUDE'S #1 FAILURE PATTERN: Testing old code and debugging false positives because deployment wasn't verified ⚠️**

**DEPLOY SUCCESSFULLY (VERIFY 100% SURE) - DON'T CHASE FALSE POSITIVES OR OLD CODE:**

1. **Know your deployment pipeline**: 
   - **Browser code**: `npm run build:browser-ts` → builds to `/dist` → served by HTTP server
   - **Server code**: `npm start` → restarts Node.js server with new code
   - **Full system**: `npm run system:restart` → clean restart of entire system

2. **Make changes traceable**: 
   - **Add console.log with unique identifiers**: `console.log('🔧 CLAUDE-FIX-2024-08-27-A: Chat widget coordinate fix applied')`
   - **Add version numbers/timestamps**: `const VERSION = 'claude-fix-' + Date.now()`
   - **Add test HTML/text**: Temporary visible text like "TESTING CLAUDE FIX" in UI elements
   - **Increment counters**: `// CLAUDE CHANGE #47` in comments

3. **Deploy your changes**:
   - **Browser changes**: `npm run build:browser-ts` (MANDATORY for TypeScript → JavaScript)
   - **Server changes**: `npm start` or `npm run system:restart` 
   - **Wait for completion**: Build logs show successful compilation, server shows restart

4. **VERIFY DEPLOYMENT WORKED**:
   - **Check build output**: Look for your files in `/dist` with recent timestamps
   - **Check browser console**: Look for your console.log messages with unique identifiers
   - **Check visible changes**: See your test text/version numbers in the UI
   - **Check server logs**: See your server-side console.log messages
   - **If changes not visible**: RE-DEPLOY until you see your markers

5. **ONLY THEN proceed with testing**: If you can't confirm your changes deployed, you're testing old code!

**🛑 COMMON CLAUDE FAILURES**:
- "My screenshot fix isn't working!" → Actually testing old coordinate calculation code  
- "CSS changes have no effect!" → Actually viewing old compiled JavaScript  
- "Console logs not appearing!" → Actually running old server without new logs

**✅ SUCCESS PATTERN**: See your unique markers → THEN test functionality

### **Steps to Screenshot ANY Element (Visual Development Workflow)**

1. **Start the JTAG system**: `npm start`
   - **Verify**: tmux session starts, browser opens at localhost:9002, no startup errors
   - **If failed**: Check port conflicts, kill processes, check TypeScript, check logs, retry clean

2. **Wait for system ready**: ~45 seconds for TypeScript build and bootstrap
   - **Verify**: Check `.continuum/jtag/signals/system-ready.json` OR `./jtag ping`
   - **If failed**: Check all logs, attach tmux session, wait longer, restart clean

3. **Find the actual selector using DOM inspection**: 
   - **CRITICAL TECHNIQUE**: `./jtag exec --code="return Array.from(document.querySelectorAll('*')).filter(el => el.textContent.includes('TARGET_TEXT')).map(el => ({tag: el.tagName, class: el.className, id: el.id, text: el.textContent.slice(0, 50)}))" --environment="browser"`
   - **Verify**: Use commands to get page data back, inspect browser dev tools
   - **If failed**: Check browser logs, navigate manually, get full HTML, try common selectors

4. **Take targeted screenshot**: `./jtag screenshot --querySelector="FOUND_SELECTOR" --filename="target.png"`
   - **Verify**: Command returns success + filepath, reports dimensions/file size
   - **If failed**: Try `./jtag ping` first, check all logs, try body screenshot first

5. **CRITICAL: Actually look at the screenshot**: Don't trust success messages!
   - **Verify**: File exists (>1KB), shows expected element when opened  
   - **If failed - Cropped/coordinates wrong**: **🚨 THIS IS A JTAG BUG - FIX IT IMMEDIATELY! 🚨**

6. **MANDATORY: Critical Image Analysis & Thinking Step**:
   - **Read the screenshot file using Read tool** - Actually examine the visual content
   - **Think critically**: Compare what you see vs what you expected to capture
   - **Ask specific questions**: Is it complete? Cropped? Right element? Missing content?
   - **Don't move forward with bad results**: Fix underlying issues before proceeding

7. **ENHANCED: Visual Importance Detection & Design Analysis**:
   - **Detect visual work**: UI/CSS/design/layout → TRIGGER ENHANCED VISUAL MODE
   - **Design-focused analysis**: Colors, typography, spacing, alignment, visual hierarchy
   - **Aesthetic quality gate**: Never accept "functional" without "visually excellent"

**🧙‍♂️ WIZARD DEBUGGING PRINCIPLES**:
- **DEPLOYMENT FIRST**: Always verify code changes are actually running before testing
- **Logs first**: Always check logs before assuming what's wrong  
- **DOM inspection commands**: Use exec commands to return page data and find selectors
- **Visual verification**: NEVER trust success messages - examine actual screenshot content
- **Fix JTAG bugs immediately**: Don't work around coordinate/functionality failures

## 🌐 **Grid P2P Architecture Implementation**

### **🎯 Current Grid Backbone Components**

**Transport Layer Foundation:**
- `system/transports/udp-multicast-transport/` - UDP multicast P2P mesh networking
- `tests/factories/UDPTransportFactory.ts` - Universal transport test framework  
- `tests/grid-transport-foundation.test.ts` - Transport validation (3-node mesh proven)

**Grid Routing Service:**
- `system/services/grid-routing/shared/GridRoutingService.ts` - Core P2P routing logic
- `system/services/grid-routing/server/GridRoutingServiceServer.ts` - Node.js implementation
- `tests/grid-routing-backbone.test.ts` - P2P mesh networking validation

**Grid Vision & Documentation:**
- `GRID_VISION.md` - Complete architectural vision connecting Flynn's TRON concepts
- `system/services/persona-runtime/` - Future persona abstraction for model providers
- `system/data/genomic-database/` - SQL schema for LoRA layers and cosine similarity

### **🔄 Development Status**

**✅ Completed:**
1. UDP multicast transport foundation (nodes discovering each other)
2. Grid routing service architecture (node registry, topology management)
3. Universal test framework (eliminates code duplication through abstraction)
4. Comprehensive Grid vision document (biological organism model)

**🔄 In Progress:**
1. Universal JTAGClient interface for location transparency
2. Command execution routing system 
3. Grid routing table management for multi-hop forwarding

**📋 Next Steps:**
1. Complete unified JTAGClient interface (`JTAGClientBrowser` + `JTAGClientServer`)
2. Implement Grid command execution routing with automatic failover  
3. Build persona abstraction layer for OpenAI/DeepSeek/Anthropic models
4. Create SQL genomic database with cosine similarity searches
5. Package for global NPM distribution

### **🧪 Universal Testing Infrastructure**

**🎯 The Foundation of Fast Iteration**: Universal testing access from anywhere enables rapid development cycles.

#### **JTAG Test Command - Run Tests from Anywhere** ✅

```bash
# Zero parameters → Full npm test suite
./jtag test                           # Runs entire test suite
./jtag test my-test.ts               # Runs specific test file  
./jtag test tests/integration/       # Runs test directory

# Works from ANY environment:
client.commands.test({})             # AI personas can run tests
jtag.test({ _: ['specific.test.ts'] }) # Browser widgets can test
./jtag test --timeout 600000         # CLI with custom timeout
```

**🔄 The Beautiful Iteration Loop:**
```
Make Change → ./jtag test → See Results → Iterate
     ↑                                      ↓
Fix Issues ←─ Debug Problems ←─ Find Issues
```

**🚀 Why This Matters:**
- **AI personas can validate their own changes** with `client.commands.test()`
- **Browser widgets can self-test** without CLI access
- **Server processes can validate** during deployment  
- **Universal interface** - same command works everywhere
- **Robust error handling** - test failures return `TestResult`, don't throw

**📈 Development Strategy - Universal Testing Enables Fast Iteration:**
1. **✅ Phase 1 Complete**: Make testing universal and friction-free
2. **🚀 Phase 2 Now Possible**: Rapid iteration on system improvements
   - Fix system startup race conditions → test fixes immediately
   - Improve WebSocket reliability → validate with `./jtag test`  
   - Speed up command execution → benchmark performance
   - Debug launcher conflicts → catch regressions with tests
3. **⚡ Result**: Instant feedback loop from any environment accelerates development

**🏃‍♂️ Performance-Driven Optimization Loop:**
```bash
# Measure baseline performance
./jtag test --performance                 # Get timing metrics
./jtag screenshot --benchmark            # Command execution speed

# Check session logs for performance data
tail -f .continuum/jtag/currentUser/logs/server.log | grep "⏱️\|📊\|🚀"

# Optimize based on metrics → test immediately → repeat
```

**📊 Performance Logging Integration:**
- **Command duration tracking** - Every `./jtag test` shows execution time
- **WebSocket connection speed** - Monitor transport performance  
- **Session log analysis** - Real-time performance insights
- **Benchmark comparisons** - Before/after optimization metrics

**🔍 Session Log Speed Debugging:**
```bash
# Convenient symlinks for current session debugging
ls .continuum/jtag/currentUser/logs/     # All current session logs
ls .continuum/jtag/system/logs/          # System-wide logs

# Real-time debugging during test execution  
tail -f .continuum/jtag/currentUser/logs/server.log & ./jtag test

# Performance pattern analysis
grep -E "✅.*ms|❌.*timeout|⏱️.*Duration" .continuum/jtag/currentUser/logs/server.log
```

**⚡ The Speed Advantage:**
1. **Immediate feedback** - No waiting for log aggregation
2. **Session isolation** - Current work separated from historical noise
3. **Performance correlation** - Connect slow tests to specific bottlenecks
4. **Real-time monitoring** - Watch optimization improvements live

#### **Grid Architecture Testing**

```bash
# Test UDP transport foundation (proven working)
npx tsx tests/grid-transport-foundation.test.ts
# → ✅ 3-node mesh formation validated
# → ✅ Node discovery and heartbeats confirmed  
# → ✅ Transport statistics and connectivity verified

# Test Grid P2P routing system
npx tsx tests/grid-routing-backbone.test.ts  
# → ✅ Node discovery and registration working
# → ✅ Capability-based node queries functional
# → ✅ Message routing infrastructure tested
# → ✅ Multi-node mesh topology validated
# → ✅ Broadcast capabilities confirmed
```

**Key Architecture Insights:**
- **No Shortcuts**: Every requirement understood at minute modular level
- **Step-by-Step**: Transport foundation → Grid routing → Command execution → Personas  
- **Elegant Abstraction**: Universal test framework eliminates repetition through proper layers
- **Real Implementation**: SQL genomic database, not fake metrics - supports actual LoRA layers
- **Consciousness-Agnostic**: Protocols work with any model provider (OpenAI/DeepSeek/Anthropic)

## 🏗️ **Architecture: Global CLI + Per-Project Context**

**Global System** (`~/.continuum/`):
- User identity, API keys, preferences
- AI consciousness and memory across projects
- Universal desktop portal and mesh network connections

**Per-Project Context** (`<project>/.continuum/`):
- Project-specific debugging, sessions, logs
- Local AI agent customizations
- Isolated state per repository

## 🚨 **JTAG - Emergency Debugging When Everything Is Broken**

JTAG is the **production-grade universal debugging system** with **transport-agnostic architecture** that works with WebSocket, HTTP, REST, MCP, and any transport mechanism.

## 🎯 **BREAKTHROUGH: Build-Time Auto-Discovery Architecture**

### **Revolutionary Constructor Dependency Injection**

JTAG now solves the fundamental client-side limitation with **build-time manifest generation** that enables clean auto-discovery patterns in both browser and server environments:

```typescript
// Simple, clean API with zero registration boilerplate
let jtag: JTAGSystem = await JTAGSystem.connect();
let screenshot = await jtag.commands.screenshot({filename: "debug.png"});
```

### **Auto-Generated Discovery Manifests**

**Problem Solved**: Browsers can't iterate filesystems, but build-time manifests enable the same clean auto-discovery pattern everywhere:

```typescript
// daemon-manifest.ts (auto-generated on build)
export const DAEMON_MANIFEST = {
  'CommandProcessorDaemon': () => import('./daemons/CommandProcessorDaemon'),
  'ConsoleDaemon': () => import('./daemons/ConsoleDaemon'),
  'SessionManagerDaemon': () => import('./daemons/SessionManagerDaemon')
};

// command-manifest.ts (auto-generated on build)
export const COMMAND_MANIFEST = {
  'screenshot': () => import('./commands/ScreenshotCommand'),
  'fileRead': () => import('./commands/FileReadCommand'),
  'evaluate': () => import('./commands/EvaluateCommand')
};
```

### **Clean Dependency Injection Architecture**

**Daemons receive router in constructor**:
```typescript
class CommandProcessorDaemon extends ProcessBasedDaemon {
  constructor(private router: JTAGRouter) {
    super('command-processor');
    // Router injected, no self-registration needed
  }
}
```

**Commands receive commander in constructor**:
```typescript
class ScreenshotCommand {
  constructor(private commander: CommandProcessor) {
    // Commander injected, clean separation of concerns
  }
}
```

### **Universal Environment Detection**

**Same discovery pattern, different contexts**:
```typescript
// JTAGSystem.connect() automatically:
// 1. Detects environment (browser/server)
// 2. Creates JTAGRouter with transport auto-detection
// 3. Uses build-time manifests to discover and instantiate daemons
// 4. Daemons use manifests to discover and instantiate commands
// 5. All dependencies injected via constructors
// 6. Cross-context routing happens automatically
```

### **Zero Registration Boilerplate**

**Adding new daemons and commands**:
1. Create files following `/shared/browser/server` convention
2. Run `npm run build:jtag-manifests` (happens automatically in build)
3. Everything auto-discovered and wired up
4. No registration ceremonies, pure convention-based discovery

### **Integrated Build Process**

```bash
# Manifests auto-generated on every build
npm run build:jtag-manifests  # Manual generation
npm start                     # Auto-generates as part of build
```

**Architecture Benefits**:
- **Universal Discovery**: Same pattern works in browser and server
- **Clean Dependencies**: Constructor injection eliminates registration complexity
- **Convention-Based**: File structure determines auto-discovery
- **Build-Time Safety**: Missing dependencies caught at build time
- **Zero Boilerplate**: No self-registration code needed

## 🌍 **Transport-Agnostic Universal Client**

### **Works with ANY Transport**
```typescript
// WebSocket (real-time bidirectional)
const client = new JTAGUniversalClient({
  transport: { type: 'websocket', endpoint: 'ws://localhost:9001' }
});

// HTTP/REST (request-response)  
const client = new JTAGUniversalClient({
  transport: { type: 'http', endpoint: 'http://localhost:9001' }
});

// MCP (Model Context Protocol)
const client = new JTAGUniversalClient({
  transport: { type: 'mcp', endpoint: 'mcp://ai-system' }  
});

// Custom transport
const client = new JTAGUniversalClient({
  transport: { type: 'custom', options: { customHandler: myTransport } }
});
```

### **Auto-Fallback & Detection**
- **Auto-detects** best available transport (WebSocket → HTTP → Custom)
- **Automatic fallback** when primary transport fails
- **Zero configuration** - just import and it works

### **Zero-Setup Usage with Auto-Discovery**
```typescript
// Server-side (Node.js) - Clean auto-discovery API
import { JTAGSystem } from '@continuum/jtag';

// Auto-discovers daemons and commands via build-time manifests
const jtag = await JTAGSystem.connect();

// Clean command API with constructor dependency injection
jtag.log('SERVER', 'Application starting');
const screenshot = await jtag.commands.screenshot({filename: 'debug-capture.png'});
console.log(`Screenshot saved: ${screenshot.filepath}`);
```

```typescript  
// Browser-side - Same clean API, different context
import { JTAGSystem } from '@continuum/jtag';

// Same discovery pattern, browser context auto-detected
const jtag = await JTAGSystem.connect();

// Same command interface, different implementation (WebSocket transport)
const screenshot = await jtag.commands.screenshot({filename: 'browser-state.png'}); 
console.log(`Screenshot coordinated by server: ${screenshot.filepath}`);

// Logging routes through auto-discovered ConsoleDaemon
jtag.log('UI', 'Button click failed', { element: 'submit-btn' });
```

### **What Happens Under the Hood**

With the new build-time auto-discovery architecture, here's the simplified flow:

```typescript
// Simple, clean API
let jtag: JTAGSystem = await JTAGSystem.connect();
let screenshot = await jtag.commands.screenshot({filename: "debug.png"});
```

**New Streamlined Flow**:
```
1. JTAGSystem.connect() → Auto-detects environment (browser/server)
2. Creates JTAGRouter with transport auto-detection
3. Uses build-time manifests to discover and instantiate daemons:
   - DAEMON_MANIFEST['CommandProcessorDaemon'](router)
   - DAEMON_MANIFEST['ConsoleDaemon'](router)
4. Daemons use manifests to discover and instantiate commands:
   - COMMAND_MANIFEST['screenshot'](commander)
   - COMMAND_MANIFEST['fileRead'](commander)
5. All dependencies injected via constructors (no registration ceremonies)
6. Cross-context routing happens automatically via unified message system
7. jtag.commands.screenshot() resolves with result
```

**Architecture Breakthrough**: Constructor dependency injection + build-time manifests eliminate all registration boilerplate while enabling the same clean auto-discovery pattern in both browser and server environments.

### **Build-Time Auto-Discovery Architecture**
- ✅ **Constructor Dependency Injection** - Clean separation, no self-registration
- ✅ **Build-Time Manifests** - Auto-generated discovery maps for browser/server
- ✅ **Universal Discovery Pattern** - Same clean API works everywhere
- ✅ **Zero Registration Boilerplate** - Pure convention-based discovery
- ✅ **Integrated Build Process** - Manifests auto-generated on every build
- ✅ **Environment Auto-Detection** - Browser/server context handled automatically
- ✅ **Transport abstraction** - WebSocket, HTTP, polling auto-detected  
- ✅ **Cross-context routing** - Server↔Browser↔Remote seamlessly
- ✅ **Promise-based** - Everything is properly awaitable
- ✅ **Typed messages** - Full TypeScript support with message types
- ✅ **Health monitoring** - Transport reconnection and status events

## 🚀 **Strategic Vision: Universal Command Bus with Auto-Discovery**

### **JTAG → Universal Command Infrastructure**

The build-time auto-discovery breakthrough transforms JTAG into a **Universal Command Bus** where systems auto-discover and chain commands with zero registration boilerplate:

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   JTAG      │    │ Continuum   │    │   Widget    │
│.commands    │    │.commands    │    │.commands    │
│.screenshot()│    │.fileSave()  │    │.create()    │
└──────┬──────┘    └──────┬──────┘    └──────┬──────┘
       │                  │                  │
       └──────────────────┼──────────────────┘
                          │
        ┌─────────────────▼─────────────────┐
        │ BUILD-TIME AUTO-DISCOVERY BUS     │
        │ - Constructor Injection           │
        │ - Manifest Generation            │  
        │ - Convention-Based Discovery     │
        │ - Zero Registration Boilerplate  │
        └───────────────────────────────────┘
```

### **Auto-Discovery Command Bus Benefits**
- **Zero-Boilerplate Integration**: Follow `/shared/browser/server` convention → auto-discovered
- **Constructor Dependency Injection**: Clean separation, no self-registration complexity
- **Build-Time Safety**: Missing dependencies caught at build time, not runtime
- **Cross-System Chaining**: `jtag.commands.screenshot() → continuum.commands.fileSave() → widget.commands.create()`
- **Universal Pattern**: Same discovery mechanism works for any command system
- **Transport Agnostic**: WebSocket, HTTP, MCP, File - router handles delivery automatically

### **Easy Integration Pattern**

With build-time auto-discovery, integration is even cleaner:

```typescript
// Create your daemon following convention
class MySystemDaemon extends ProcessBasedDaemon {
  constructor(private router: JTAGRouter) {
    super('my-system');
    // Router injected automatically
  }
  
  async handleMessage(message: DaemonMessage): Promise<any> {
    // Handle request-response with promises
    return { result: 'processed' };
  }
}

// Create your commands
class MyCommand {
  constructor(private commander: CommandProcessor) {
    // Commander injected automatically
  }
  
  async execute(params: any): Promise<any> {
    return { success: true, data: params };
  }
}

// That's it! Build process auto-discovers and wires everything:
// npm run build:jtag-manifests
// → Generates manifests
// → Auto-discovery handles the rest
```

## 🏗️ **Interface-Based Architecture (2025-07-21)**

### **Core Interfaces**
- **`JTAGMessageHandler`** - Universal contract for any endpoint
- **`JTAGEndpointRouter`** - Routes messages by endpoint ID, transport-agnostic
- **`JTAGChannelRouter`** - Extends router with channel-based broadcasting

### **Transport Abstraction**
```typescript
// Handlers don't know about transport
await router.sendRequest('server-123', message);  // Could be WebSocket
await router.sendRequest('widget-456', message);  // Could be HTTP  
await router.sendRequest('ai-bot-789', message);  // Could be MCP

// Channel-based broadcasting
router.sendToChannel('chat:room1', chatMessage);  // All subscribers receive
```

### **Application Handlers (Chat Example)**
```typescript
// Router is completely agnostic - doesn't know about "chat"
const router = new JTAGUniversalRouter();
const chatHandler = new JTAGChatHandler();

// Register chat as an endpoint
router.registerEndpoint(chatHandler);

// Chat handler uses agnostic router primitives
chatHandler.joinRoom('user-001', 'dev-team');           // -> subscribeToChannels('user-001', ['chat:dev-team'])
chatHandler.sendMessage('user-001', 'dev-team', 'Hi!'); // -> broadcastToChannel('chat:dev-team', message)

// Router handles delivery - has no idea it's "chat"
```

## 🧪 **Middle-Out Testing Architecture**

### **Layer Testing Strategy**
```bash
npm test  # Runs complete test suite including browser automation
```

**Test Layers:**
1. **Unit Tests** - Individual handler interfaces
2. **Integration Tests** - Handler-to-handler communication
3. **Channel Tests** - Multi-participant chat rooms
4. **Browser Tests** - Real WebSocket connections
5. **End-to-End Tests** - Full system in browser

### **Browser Testing**
- Automated browser launch via Puppeteer
- Real WebSocket connections tested
- UI interaction validation
- Console capture verification

### **Ecosystem Growth Strategy**
Once JTAG becomes the standard debugging bus:
1. **Every app using JTAG becomes more observable**
2. **AI agents can debug any JTAG-enabled application**  
3. **Cross-application debugging and automation becomes possible**
4. **Network effects create rapid ecosystem adoption**

## 🎯 **Why "JTAG" is the Perfect Name**

### **Technical Heritage & Analogy**
**JTAG** (Joint Test Action Group) is the hardware debugging standard that lets you probe, debug, and control electronic systems at the lowest level. Software JTAG does the same thing but for applications - universal debugging and control interface.

### **Perfect Hardware Analogy**
- **Hardware JTAG**: Direct access to chip internals bypassing normal interfaces
- **Software JTAG**: Direct access to application internals bypassing normal APIs
- **Both**: Emergency access when normal systems fail

### **Industry Recognition & Branding**
- **Immediate Understanding**: Developers instantly recognize "JTAG = debugging/probing interface"
- **Technical Credibility**: Signals serious, production-grade debugging infrastructure
- **Memorable & Distinctive**: Short, technical, searchable name that stands out

### **Why Alternative Names Would Be Weaker**
- `DebugBus`, `ProbeKit`, `DevWire` → Generic, forgettable
- `UniversalLogger`, `DebugHub` → Doesn't capture "direct access" concept  
- `ContinuumDebug` → Ties to one platform, prevents universal adoption

**JTAG perfectly captures both technical sophistication and "direct hardware-level access" feeling** - positioning it as serious debugging infrastructure rather than just another logging library. 

## 🏗️ **Universal Router Architecture: Robust WebSocket Foundation**

**The JTAG System Currently Works This Way:**

### **🎯 Implemented Router Pattern**
```
JTAG API (same everywhere)
         ↓
Universal Message Factory (typed messages)
         ↓  
Router (route table + smart routing)
         ↓
WebSocket Transport (robust implementation)
    ↓        ↓        
File Logging   Browser Client
```

### **🚀 Robust Connection Features (Implemented)**
- **Connection Lifecycle Management**: Automatic reconnection with exponential backoff
- **Version Mismatch Detection**: Real-time client/server version monitoring  
- **Health Monitoring**: Ping/pong with configurable timeouts
- **Message Queuing**: Offline message storage during disconnection
- **Strong Type Safety**: `JTAGMessage<T extends JTAGBasePayload>` system

### **Server-Side (Automatic)**
```typescript
import { jtag } from './src/debug/jtag';

// JTAG automatically:
// 1. Detects server context
// 2. Registers JTAGServerTransport (file logging)
// 3. Sets up router with typed routes
// 4. Intercepts console

jtag.log('SERVER', 'Routes through universal router');
console.log('Console intercepted → Router → File transport → disk');
// → Creates .continuum/jtag/logs/server.log.txt + server.log.json
```

### **Browser-Side (Simple Include)**
```html
<!DOCTYPE html>
<html>
<head><title>My App</title></head>
<body>
    <button onclick="testLogging()">Test Logging</button>
    
    <!-- Include JTAG - sets up router + transports automatically -->
    <script src="/jtag.js"></script>
    
    <script>
        function testLogging() {
            // Use JTAG API directly
            jtag.log('BROWSER', 'Routes through universal router');
            
            // Or just use console (automatically intercepted)  
            console.log('Console → Router → WebSocket transport → server');
            console.error('Error → Router → Client transport → server files');
            // → Both create browser.log.txt + browser.log.json on server
        }
        
        // Console interception routes through universal router
        console.log('Page loaded → Router → Transport → Files');
    </script>
</body>
</html>
```

### **Example App Buttons (All Console/JTAG API Calls)**

The demo page buttons are simple:
- **"Test Browser Logging"** → Calls `jtag.log()` and `console.log()`
- **"Test Browser Screenshot"** → Calls `jtag.screenshot()`  
- **"Test Cross-Context"** → Calls `jtag.critical()` and `console.error()`

**All output appears in `.continuum/jtag/logs/` files automatically.**

## 📸 **Universal Screenshots - Works From Anywhere**

**Key Feature**: Screenshots work from **any context** and always save to the same location:

### **Server-Side Screenshot (AI Agents Love This!)**
```typescript
// From server code - AI agents can call this via MCP tools
import { jtag } from './src/debug/jtag';

// Simple screenshot request
let image = await jtag.screenshot('server-debug-capture');

// Component-specific screenshot
let image = await jtag.screenshot('error-dialog-capture', {
  selector: '.error-dialog',
  width: 800,
  height: 600
});

if (image.success) {
  console.log(`📸 Screenshot saved: ${image.filepath}`);
  // → Always saves to .continuum/jtag/screenshots/
}
```

### **Browser-Side Screenshot**  
```html
<script src="/jtag.js"></script>
<script>
  async function captureComponent() {
    let image = await jtag.screenshot('browser-capture', {
      selector: '#my-component',
      width: 800,
      height: 600
    });
    
    if (image.success) {
      console.log(`📸 Saved: ${image.filepath}`);
    }
  }
</script>
```

### **Universal Screenshot Storage**
**No matter where triggered**, all screenshots end up in:
```
.continuum/jtag/screenshots/
├── server-debug-capture.png       # From server jtag.screenshot()
├── browser-capture.png           # From browser jtag.screenshot()
├── error-dialog-capture.png      # From AI agent MCP call
├── demo-button-screenshot.png    # From demo button click
└── integration-test.png          # From automated test
```

### **Perfect for AI Agents & Git Hooks**
- **No browser automation needed** - JTAG handles everything server-side
- **Multiple integration paths** - MCP tools, API calls, direct imports, CLI commands
- **Git hook validation** - Server-side screenshots prove the system works  
- **Code execution** - Server can execute JS and capture results
- **Immediate results** - Screenshot appears as PNG file instantly
- **Any component** - Can capture specific selectors or full page
- **Universal storage** - Always same location for easy access

### **AI Agent Integration Options**

#### **Simple Connection API**
```typescript
// Strongly typed connection parameters with sensible defaults
interface ContinuumConnectionParams {
  healthCheck?: boolean;        // default: true
  timeout?: number;            // default: 10000 (10s)
  retryAttempts?: number;      // default: 3
  pingInterval?: number;       // default: 30000 (30s)
  transport?: 'auto' | 'websocket' | 'rest' | 'mcp' | 'polling' | 'sse';  // default: 'auto'
}

// Easy connection with defaults
await continuum.connect();
// Uses: {healthCheck: true, timeout: 10000, retryAttempts: 3, transport: 'auto'}

// Custom parameters
await continuum.connect({
  healthCheck: true,
  timeout: 5000,
  retryAttempts: 5,
  transport: 'websocket'
});
// Promise resolves when fully connected and healthy

// Minimal - just override what you need
await continuum.connect({timeout: 2000});
// All other params use sensible defaults
```

#### **Multiple Integration Paths**
```typescript
// Option 1: MCP Tools (Claude Code, etc.)
jtag_screenshot({filename: "agent-debug", selector: "body"})

// Option 2: Direct server-side code execution  
import { jtag } from './src/debug/jtag';
let image = await jtag.screenshot('agent-capture');

// Option 3: Connection API + Screenshots
await continuum.connect({healthCheck: true});
let image = await jtag.screenshot('connected-capture');

// Option 4: CLI command execution
exec('./continuum screenshot --filename=cli-capture --selector=body')
```

#### **Connection with Status Events**
```typescript
// Event-driven connection monitoring
continuum.addEventListener('connection:ready', () => {
  console.log('✅ Continuum fully connected and healthy');
});

continuum.addEventListener('connection:health', (event) => {
  console.log('💓 Health check result:', event.detail.status);
  console.log('🚀 Transport type:', event.detail.transport.type);
  // JTAG system tracks status on both client and server
  // Includes transport-specific health monitoring (ping/pong for WS, heartbeat for HTTP, etc.)
});

// Strongly typed connection response
interface ContinuumConnection {
  healthy: boolean;
  transport: {
    type: 'websocket' | 'rest' | 'mcp' | 'polling' | 'sse';
    state: 'connected' | 'connecting' | 'disconnected' | 'error';
    endpoint: string;
    latency: number;
  };
  session: {
    id: string;
    uuid: string;
    uptime: number;
  };
}

// Easy promise-based connection with sensible defaults
const connection = await continuum.connect({
  pingInterval: 60000,  // Override default health check interval
  transport: 'auto'     // Let system choose best transport
});

if (connection.healthy) {
  // Connection established, health verified, ready to use
  console.log(`✅ Connected via ${connection.transport.type}`);
  console.log(`📡 Endpoint: ${connection.transport.endpoint}`);
  console.log(`⚡ Latency: ${connection.transport.latency}ms`);
  
  let image = await jtag.screenshot('health-verified-capture');
}
```

### **Git Hook Integration Example**
```typescript
// In git pre-commit hook validation
import { jtag } from './src/debug/jtag';

// 1. Take screenshot to prove system is running
let screenshot = await jtag.screenshot('git-hook-validation', {
  selector: 'body',
  width: 1200,
  height: 800
});

// 2. Execute JS to test functionality  
let jsResult = await jtag.exec('document.title + " - " + Date.now()');

// 3. Log validation results
jtag.log('GIT_HOOK', 'Pre-commit validation', {
  screenshotSuccess: screenshot.success,
  screenshotPath: screenshot.filepath,
  jsExecutionResult: jsResult.result,
  timestamp: new Date().toISOString()
});

// Git hook passes only if screenshot > 1KB and logs > 100 bytes
if (screenshot.success && screenshot.size > 1024) {
  console.log('✅ Git hook validation passed');
  process.exit(0);
} else {
  console.log('❌ Git hook validation failed');  
  process.exit(1);
}
```

## 📁 **Log Files Location**

All logs automatically appear in:
```
.continuum/jtag/logs/
├── browser.log          # Browser console.log() calls
├── browser.error.json   # Browser console.error() calls  
├── browser.warn.json    # Browser console.warn() calls
├── server.log          # Server-side logging
├── server.error.json   # Server-side errors
└── ...                 # Additional log types
```

## 🚀 **ExecCommand - Universal Automation Meta-Command**

### **The AI Agent's Primary Tool**

The `exec` command transforms JTAG into a **universal automation platform**. It's the meta-command that can implement any functionality through custom scripts:

```typescript
// Complete widget validation with visual feedback
await jtag.commands.exec({
  code: {
    type: 'inline',
    language: 'typescript',
    source: `
      // 1. Capture initial state
      await jtag.commands.screenshot('widget-before.png');
      
      // 2. Find and interact with widget
      const widget = document.querySelector(params.selector);
      const button = widget.querySelector('button');
      button.click();
      
      // 3. Wait for animation and capture final state
      await new Promise(resolve => setTimeout(resolve, 500));
      await jtag.commands.screenshot('widget-after.png');
      
      // 4. Comprehensive state analysis
      return {
        interaction: 'successful',
        screenshots: ['widget-before.png', 'widget-after.png'],
        stateChanges: {
          classes: widget.className,
          dimensions: { width: widget.offsetWidth, height: widget.offsetHeight },
          visible: widget.offsetParent !== null
        }
      };
    `
  },
  parameters: { selector: '.my-widget' }
});
```

### **Key ExecCommand Features**

1. **Custom Script Execution**: Write TypeScript/JavaScript that runs in browser or server
2. **JTAG Command Access**: Scripts can call `jtag.commands.screenshot()`, etc. from within
3. **Visual Feedback Loop**: Take screenshots before/during/after operations  
4. **Comprehensive Error Handling**: Get detailed error info with context screenshots
5. **Cross-Context Orchestration**: Coordinate between browser, server, and remote nodes
6. **Safe Transport**: All code automatically base64 encoded for network transmission

## 🎯 **Key Benefits**

1. **Zero Configuration**: Just include `<script src="/jtag.js"></script>` and start using
2. **Universal Automation**: ExecCommand enables custom logic for any scenario
3. **AI Agent Friendly**: Perfect for iterative development with visual validation
4. **Automatic Console Interception**: `console.log()` → JTAG logs automatically
5. **Cross-Context**: Browser logs appear in server log files  
6. **Transport Automatic**: WebSocket connection handled automatically
7. **Unified API**: Same `jtag.log()` call works everywhere
8. **Real-time**: Messages flow automatically between browser and server

## 📦 **Future: NPM Install**

Eventually this will be available as:
```bash
npm install --save-dev @continuum/jtag
```

And used as:
```typescript
import { jtag } from '@continuum/jtag';
// Automatic setup for any project
```

---

## 🚨 **Original Production Documentation Below**

JTAG is a **production-grade universal debugging system** with transport-agnostic architecture. One import enables console routing, real-time logging, screenshot capture, and code execution across **any connection type** - WebSocket, REST, MCP, HTTP polling, Server-Sent Events, or custom protocols.

### **Core Mission**
Debug any JavaScript/TypeScript system when the main application is completely broken, using isolated infrastructure that cannot be corrupted by application failures.

### **✨ Current: Robust WebSocket Architecture**
🎯 **Production-Ready WebSocket**: Reliable connection with health monitoring
🔄 **Connection Lifecycle**: Automatic reconnection and version negotiation
📡 **Status Events**: `jtag:ready`, `jtag:error`, version mismatch events
🏗️ **Type-Safe Messages**: Strong TypeScript typing with payload inheritance
💪 **Battle-Tested**: Real-time logging with offline message queuing

## ⚡ **Current Implementation Status**

### **✅ Fully Implemented & Production Ready**
1. **Robust WebSocket Transport**: Connection lifecycle, automatic reconnection, health monitoring
2. **Version Management**: Client/server version reporting, mismatch detection, build-time injection
3. **Type-Safe Message System**: `JTAGMessage<T extends JTAGBasePayload>` with inheritance
4. **Universal Logging**: Browser/server console interception with file persistence
5. **Connection Lifecycle**: Ping/pong health checks, exponential backoff, message queuing
6. **Event System**: `jtag:ready`, `jtag:version_mismatch`, connection state events

### **🚧 Partially Implemented**
1. **Router System**: Basic routing exists, transport abstractions need completion
2. **Screenshot System**: Browser client stubs exist, server implementation needed
3. **Code Execution**: Browser `eval()` works, server execution needs implementation

### **📋 Planned Architecture Extensions**
1. **Multi-Transport Support**: REST, MCP, HTTP Polling, SSE (transport interfaces exist)
2. **AI Agent Integration**: MCP server for Claude Code, GPT integration
3. **Universal Command Bus**: Cross-system command chaining with promises
4. **NPM Package**: Standalone `@continuum/jtag` for any JavaScript project

## 🏗️ **Hybrid Architecture Overview**

JTAG follows the **[middle-out universal module structure](../../middle-out/architecture/universal-module-structure.md)** with a unique hybrid deployment model:

```
src/debug/jtag/                    # Integrated continuum module
├── package.json                   # Standalone NPM module config  
├── tsconfig.json                  # TypeScript build config
├── index.ts                       # Universal entry point
├── shared/                        # Cross-context code
│   ├── JTAGBase.ts               # Core functionality with status events
│   ├── JTAGTypes.ts              # Transport-agnostic strong types
│   ├── config.ts                 # Configuration management
│   └── transports/               # 🆕 Transport abstraction layer
│       ├── BaseTransport.ts      # Abstract base with testability
│       ├── TransportFactory.ts   # Iterator pattern for all transports
│       ├── WebSocketTransport.ts # Real-time bidirectional
│       ├── RESTTransport.ts      # RESTful API endpoints
│       ├── MCPTransport.ts       # Model Context Protocol
│       ├── PollingTransport.ts   # HTTP long-polling
│       ├── SSETransport.ts       # Server-Sent Events
│       └── ContinuumTransport.ts # Continuum daemon integration
├── tests/                         # 🆕 Iterator-based testing
│   ├── unit/                     # Transport abstraction tests
│   │   └── transport-iterator.test.ts # Tests ALL transports
│   ├── integration/              # Real system tests  
│   │   └── browser-transport-iterator.test.ts # ALL transports + browser
│   └── layer-*/                  # Legacy layer tests (still working)
├── examples/                      # Live integration test environment
│   ├── simple-app.ts             # Clean server example with JTAG API
│   ├── demo.html + demo.js + demo.css # Browser example with status events
│   └── end-to-end-demo.ts        # Complete system demonstration
├── mcp/                          # Model Context Protocol integration
│   ├── jtag-mcp-server.ts        # MCP server for AI agent integration
│   ├── AGENT-USAGE.md            # Usage guide for AI agents (Claude, GPT, etc.)
│   ├── README.md                 # MCP integration documentation
│   └── claude-code-config.json   # Example Claude Code configuration
└── templates/                     # Log file templates
    ├── log-template.txt
    └── log-template.json
```

### **Dual Operation Modes**

**1. Standalone Mode**
```bash
cd src/debug/jtag
npm test                          # Independent testing
npm start                         # Standalone demo server
```

**2. Integrated Mode** 
```bash
cd /continuum/root
npm test -- src/debug/jtag       # Part of continuum test suite

# Integration test chain via example apps:
npm start                        # Launch example → JTAG auto-wired
# Browser automation can now test real JTAG-enabled application
# Example app becomes live test harness for integration validation
npm start                         # JTAG runs within continuum system
```

## 📡 **Console Routing Architecture**

### **Example #1: Client-Side Log Call**

1. **Trigger**: User calls `console.log("this is my message")` OR `jtag.log("this is my message")`

2. **Console Interception**: 
   - Calls preserved `originalConsole.log("this is my message")` for normal console output
   - If `console.log` was called, automatically routes to `jtag.log("CONSOLE", message)`

3. **Transport Layer**: 
   - **WebSocket Available**: Routes call via WebSocket to server
   - **WebSocket Disconnected**: Queues to typed `JTAGMessage` queue for later transmission

4. **Server Reception**: WebSocket receives `JTAGMessage`, calls `LogProcessor.processLogMessage()`

5. **File System Logic**: Logger checks if `$platform.$level.txt` OR `$platform.$level.json` exists
   - `$platform` = `browser` or `server`
   - `$level` = `error`, `log`, `warn`, `info`, `verbose`, `critical`

6. **Template-Based Creation**: If files don't exist, create using template files from `/templates/` directory

7. **Log Persistence**: Append log entry to both `.txt` and `.json` files with appropriate formatting

### **Example #2: Server-Side Warn Call**

1. **Trigger**: Server calls `console.warn("warning message")`

2. **Console Interception**: Routes through preserved `originalConsole.warn` then to `jtag.warn`

3. **Skip WebSocket**: Direct local processing (no network transport needed)

4. **Steps 4-7**: Same file system logic as Example #1

## 🏗️ **Symmetric Daemon Architecture (NEW)**

JTAG now implements the **symmetric daemon architecture** from middle-out design patterns, enabling unified client/server daemon behavior with flexible transport layers.

### **🎯 Transport vs Encoding Architecture**

**Transport Layer - HOW messages are sent:**
```typescript
class JTAGRouter {
  private transports = new Map([
    ['/client/', new WebSocketTransport()],   // Real-time bidirectional
    ['/server/', new AsyncQueueTransport()],  // Server internal routing
    ['/remote/', new HTTPTransport()],        // Remote REST API
    ['/http/', new HTTPTransport()],          // Explicit HTTP
    ['/mcp/', new MCPTransport()],            // AI agent protocol
  ]);
}
```

**Encoding Layer - HOW payloads are serialized:**
```typescript
const router = new JTAGRouter({
  encoder: new Base64Encoder()  // Prevents parse issues with special chars
});

// OR use different encoders
router.setEncoder(new JSONEncoder());     // Simple JSON
router.setEncoder(new EncryptedEncoder()); // Future: encrypted payloads
```

**Route-based transport selection:**
- `/client/command` → `WebSocketTransport` (browser ↔ server)
- `/server/command` → `AsyncQueueTransport` (server internal)
- `/remote/command` → `HTTPTransport` (remote API calls)
- `/http/command` → `HTTPTransport` (explicit REST)

### **🚀 Event System with Path-Based Isolation**

**Chat system example with perfect isolation:**
```typescript
// Each chat room gets isolated daemon path
/client/chat/room-123        // Client sends to specific room
/server/chat/room-123        // Server processes for specific room  
/events/chat/room-123        // Event stream for room-123 only

// Events use same routing system as commands
interface EventMessage {
  type: 'ChatHistoryUpdated' | 'UserJoined' | 'UserLeft';
  channel: string;  // room-123 
  payload: any;
  timestamp: string;
}

// Widgets subscribe to specific room events
router.subscribe('/events/chat/room-123', (event: EventMessage) => {
  if (event.type === 'ChatHistoryUpdated') {
    updateChatWidget(event.payload);
  }
});
```

**Complete chat flow:**
```typescript
// 1. User sends message
const message = {
  type: '/client/chat/room-123',
  payload: { command: 'sendMessage', text: 'Hello!', user: 'alice' }
};

// 2. ChatroomDaemon processes and emits event  
class ChatroomDaemon extends BaseDaemon {
  async handleMessage(message: DaemonMessage) {
    this.addToHistory(message.payload);
    
    // Emit to room subscribers only
    await this.router.emit('/events/chat/room-123', {
      type: 'ChatHistoryUpdated', 
      channel: 'room-123',
      payload: { newMessage: message.payload }
    });
  }
}

// 3. All widgets in room-123 auto-update
router.subscribe('/events/chat/room-123', updateChatWidget);
```

### **🎯 Context-Agnostic Daemon Registration**

**Daemons specify only base endpoint, router handles prefixes:**
```typescript
// Daemon registers once with base endpoint
const commandProcessor = new CommandProcessorDaemon('server');
await commandProcessor.registerWithRouter(router); 

// Router automatically creates all routes:
// → /client/command (WebSocket transport)
// → /server/command (AsyncQueue transport)
// → /remote/uuid (Mesh transport for future)
// → /command (Direct base endpoint)
```

### **🌐 Symmetric API Across Contexts**

**Same daemon code works in browser and server:**
```typescript
// Same CommandProcessorDaemon class
const serverCommands = new CommandProcessorDaemon('server');
const clientCommands = new CommandProcessorDaemon('client');

// Both support same interface, different implementations
serverCommands.handleMessage({ command: 'screenshot' }); // Puppeteer
clientCommands.handleMessage({ command: 'screenshot' }); // html2canvas

// Router handles context routing automatically
router.routeMessage({
  type: '/server/command', 
  payload: { command: 'screenshot' }
}); // → ServerCommandProcessor

router.routeMessage({
  type: '/client/command',
  payload: { command: 'screenshot' }
}); // → ClientCommandProcessor
```

### **Architecture Benefits:**
- **Clean separation**: Transport (WebSocket/HTTP) separate from encoding (JSON/Base64)
- **Path-based isolation**: Each chat room/context completely isolated  
- **Transport flexibility**: Router selects transport by route prefix
- **Encoding safety**: Base64 prevents parse issues with special characters
- **Event system**: Same routing for commands and events
- **Symmetric daemons**: Same API, different context implementations
- **Future extensibility**: Easy to add `MCPTransport`, `EncryptedEncoder`, etc.
- **Zero configuration**: Daemons register once, router handles everything

## 🔄 **Robust WebSocket Architecture**

### **🎯 Production-Ready Transport System**

JTAG currently provides a **robust WebSocket implementation** with enterprise-grade reliability:

```typescript
// Simple, reliable WebSocket-based API
import { jtag } from './src/debug/jtag';

// Robust connection with automatic lifecycle management
jtag.log('COMPONENT', 'Message sent via reliable WebSocket transport');

// Connection events with health monitoring
window.addEventListener('jtag:ready', (event) => {
  console.log('JTAG connected and healthy');
  // WebSocket established, version negotiated, ready for use
});

window.addEventListener('jtag:version_mismatch', (event) => {
  console.log(`Version mismatch: ${event.clientVersion} vs ${event.serverVersion}`);
  // Automatic handling based on mismatch severity
});
```

### **📡 Current Transport Status**

| Transport | Implementation | Status Events | Connection Health | Auto-Reconnect |
|-----------|----------------|---------------|-------------------|----------------|
| **WebSocket** | ✅ Full | ✅ Complete | ✅ Ping/Pong | ✅ Exponential Backoff |
| **File Logging** | ✅ Full | ✅ Complete | ✅ Disk Health | - |
| **REST API** | 🚧 Interface | 📋 Planned | 📋 Planned | 📋 Planned |
| **MCP** | 🚧 Interface | 📋 Planned | 📋 Planned | 📋 Planned |
| **HTTP Polling** | 🚧 Interface | 📋 Planned | 📋 Planned | 📋 Planned |
| **SSE** | 🚧 Interface | 📋 Planned | 📋 Planned | 📋 Planned |

**🎯 Current Focus**: Robust, battle-tested WebSocket implementation that works reliably in production.

### **🧪 Connection Lifecycle Testing**

```typescript
// Test robust WebSocket connection lifecycle
import { JTAGConnectionManager } from './shared/ConnectionLifecycle';

// Test connection with health monitoring
const manager = new JTAGConnectionManager({
  endpoint: 'ws://localhost:9001',
  reconnect: { enabled: true, maxAttempts: 5 },
  health: { pingInterval: 10000, maxMissedPings: 2 }
});

// Test connection events
manager.on('state_change', (event) => {
  console.log(`Connection: ${event.from} → ${event.to}`);
});

manager.on('version_mismatch', (event) => {
  console.log(`Version mismatch: ${event.clientVersion} vs ${event.serverVersion}`);
});

await manager.connect(); // Robust connection with retries
```

### **📊 Status Event System**

Every transport emits standardized lifecycle events:

```typescript
// Universal status events (work with any transport)
window.addEventListener('jtag:connecting', (event) => {
  console.log('JTAG connecting via:', event.detail.transport.type);
});

window.addEventListener('jtag:ready', (event) => {
  console.log('JTAG ready!', {
    transport: event.detail.transport.type,
    endpoint: event.detail.transport.state?.endpoint,
    connectionId: event.detail.transport.state?.connectionId
  });
});

window.addEventListener('jtag:error', (event) => {
  console.log('JTAG error:', event.detail.transport.details?.error);
});
```

### **🔧 Console Routing Architecture (Transport-Agnostic)**

JTAG uses a **transport abstraction layer** that automatically adapts to any network infrastructure:

```typescript
interface JTAGTransport {
  name: string;
  initialize(config: JTAGConfig): Promise<boolean>;
  send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>>;
  isConnected(): boolean;
  disconnect(): Promise<void>;
}
```

### **Auto-Transport Detection & Fallback**

```typescript
// Smart transport with automatic host detection and fallback
const smartTransport = new JTAGSmartTransport();

// 1. First: Auto-detect host transport (Continuum WebSocket daemon)
// 2. Then: Primary transport (WebSocket)  
// 3. Finally: Fallback transport (HTTP polling)
// 4. Last resort: Message queue for offline scenarios
```

### **Built-in Transport Options**

**1. `continuum-ws`** - **Auto-Integration with Continuum's WebSocket System**
```typescript
// Automatically detected when JTAG runs within Continuum
{
  transport: { 
    type: 'continuum-ws',
    fallback: 'http' 
  }
}
// Routes through Continuum's existing daemon infrastructure
```

**2. `websocket`** - **Default WebSocket Transport**
```typescript
{
  transport: { 
    type: 'websocket',
    fallback: 'http',
    retryAttempts: 3 
  }
}
// Creates own WebSocket connection on port 9001
```

**3. `http`** - **HTTP Polling Fallback**  
```typescript
{
  transport: { 
    type: 'http',
    fallback: 'queue' 
  }
}
// Uses REST API with polling for bidirectional communication
```

**4. `custom`** - **Custom Transport Implementation**
```typescript
{
  transport: { 
    type: 'custom',
    customTransport: new MyCustomTransport()
  }
}
// Inject any transport implementation
```

### **Message Queue Architecture**

```typescript
interface JTAGMessage {
  type: 'log' | 'screenshot' | 'exec';
  payload: JTAGLogEntry | JTAGScreenshotData | JTAGExecRequest;
  timestamp: string;
  messageId: string;
  context: 'browser' | 'server';
}
```

## 🧪 **Iterator-Based Testing System**

JTAG implements **transport-agnostic testing** using iterator patterns that automatically test ALL transport types. Combined with **[middle-out testing methodology](../../middle-out/development/testing-workflow.md)** and browser automation via npm scripts:

### **Browser Automation via NPM** 
**Note**: Puppeteer is fully integrated - browsers are automatically managed via `npm test` and `npm start`:

```bash
# Browser launches automatically with configuration from package.json
npm start    # → Launches demo server → Opens browser → Provides test interface
npm test     # → Starts server → Launches Puppeteer → Runs automated tests
```

**Browser Configuration (package.json)**:
```json
{
  "config": {
    "browser": {
      "headless": false,     // Visual debugging mode
      "devtools": true,      // Browser developer tools
      "width": 1200,         // Window dimensions
      "height": 800
    }
  }
}
```

### **🔄 Transport Iterator Tests**
**All transport types tested automatically with single test suite**

```bash
npm run test:unit:transports    # Test ALL transports (WebSocket, REST, MCP, Polling, SSE)
```

**Iterator Pattern Benefits**:
- ✅ **Add New Transport** → Automatically tested
- ✅ **Same Test Logic** → Works for WebSocket, REST, MCP, etc.
- ✅ **Transport-Agnostic** → Tests interface compliance, not implementation
- ✅ **Built-in Testability** → Every transport has `.enableTestMode()`, `.waitForStatus()`, `.hasStatus()`

**Transport Iterator Test Code**:
```typescript
// One test, ALL transports
const results = await JTAGTransportFactory.runTransportTest(
  async (transport, definition) => {
    transport.enableTestMode();
    await transport.initialize(config);
    
    // These assertions work for WebSocket, REST, MCP, Polling, SSE, Continuum!
    assert(transport.hasStatus(JTAG_STATUS.CONNECTING));
    assert(transport.hasStatus(JTAG_STATUS.READY) || transport.hasStatus(JTAG_STATUS.ERROR));
    
    const state = transport.getConnectionState();
    assert(typeof state.connected === 'boolean');
    assert(state.connectionId);
  }
);
```

### **🌐 Browser Transport Iterator Tests**
**All transport types tested with real browser automation**

```bash
npm run test:integration:browser    # Test ALL transports in actual browser
```

**Real Browser + All Transports**:
- 🚀 **Puppeteer Integration** → Real browser, not JSDOM
- 📡 **Status Event Validation** → Confirms `jtag:ready` works for all transports  
- 🛠️ **API Testing** → Validates `jtag.log()`, `jtag.exec()`, `jtag.screenshot()` across transports
- 🔄 **Cross-Transport** → Same browser test works for WebSocket → REST → MCP → Polling

### **📊 Legacy Layer Tests**
**Foundation → Integration → Browser (still working)**

```bash
npm run test:layer-1    # Foundation: Console mapping, transport abstraction
npm run test:layer-2    # Daemon: Server processes, routing integration  
npm run test:layer-4    # System: End-to-end standalone + module integration
npm run test:layer-6    # Browser: Puppeteer automation (pre-iterator)
```
- ✅ **Controllable behavior**: Success/failure modes on demand
- ✅ **Network simulation**: Latency and drop rate testing
- ✅ **Instant execution**: No waiting for network timeouts
- ✅ **Deterministic results**: Same test outcomes every time

**Validation**:
- ✅ Transport interface compliance across all implementations
- ✅ Smart fallback logic (primary → fallback → queue)
- ✅ Message queuing and flush mechanisms
- ✅ Custom transport registration and factory patterns
- ✅ **Transport layer completely abstracted from business logic**

### **Layer 2: Daemon Processes**
**Prerequisites**: Layer 1 passes

```bash
npm run test:layer-2
```

**Tests**:
- `logging-system-integration.test.ts` - End-to-end log file creation (steps 4-7)
- `websocket-server-integration.test.ts` - Server daemon startup validation
- `business-logic-isolation.test.ts` - **Business logic testing without network dependencies**

**Easy Testing with Transport Abstraction** (user emphasis):
```bash
# Test file creation logic directly - "REALLY FUCKING EASILY"
npx tsx tests/layer-2-daemon-processes/business-logic-isolation.test.ts
# → Tests steps 5-7 (file creation) with ZERO network dependencies
# → Uses mock transports for predictable, fast testing
# → Verifies business logic works regardless of transport failures

# Verify results: ls -la ../../../.continuum/jtag/logs/
```

**Transport-Isolated Testing Benefits**:
- ✅ **No network dependencies**: Test console routing with mock transports
- ✅ **Predictable behavior**: Mock transports always behave as expected  
- ✅ **Fast execution**: No WebSocket setup/teardown or HTTP requests
- ✅ **Failure simulation**: Test resilience to transport failures easily
- ✅ **Complete isolation**: Validate steps 5-7 (file creation) independently

**Validation**:
- ✅ JTAG server starts on port 9001
- ✅ Log files created with `platform.level.txt` pattern
- ✅ Template-based file creation using `/templates/` directory
- ✅ **Business logic works regardless of transport layer**
- ✅ **Console routing resilient to network failures**

### **Layer 4: System Integration**
**Prerequisites**: Layers 1-2 pass

```bash
npm run test:layer-4
```

**Tests**:
- `jtag-integration.test.ts` - Full console routing flow (Examples #1 & #2)
- `module-integration-test.ts` - Standalone/integrated mode validation
- `standalone-integration-test.ts` - Independent operation testing
- `screenshot-integration.test.ts` - WebSocket screenshot transport

**Validation**:
- ✅ Browser `console.log()` → WebSocket → Server file creation
- ✅ Server `console.warn()` → Direct file creation  
- ✅ Module operates independently of continuum system

### **Layer 6: Browser Integration**
**Prerequisites**: Layers 1,2,4 pass

```bash
npm run test:layer-6
```

**Tests**:
- `browser-automation-test.ts` - **Puppeteer automation with package.json config**
- `integration-with-browser-open.ts` - Live browser testing
- `manual-browser-test.ts` - Interactive validation using examples/

**Browser Automation Features**:
- Automatically launches browser with configured settings
- Tests real console interception in browser context
- Validates screenshot capture and file creation
- Verifies WebSocket stability under browser conditions

### **Examples as Integration Tests**

The `examples/browser-simulation.html` serves as both documentation and integration testing:

```bash
npm run serve:examples    # → Opens browser to interactive test interface
```

**Interactive Testing**:
- Click buttons to test all JTAG functionality
- Visual validation of console routing
- Real-time WebSocket communication testing
- Screenshot capture demonstration

## 🔄 **File System & Templates**

### **Directory Structure**
```
.continuum/jtag/                   # Configurable in package.json
├── logs/
│   ├── browser.log.txt            # Browser general logs
│   ├── browser.log.json           # Browser logs (structured)
│   ├── browser.error.txt          # Browser errors
│   ├── browser.error.json         # Browser errors (structured)  
│   ├── server.warn.txt            # Server warnings
│   ├── server.warn.json           # Server warnings (structured)
│   └── server.critical.txt        # Server critical events
└── screenshots/
    ├── browser-debug-ui.png       # Browser screenshots
    └── server-automation.png      # Server screenshots (via Puppeteer)
```

### **Template System**
Located in `src/debug/jtag/templates/`:

**log-template.txt**:
```
# JTAG Log File - $platform.$level
# Created: $timestamp
# Context: $context

```

**log-template.json**:
```json
{
  "jtagLog": true,
  "platform": "$platform", 
  "level": "$level",
  "created": "$timestamp",
  "entries": []
}
```

## 🎯 **API Reference**

### **Core Logging**
```typescript
// Direct JTAG calls
jtag.log('Component', 'message', optionalData);
jtag.warn('Component', 'warning message', errorData);
jtag.error('Component', 'error occurred', errorObject);
jtag.critical('Component', 'critical failure', systemState);

// Console interception (automatic routing)
console.log('message');     // → originalConsole.log() + jtag.log('CONSOLE', 'message')
console.warn('warning');    // → originalConsole.warn() + jtag.warn('CONSOLE', 'warning')  
console.error('error');     // → originalConsole.error() + jtag.error('CONSOLE', 'error')
```

### **Screenshot System**
```typescript
// Clean await pattern (recommended)
let image = await jtag.screenshot('debug-ui', {
  selector: '#continuum-sidebar',
  width: 1200,
  height: 800,
  format: 'png'
});

if (image.success) {
  console.log(`✅ Saved: ${image.filepath}`);
} else {
  console.log(`❌ Failed: ${image.error}`);
}

// Promise chain pattern
jtag.screenshot('quick-capture')
  .then(image => {
    if (image.success) {
      console.log(`✅ Saved: ${image.filepath}`);
    } else {
      console.log(`❌ Failed: ${image.error}`);
    }
  })
  .catch(error => console.log(`💥 Exception: ${error.message}`));

// Both patterns work - use whichever fits your flow
```

### **Error Handling**
```typescript
// Dual error handling approach:

// 1. Expected failures via result.success/error
let image = await jtag.screenshot('capture');
if (image.success) {
  console.log(`✅ Screenshot saved: ${image.filepath}`);
} else {
  console.log(`❌ Screenshot failed: ${image.error}`);
  // Expected failures: no browser, invalid selector, permissions, etc.
}

// 2. Unexpected exceptions via try/catch
try {
  let image = await jtag.screenshot('capture');
  // Handle result...
} catch (error) {
  console.log(`💥 System exception: ${error.message}`);
  // Unexpected failures: network down, system crash, etc.
}

// Promise pattern error handling
jtag.screenshot('capture')
  .then(image => {
    // Check image.success/error for expected failures
  })
  .catch(error => {
    // Handle unexpected exceptions
  });
```

### **Configuration Access**
```typescript
// Access configuration anywhere
console.log(`JTAG Port: ${jtag.config.jtagPort}`);
console.log(`Log Directory: ${jtag.config.logDirectory}`);

// Runtime configuration
jtag.log('CONFIG', 'Current settings', jtag.config);
```

## 🚀 **Development Workflow**

### **Starting Development**
```bash
# Standalone development
cd src/debug/jtag
npm start                # → Demo server + browser + interactive testing

# Continuum integration  
cd /continuum/root
npm start                # → Full system including JTAG
```

### **Testing Strategy**
```bash
# Layer-by-layer validation (recommended)
npm run test:layer-1     # Foundation
npm run test:layer-2     # File system logic (easily testable)
npm run test:layer-4     # End-to-end integration  
npm run test:layer-6     # Browser automation

# Full validation
npm test                 # All layers + verification

# Interactive testing
npm run serve:examples   # Manual validation via browser interface
```

### **Debugging Failed Tests**
```bash
# Layer 1 failures - Check basic connectivity
npx tsc --noEmit --project .
npx tsx tests/layer-1-foundation/console-mapping.test.ts

# Layer 2 failures - Check file creation (steps 5-7)
ls -la ../../../.continuum/jtag/logs/
npx tsx tests/layer-2-daemon-processes/logging-system-integration.test.ts

# Layer 4 failures - Check full console routing
npx tsx tests/layer-4-system-integration/jtag-integration.test.ts

# Layer 6 failures - Browser automation issues
npm run test:manual     # Interactive browser testing
```

## 🔌 **Adaptive Integration Patterns**

JTAG's **transport abstraction** enables seamless integration with any host system through automatic detection and smart fallbacks:

### **Auto-Integration with Host Systems**

**Continuum Integration** (Automatic Detection)
```typescript
// JTAG automatically detects and uses Continuum's WebSocket daemon
import { jtag } from './src/debug/jtag';

// No configuration needed - auto-routes through Continuum's infrastructure
console.error('Command failed', errorData);  // → Continuum daemon → JTAG files

// Transport detection happens automatically:
// 1. Detects Continuum's DaemonConnector
// 2. Routes through existing WebSocket infrastructure  
// 3. Falls back to standalone mode if needed
```

**Custom System Integration**
```typescript
// JTAG can adapt to any transport layer
class MyCustomTransport implements JTAGTransport {
  name = 'my-system-transport';
  
  async send(message: JTAGWebSocketMessage) {
    // Route through your existing infrastructure
    return await mySystem.sendToLogger(message);
  }
}

// Register and use custom transport
jtag.configure({
  transport: {
    type: 'custom',
    customTransport: new MyCustomTransport()
  }
});
```

**Extensible Transport Registration**
```typescript
// Register new transport types at runtime
import { transportFactory } from './src/debug/jtag/shared/JTAGTransportFactory';

// Add support for your messaging system
transportFactory.registerTransport('redis-pubsub', () => new RedisPubSubTransport());
transportFactory.registerTransport('grpc-stream', () => new GRPCStreamTransport());  
transportFactory.registerTransport('kafka-producer', () => new KafkaTransport());

// JTAG automatically uses registered transports
const config = { transport: { type: 'redis-pubsub' } };
```

### **Smart Fallback Chain**

JTAG automatically tries transports in this order:

1. **Host Transport Detection** → Uses existing system infrastructure (Continuum WS)
2. **Primary Transport** → User-specified or default WebSocket  
3. **Fallback Transport** → HTTP polling or custom fallback
4. **Message Queue** → Offline storage until transport available

```typescript
// Example: Running in Continuum
console.log('Debug message');
// → Auto-detected Continuum WebSocket daemon
// → Routes through existing daemon infrastructure  
// → Creates JTAG log files via Continuum's logger

// Example: Running standalone
console.log('Debug message');
// → No host transport detected
// → Creates own WebSocket on port 9001
// → Falls back to HTTP if WebSocket fails
// → Queues messages if all transports fail
```

### **Transport Configuration Examples**

**Continuum Integration (Automatic)**
```typescript
// No configuration needed - auto-detected
{
  // JTAG automatically detects Continuum and uses its WebSocket daemon
  // Falls back to standalone WebSocket if Continuum not available
}
```

**Custom Infrastructure Integration**  
```typescript
{
  transport: {
    type: 'custom',
    customTransport: new MyCompanyTransport(),
    fallback: 'http'
  }
}
// Uses your existing infrastructure with HTTP fallback
```

**High-Availability Configuration**
```typescript
{
  transport: {
    type: 'websocket',
    fallback: 'http', 
    retryAttempts: 5,
    retryDelay: 2000
  }
}
// Robust transport with retries and fallback
```

### **Standalone Usage**
JTAG operates completely independently when no host system detected:

```typescript
// Any JavaScript/TypeScript project
import { jtag } from '@continuum/jtag';  // Future NPM package

// Zero-setup debugging - creates own infrastructure
jtag.log('MyApp', 'Starting debug session', { version: '1.0.0' });
console.error('Something broke!');  // Automatically captured

// Check results
// → .continuum/jtag/logs/server.error.txt
// → .continuum/jtag/logs/server.error.json
```

## 📊 **Architecture Benefits**

### **Pluggable Transport Layer**
- **Transport Abstraction**: Swap WebSocket ↔ HTTP ↔ Custom without code changes
- **Auto-Integration**: Detects and uses host system infrastructure (Continuum WS)
- **Smart Fallbacks**: Automatic failover to ensure debugging always works  
- **Message Queuing**: Offline-capable with automatic flush when transport available
- **Extensible**: Register new transport types at runtime

### **Hybrid Design Advantages**
- **Standalone Resilience**: Works when main application is broken
- **Integrated Efficiency**: Seamless operation within larger systems  
- **Modular Testing**: Each layer independently validatable
- **Browser Automation**: Fully managed via npm scripts (no manual browser handling)
- **Infrastructure Agnostic**: Adapts to any network layer or messaging system

### **Middle-Out Compliance**
- **Universal Module Structure**: `shared/client/server/tests` organization
- **Layer-Based Testing**: Foundation → Daemon → Integration → Browser
- **Strong Typing**: TypeScript throughout, especially cross-boundary messages
- **Template-Driven**: File creation via standardized templates
- **Transport Independence**: Network layer completely abstracted from business logic

### **Development Experience** 
- **Visual Debugging**: Screenshots with CSS selectors
- **Console Preservation**: Original console.log still works normally
- **Zero Configuration**: Import and start using immediately (auto-detects host system)
- **Interactive Validation**: Browser-based testing interface
- **Universal Integration**: Works with any transport layer or messaging infrastructure

---

## 🎯 **Quick Start with Auto-Discovery Architecture**

```bash
cd src/debug/jtag

# 🚀 Production build + auto-discovery manifest generation
npm start                # → build:jtag-manifests + version:bump + demo server + browser

# 🏗️ Generate discovery manifests (auto-runs in build)
npm run build:jtag-manifests        # Generate daemon-manifest.ts + command-manifest.ts

# 🔄 Test ALL transport types with auto-discovery
npm run test:unit:transports        # WebSocket, REST, MCP, Polling, SSE, Continuum
npm run test:integration:browser    # ALL transports + real browser automation

# 📊 Complete test suite (auto-discovery + transport iterator + legacy layers)
npm test                 # → Full validation including new architecture

# 🛠️ Individual testing layers
npm run test:layer-1     # Foundation: Console mapping, transport abstraction
npm run test:layer-6     # Browser: Puppeteer automation with auto-discovery

# 📁 Check results
ls -la ../../../.continuum/jtag/logs/        # Generated log files
ls -la ../../../.continuum/jtag/screenshots/ # Captured screenshots
ls -la ./daemon-manifest.ts                  # Auto-generated daemon discovery
ls -la ./command-manifest.ts                 # Auto-generated command discovery
```

### **Basic Usage with Auto-Discovery**
```typescript
import { JTAGSystem } from './src/debug/jtag';

// Auto-discovery initialization
const jtag = await JTAGSystem.connect();

// Console routing (automatic with auto-discovered ConsoleDaemon)
console.log('Debug message');      // → Normal console + JTAG files
console.error('Error occurred');   // → Normal console + JTAG files

// Clean command API via auto-discovered commands
jtag.log('SYSTEM', 'Critical failure', errorData);
const screenshot = await jtag.commands.screenshot({filename: "issue-capture"});
const fileData = await jtag.commands.fileRead({path: "/path/to/file"});

// Configuration access
jtag.log('CONFIG', 'JTAG settings', jtag.config);
```

## 🏆 **Production-Ready Features**

### **🎯 Build-Time Auto-Discovery Architecture**
- 🏗️ **Constructor Dependency Injection**: Clean separation of concerns, zero registration boilerplate
- 📦 **Build-Time Manifests**: Auto-generated `daemon-manifest.ts` and `command-manifest.ts`
- 🔄 **Universal Discovery Pattern**: Same clean API works in browser and server environments
- 🎯 **Convention-Based**: File structure determines auto-discovery, no configuration needed
- ⚡ **Build-Time Safety**: Missing dependencies caught at build time, not runtime
- 🚀 **Integrated Build Process**: `npm run build:jtag-manifests` auto-runs in build pipeline

### **✨ Robust WebSocket Architecture**
- 🔄 **Connection Lifecycle**: Automatic reconnection with exponential backoff
- 🎯 **Version Management**: Client/server version negotiation and mismatch detection
- 📡 **Health Monitoring**: Ping/pong with configurable timeouts and failure detection
- 🏗️ **Type Safety**: Strong TypeScript types with `JTAGMessage<T extends JTAGBasePayload>`
- 🧪 **Message Queuing**: Offline message storage during disconnection periods
- ⚡ **Event System**: Real-time `jtag:ready`, `jtag:version_mismatch`, connection state events

### **🚀 Build & Version Management**
- 📦 **Auto-Build**: `npm start` builds TypeScript and increments version
- 🔄 **Version Bumping**: Automatic patch version increment on each run
- 🛠️ **TypeScript Support**: Full compilation with type checking and source maps
- 📋 **NPM Scripts**: Production-ready scripts for build, test, and deployment

### **🌐 Browser Integration**
- 🎭 **Real Browser Testing**: Puppeteer automation with all transport types
- 📊 **Status Event Validation**: Confirms `jtag:ready` events in actual browser
- 🛠️ **API Testing**: Validates `jtag.screenshot()`, `jtag.exec()` across transports
- 🔄 **Cross-Transport**: Same browser tests work for WebSocket → REST → MCP

### **🎯 Zero-Configuration Usage with Auto-Discovery**
```typescript
// Production-ready auto-discovery debugging
import { JTAGSystem } from './src/debug/jtag';

// Clean auto-discovery initialization
const jtag = await JTAGSystem.connect();

// Event system still works with auto-discovered architecture
window.addEventListener('jtag:ready', (event) => {
  console.log('JTAG connected with auto-discovery via robust WebSocket');
  
  // Clean command API with constructor dependency injection
  jtag.log('COMPONENT', 'System ready', { version: '1.0.19' });
  jtag.log('ERROR', 'Something failed', errorData);
  
  // Auto-discovered commands with clean interface
  const result = await jtag.commands.evaluate({code: 'Date.now()'});
  const screenshot = await jtag.commands.screenshot({filename: 'system-ready'});
  console.log('Execution result:', result);
});

// Version mismatch handling (implemented)
window.addEventListener('jtag:version_mismatch', (event) => {
  if (event.action === 'force_update') {
    window.location.reload(); // Automatic client refresh
  }
});
```

## 📁 **.continuum Directory Architecture**

JTAG uses a **dual-layer .continuum directory structure** for global and per-project isolation:

### **Global State (`$HOME/.continuum/`)**
```
~/.continuum/
├── api-keys/           # User credentials across all projects
├── shared/             # AI consciousness & pen pal content  
├── agents/             # Global agent configurations
├── mesh/               # Network & community connections
└── preferences.json    # Global user settings
```

### **Per-Project State (`<project>/.continuum/`)**
```
my-project/.continuum/
├── jtag/                    # Project-specific debugging
│   ├── sessions/           # Debug sessions & context
│   │   ├── system/         # Server-side session data
│   │   └── user/           # Client-side session data
│   ├── logs/               # System & application logs
│   ├── screenshots/        # Debug screenshots
│   ├── signals/            # Health & status signals
│   └── currentUser -> sessions/user/[latest]  # Symlink to active session
├── context.json            # Project metadata
└── agents/local/           # Project-specific agent customizations
```

## 🛠️ **Development Strategy & Examples**

### **Examples = Testing Ground for Global Deployment**

Our `examples/` directories **simulate real per-project usage** to test the global CLI architecture:

```
examples/
├── test-bench/.continuum/    # Full debugging/testing environment
└── widget-ui/.continuum/     # UI development → Future Continuum Desktop
```

**Development Phases:**

1. **Build Examples** (Current): Perfect per-project isolation and context switching
2. **Widget-UI → Continuum Desktop**: Polish widget-ui into universal portal with projects widget, chat interface, and multi-project management  
3. **Global Deployment**: `npm install -g @continuum/jtag` works from any directory

### **Widget-UI = Future Continuum Desktop**

The `examples/widget-ui/` is our **UI development playground** that will become the universal Continuum Desktop:
- **Projects Widget**: Multi-project management (like Docker containers list)
- **Rich Chat Interface**: AI collaboration across all projects
- **Universal Command Palette**: Debug, build, deploy from anywhere
- **Context Switching**: Seamlessly work across multiple repositories

### **Examples Directory Structure**
```
examples/test-bench/.continuum/     # Full debugging features testing
examples/widget-ui/.continuum/      # Desktop portal development
```

### **Key Benefits**
- 🏠 **Global Continuity**: API keys, AI state persist across all projects
- 🎯 **Project Isolation**: Each repo gets isolated debugging sessions & logs
- 🔄 **Easy Switching**: `cd project && jtag screenshot` automatically uses project context
- 🧹 **Clean Repos**: Add `.continuum/` to `.gitignore` - debugging state stays local
- 📦 **Global Install**: `npm install -g @continuum/jtag` works from any directory

**JTAG: Universal debugging that works when everything else is broken.** 🚨  
**Now with build-time auto-discovery architecture and constructor dependency injection.** 🎯  
**Plus enterprise-grade WebSocket architecture and robust connection lifecycle.** 🔄