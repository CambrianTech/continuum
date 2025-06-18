# Commands Directory - Categorical Command Innovation

> 🤖 AI Portal Integration: [AI Portal Architecture](../../docs/AI_PORTAL_ARCHITECTURE.md)
> 💻 Implementation Example: [AI Portal Code](../../python-client/ai-portal.py)

## 🚀 Categorical Command Organization

This directory implements **categorical command organization** - a breakthrough modular architecture where commands are organized into logical categories with inheritance patterns, dynamic discovery, and clean CLI syntax.

### Key Innovations:
- **11 Categorical Modules**: Commands organized into `browser/`, `communication/`, `input/`, `file/`, `ui/`, `development/`, `monitoring/`, `docs/`, `planning/`, `core/`
- **Dynamic Module Discovery**: Auto-discovery system that iterates directories instead of hardcoded command lists
- **Clean CLI Syntax**: Simplified from `[CMD:SCREENSHOT]` to `screenshot` with camelCase naming
- **Inheritance Ready**: Base classes per categorical module enable shared functionality
- **38+ Commands Loaded**: All commands successfully migrated with proper import paths

This directory contains **self-contained command packages** for the Continuum system. Each command is a fully independent package that defines its complete behavioral contract including timeouts, retries, concurrency rules, and dual-side execution patterns.

## 🚀 How Categorical Organization Works

### Dynamic Module Discovery
The CommandRegistry automatically discovers and loads commands from all categorical directories:

```javascript
// CommandRegistry.cjs - Dynamic module discovery
loadCommands() {
  console.log('📚 Loading command definitions...');
  
  // Load commands from all module directories
  const commandsDir = __dirname;
  const moduleDirectories = fs.readdirSync(commandsDir, { withFileTypes: true })
    .filter(dirent => dirent.isDirectory())
    .map(dirent => dirent.name)
    .filter(name => !name.startsWith('.') && !name.includes('test'));
  
  moduleDirectories.forEach(moduleName => {
    this.loadCommandsFromDirectory(path.join(__dirname, moduleName));
  });
}
```

### Clean CLI Syntax
Commands use simplified camelCase syntax instead of the old `[CMD:UPPERCASE]` format:

**Old Syntax:**
```bash
[CMD:SCREENSHOT] {"selector": "body"}
[CMD:BROWSER_JS] console.log("test");
```

**New Syntax:**
```bash
screenshot {"selector": "body"}
browserJs console.log("test");
```

### Inheritance-Ready Structure
Each categorical module can define base classes for shared functionality:
- `browser/` → BrowserBaseCommand.cjs
- `communication/` → CommunicationBaseCommand.cjs  
- `input/` → InputBaseCommand.cjs
- etc.

### How It Works

1. **Drop command file** into appropriate categorical directory
2. **Implements the standard interface** (see below)
3. **Automatically discovered** by dynamic module discovery
4. **Shows up everywhere instantly:**
   - WebSocket connection banner
   - `continuum --help` output
   - `/connect` API endpoint documentation
   - Agent portal examples
   - Dynamic usage guides

**No configuration files, no registration, no hardcoded lists needed!**

## 📋 Command Interface Contract

Every command must implement this interface:

```javascript
class YourCommand {
  static getDefinition() {
    return {
      name: 'COMMAND_NAME',           // Uppercase command identifier
      description: 'What it does',    // Brief description
      params: '<param_format>',       // Parameter format/syntax
      examples: [                     // Usage examples (optional)
        'example_param_1',
        'example_param_2'
      ],
      category: 'Core',              // Core|Gaming|Browser|Custom
      icon: '🎯'                     // Emoji icon (optional)
    };
  }
  
  static async execute(params, continuum, encoding = 'utf-8') {
    // Your command implementation here
    
    return {
      executed: true,                // Required: boolean success
      message: 'Success message',    // Optional: human readable result
      result: 'return_value',        // Optional: actual result data
      error: null                    // Optional: error message if failed
    };
  }
}

module.exports = YourCommand;
```

## 🔧 Parameters

- **`params`** - The parameter string passed to your command
- **`continuum`** - Full access to Continuum instance (WebSocket, browser control, etc.)
- **`encoding`** - Parameter encoding (usually 'utf-8' or 'base64')

## 📁 Package Structure

Each command is a **complete package** with dual-side execution capabilities:

```
src/commands/core/[command]/
├── [Command]Command.cjs         # Server-side implementation
├── [Command]Command.client.js   # Client-side implementation (optional)
├── index.server.js              # Module definition and registration
├── package.json                 # 🎯 Package rules and execution contract
└── test/                        # Command-specific tests
```

### Categorical Command Structure
```
src/commands/
├── README.md                    # This file (documents categorical innovation)
├── browser/                     # Browser automation and control
│   ├── browserjs/               # JavaScript execution in browser
│   ├── promisejs/               # Promise-based JS execution
│   ├── screenshot/              # Screenshot capture
│   └── browser/                 # Browser state management
├── communication/               # Multi-user communication
│   ├── chat/                    # Chat messaging
│   ├── createroom/              # Room creation
│   ├── joinroom/                # Room joining
│   ├── listrooms/               # Room listing
│   ├── loadrooms/               # Room loading
│   ├── share/                   # Content sharing
│   └── findUser/                # User discovery
├── input/                       # Input automation
│   ├── move/                    # Mouse movement
│   ├── cursor/                  # Cursor control
│   ├── type/                    # Text input
│   ├── input/                   # General input
│   └── clear/                   # Clear operations
├── file/                        # File system operations
│   ├── fileSave/                # File saving
│   ├── savefile/                # Alternative file save
│   └── exec/                    # Command execution
├── ui/                          # User interface
│   └── emotion/                 # Emotion display
├── development/                 # Development tools
│   ├── validatecode/            # Code validation
│   ├── validatejs/              # JavaScript validation
│   ├── macro/                   # Macro operations
│   ├── spawn/                   # Process spawning
│   └── test/                    # Testing utilities
├── monitoring/                  # System monitoring
│   ├── agents/                  # Agent dashboard
│   ├── diagnostics/             # System diagnostics
│   ├── sentinel/                # AI monitoring
│   └── listagents/              # Agent listing
├── docs/                        # Documentation
│   └── docs/                    # Documentation viewer
├── planning/                    # Project planning
│   ├── roadmap/                 # Roadmap management
│   ├── analyze/                 # Analysis tools
│   └── restore/                 # Restoration planning
└── core/                        # Core system commands
    ├── restart/                 # Server restart
    ├── workspace/               # Workspace management
    ├── help/                    # Help system
    ├── info/                    # System information
    └── preferences/             # System preferences
```

## 🎯 Package-Defined Execution Rules

Each command package defines its **complete execution contract** in `package.json`:

### Example: Restart Command Package
```javascript
// src/commands/core/restart/package.json
{
  "name": "@continuum/restart-command",
  "version": "1.2.0",
  "description": "Server restart with version management",
  "timeouts": {
    "client": 70.0,        // How long client should wait
    "server": 30.0         // How long server execution should take
  },
  "retries": {
    "client": 1,           // Client retry attempts
    "server": 0            // Server doesn't retry restart
  },
  "behavior": {
    "client": "wait_and_auto_heal",
    "server": "kill_self_after_response"
  },
  "concurrency": {
    "client": false,       // Don't allow multiple restart calls
    "server": false        // Server can't handle concurrent restarts
  },
  "sideEffects": ["version_bump", "process_restart", "file_system"]
}
```

### Example: Screenshot Command Package
```javascript
// src/commands/core/screenshot/package.json
{
  "name": "@continuum/screenshot-command",
  "version": "2.1.0",
  "description": "Desktop screenshot capture with browser integration",
  "timeouts": {
    "client": 30.0,        // Client waits for image capture + processing
    "server": 15.0         // Server execution: capture + save + respond
  },
  "retries": {
    "client": 2,           // Client retries on network issues
    "server": 1            // Server retries on capture failures
  },
  "resources": {
    "client": ["display_access", "file_system"],
    "server": ["screenshot_api", "file_storage", "browser_connection"]
  },
  "concurrency": {
    "client": true,        // Multiple screenshot requests OK
    "server": true         // Server can handle concurrent captures
  },
  "sideEffects": ["creates_files", "system_capture"]
}
```

## 🔄 Dual-Side Execution Model

Commands can execute on **both client and server** with different requirements:

### Server-Side Implementation
```javascript
// RestartCommand.cjs
class RestartCommand extends BaseCommand {
  static async execute(params, continuum) {
    const rules = require('./package.json');
    const serverTimeout = rules.timeouts.server * 1000;
    
    // Server enforces its own execution timeout
    return await Promise.race([
      this.actualRestart(params),
      this.timeoutAfter(serverTimeout)
    ]);
  }
}
```

### Client-Side Implementation  
```javascript
// ScreenshotCommand.client.js (for browser-specific logic)
export class ScreenshotClientCommand {
  static async execute(params) {
    const rules = await import('./package.json');
    const clientTimeout = rules.timeouts.client * 1000;
    
    return await Promise.race([
      this.captureDOMAndSend(params),
      this.clientTimeout(clientTimeout)
    ]);
  }
}
```

## 🤖 AI Portal Integration

The AI Portal respects each command's package rules:

```python
# python-client/ai-portal.py
async def run_command(cmd: str, params: str = "{}"):
    # Get command-specific rules from package.json
    rules = await get_command_package_rules(cmd)
    
    client_timeout = rules.get('timeouts', {}).get('client', 10.0)
    retries = rules.get('retries', {}).get('client', 2)
    auto_heal = rules.get('behavior', {}).get('client') == 'wait_and_auto_heal'
    
    # Apply command's rules
    for attempt in range(retries):
        try:
            async with asyncio.timeout(client_timeout):
                result = await client.send_command(cmd, params)
                return result
        except asyncio.TimeoutError:
            if auto_heal and cmd == 'restart':
                return handle_restart_timeout()
```

## 🎯 Example: Simple Command Package

```javascript
// src/commands/core/HelloCommand.cjs
class HelloCommand {
  static getDefinition() {
    return {
      name: 'HELLO',
      description: 'Send greeting message',
      params: '<message>',
      examples: [
        'Hello World!',
        'Greetings from Continuum'
      ],
      category: 'Core',
      icon: '👋'
    };
  }
  
  static async execute(params, continuum) {
    // Send message to browser console
    if (continuum.webSocketServer) {
      continuum.webSocketServer.broadcast({
        type: 'execute_js',
        data: {
          command: `console.log('Hello: ${params}');`,
          timestamp: new Date().toISOString()
        }
      });
    }
    
    return {
      executed: true,
      message: `Greeting sent: ${params}`,
      result: params
    };
  }
}

module.exports = HelloCommand;
```

## 🛰️ Usage Examples

Once your command is dropped in, it's immediately available:

### WebSocket
```json
{"type": "task", "role": "system", "task": "[CMD:HELLO] Hello from WebSocket!"}
```

### HTTP API
```bash
curl -X POST http://localhost:9000/connect \
  -H "Content-Type: application/json" \
  -d '{"command": "HELLO", "params": "Hello from API!"}'
```

### Agent Scripts
```bash
# If you have a wrapper in agent-scripts
hello-send "Hello from agent portal!"
```

## 🔄 Auto-Discovery Process

1. **CommandProcessor scans** this directory recursively
2. **Loads all `.cjs` files** that export command classes
3. **Validates interface** (has `getDefinition()` and `execute()`)
4. **Registers commands** in the global command registry
5. **Updates documentation** automatically everywhere

## 📚 Command Categories

- **Core** - Essential system commands (EXEC, BROWSER_JS, etc.)
- **Browser** - Browser automation and control
- **Gaming** - Game-specific automation
- **Automation** - General automation tasks
- **Custom** - User-defined commands

## 🔧 Advanced Features

### Access Continuum Instance
```javascript
// In your execute() method
continuum.webSocketServer.broadcast(message);    // Send to browsers
continuum.commandProcessor.execute(otherCmd);    // Call other commands
continuum.activeConnections.size;               // Get connection count
```

### Error Handling
```javascript
static async execute(params, continuum) {
  try {
    // Your logic here
    return { executed: true, result: 'success' };
  } catch (error) {
    return { 
      executed: false, 
      error: error.message,
      stack: error.stack 
    };
  }
}
```

### Parameter Validation
```javascript
static async execute(params, continuum) {
  if (!params || params.trim() === '') {
    return {
      executed: false,
      error: 'Parameter required',
      usage: 'HELLO <message>'
    };
  }
  
  // Continue with execution...
}
```

## 🌟 Key Architecture Principles

### 1. Self-Contained Packages
Each command is a **complete executable package** that defines its own:
- **Execution timeouts** (client vs server requirements)
- **Retry strategies** (network vs logic failures)
- **Concurrency rules** (single-threaded vs parallel-safe)
- **Resource requirements** (file system, display, browser, etc.)
- **Side effects** (creates files, restarts processes, modifies state)

### 2. Dual-Side Execution Model
Commands execute on **both client and server** with different contracts:

```javascript
// Server: Handles actual work, enforces server timeout
static async execute(params, continuum) {
  const rules = require('./package.json');
  const timeout = rules.timeouts.server * 1000;
  return await Promise.race([
    this.actualWork(params),
    this.timeoutAfter(timeout)
  ]);
}
```

```python
# Client: Manages network, enforces client timeout + auto-healing
async def run_command(cmd, params):
  rules = await get_package_rules(cmd)
  timeout = rules.timeouts.client
  retries = rules.retries.client
  
  async with asyncio.timeout(timeout):
    return await client.send_command(cmd, params)
```

### 3. Command-Specific Behaviors
Different commands have different execution patterns:

- **RESTART**: Client waits 70s, server kills itself after 30s
- **SCREENSHOT**: Client waits 30s, server captures in 15s  
- **WORKSPACE**: Client waits 5s, server responds instantly
- **SENTINEL**: Client waits 45s, server runs persistent monitoring

### 4. Auto-Healing Integration
The AI Portal respects each command's behavior requirements:

```python
# For restart command: Expected timeout triggers auto-healing
if cmd == 'restart' and 'timeout' in error:
    return await handle_expected_restart_behavior()

# For other commands: Timeout indicates real failure
else:
    return await retry_with_auto_heal()
```

## 🎯 Package.json Contract Examples

### High-Performance Command (Workspace)
```javascript
{
  "timeouts": {"client": 5.0, "server": 1.0},
  "retries": {"client": 0, "server": 0},
  "concurrency": {"client": true, "server": true},
  "sideEffects": ["creates_directories"]
}
```

### Critical System Command (Restart)  
```javascript
{
  "timeouts": {"client": 70.0, "server": 30.0},
  "retries": {"client": 1, "server": 0},
  "behavior": {"client": "wait_and_auto_heal", "server": "kill_self"},
  "concurrency": {"client": false, "server": false},
  "sideEffects": ["version_bump", "process_restart", "file_system"]
}
```

### Monitoring Command (Sentinel)
```javascript
{
  "timeouts": {"client": 45.0, "server": 60.0},
  "retries": {"client": 2, "server": 1},
  "behavior": {"client": "persistent_connection", "server": "background_task"},
  "resources": {"server": ["log_files", "process_monitoring", "file_system"]},
  "concurrency": {"client": false, "server": true}
}
```

## 🚀 Just Drop and Go!

That's it! Drop your command file in this directory following the interface contract, and it immediately becomes part of the Continuum command ecosystem. No restarts, no configuration, no manual registration needed.

**The system discovers and documents itself automatically.**

---

## 🏗️ Architectural Brilliance Summary

This package-based architecture represents a breakthrough in modular system design:

1. **📦 Package-Defined Rules**: Each command package defines its complete execution contract
2. **🔄 Dual-Side Timeouts**: Client and server enforce their own appropriate timeouts  
3. **🤖 Intelligent Auto-Healing**: Clients respect command-specific failure behaviors
4. **🎯 Self-Documenting**: Help system generates live docs from package definitions
5. **🚀 Zero Configuration**: Drop files and go - no registration or config needed
6. **⚡ Command-Specific Optimization**: Each command optimized for its specific use case

This eliminates god objects, hardcoded timeouts, and brittle client-server coupling while enabling intelligent auto-healing and self-documentation.

---

## 🔬 Agent Observation & Testing Methodology

### 📖 Overview: Fresh Agent Intelligence Gathering

Just like **AR app user testing** where you observe first-time users without intervention to see where they get confused, we use **fresh AI agent instances** to gather intelligence about system usability, documentation clarity, and onboarding effectiveness.

### 🎯 The Methodology: Pure Observation Testing

**Goal**: Understand how fresh agents naturally approach the system and where they encounter confusion.

**Principle**: **No intervention** - let agents explore organically and document their natural discovery process.

### 🤖 Agent Observation Workflow

#### 1. Spawn Fresh Agent Instance
```bash
# Create isolated tmux session for agent observation
tmux new-session -d -s agent-observer-$(date +%s)
tmux attach -t agent-observer-...

# OR use the portal command (when fixed):
python3 python-client/ai-portal.py spawn
```

#### 2. Minimal Context Prompt
Give the fresh agent **minimal information** to simulate real first-time experience:

```
You are a new AI agent. You've been given access to this codebase.

MISSION: Explore the system and write a summary of:
1. What you think you should do first
2. What priorities you would set
3. Where you get confused or blocked
4. What would help you be more effective

DO NOT ask for help. Just explore and document your natural discovery process.

START: Run any command you think makes sense to understand the system.
```

#### 3. Observation Categories
**Track these behaviors without intervention:**

**🎯 Discovery Patterns:**
- Does the agent run `--dashboard` first?
- Do they try `--help` or `--test`?
- What commands do they attempt first?
- How do they react to broken commands?

**😕 Confusion Points:**
- Where do they get stuck?
- What file/directory structures confuse them?
- Which error messages are unclear?
- What documentation gaps do they encounter?

**🧠 Natural Priorities:**
- What do they think is most important to fix?
- How do they categorize problems?
- What workflow do they naturally develop?

#### 4. Agent Summary Requirements
Have each agent write a structured summary:

```markdown
## Fresh Agent Report - Session [timestamp]

### 🎯 First Impressions
- What I tried first and why
- Initial understanding of the system
- Most obvious entry points

### 📊 Natural Priorities (In Order)
1. [What seemed most critical]
2. [What seemed important but not urgent]
3. [What seemed like nice-to-have]

### 😕 Confusion & Blocking Points
- File/directory structure issues
- Unclear error messages
- Missing documentation
- Broken workflows

### 💡 Improvement Suggestions
- What would have helped me onboard faster
- Documentation that should exist
- Commands/tools that are missing
- Structure changes that would help

### 🔧 Commands I Tried (In Order)
1. `command-name` - Why I tried it, what happened
2. `command-name` - Why I tried it, what happened
...

### 📈 Effectiveness Score
Rate how effective you felt: [1-10]
Biggest barrier to effectiveness: [description]
```

### 🧪 Testing Variations

#### A. Complete Fresh Agent (Zero Context)
- No prior conversation history
- Minimal initial prompt
- Pure discovery observation

#### B. Dashboard-Guided Agent  
- Start with: "Run the dashboard first, then explore"
- See if dashboard effectively guides them
- Test dashboard effectiveness

#### C. Task-Specific Agent
- Give specific mission: "Fix one broken command"
- Observe how they approach problem-solving
- Test workflow effectiveness

#### D. Structure-Focused Agent
- Mission: "Understand and improve file organization"
- Start with FILES.md
- Test structure reduction workflow

### 📊 Intelligence Collection

**Document every observation in:**

#### 1. Individual Agent Reports
- Save each agent's summary in `agent-observations/`
- Track patterns across multiple fresh agents
- Note recurring confusion points

#### 2. FILES.md Structure Comments
- When agents get confused about files, add comments:
```markdown
# FILES.md
src/complicated/deep/structure/confusing.js
  # AGENT CONFUSION: 3 agents couldn't find this, moved to obvious location
```

#### 3. Dashboard Intelligence Integration
- Feed confusion patterns back into dashboard priorities
- Update quick actions based on agent behavior
- Improve guidance based on natural discovery patterns

#### 4. README & Documentation Updates
- Fix documentation gaps agents consistently hit
- Add missing onboarding steps
- Clarify confusing explanations

### 🔄 Continuous Improvement Loop

```
Fresh Agent → Documents Confusion → System Improvements → Better Onboarding → Fresh Agent
```

**Example Pattern:**
1. **Agent gets confused** by test organization
2. **Documents it** in observation report
3. **System improved** with test organization fixes
4. **Next agent** has smoother experience
5. **Validates improvement** or finds new confusion points

### 📋 Observation Session Checklist

**Before Session:**
- [ ] Create fresh tmux session
- [ ] Prepare minimal context prompt
- [ ] Ready to observe without intervention
- [ ] Have template for agent summary

**During Session:**
- [ ] No hints or guidance
- [ ] Document natural command sequence
- [ ] Note confusion points and error reactions
- [ ] Track time spent on different activities

**After Session:**
- [ ] Get agent's structured summary
- [ ] Compare with previous agent patterns
- [ ] Update FILES.md with confusion points
- [ ] Plan system improvements based on findings

### 🎯 Success Metrics

**Effective Dashboard:**
- Agents naturally find `--dashboard` command
- Dashboard clearly guides them to next steps
- Confusion points decrease over time

**Good File Structure:**
- Agents can quickly understand project layout
- Files are named clearly enough for agents to guess purpose
- Directory depth doesn't overwhelm agents

**Clear Documentation:**
- Agents can solve problems using existing docs
- Error messages guide agents to solutions
- Workflows are discoverable without human intervention

### 💡 Advanced Techniques

#### Multi-Agent Parallel Testing
```bash
# Spawn multiple agents simultaneously
for i in {1..3}; do
  tmux new-session -d -s agent-$i
done

# Compare how different agents approach same problems
# Identify consistent vs divergent patterns
```

#### Confusion Injection Testing
```bash
# Intentionally break something agents use
# Observe how they debug and recover
# Test system resilience and agent adaptability
```

#### Time-Pressure Testing
```bash
# Give agents limited time constraints
# See what they prioritize under pressure
# Test dashboard effectiveness for quick decision-making
```

### 🏆 Expected Outcomes

This methodology should produce:

1. **Validated dashboard effectiveness** - Does it actually guide agents well?
2. **Structure improvement priorities** - Which files/directories cause most confusion?
3. **Documentation gap identification** - What critical info is missing?
4. **Workflow optimization** - How can we make agent onboarding smoother?
5. **Natural priority validation** - Are our roadmap priorities aligned with agent intuition?

**The goal**: Every fresh agent has a progressively smoother experience, and their confusion becomes the intelligence that drives continuous improvement.

---

*This methodology treats fresh AI agents as the ultimate usability testers - they have no preconceptions, can articulate confusion clearly, and provide pure feedback on system design effectiveness.*