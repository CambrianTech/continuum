# Session Manager Daemon

**🎯 Kernel Service**: Session-aware artifact management and lifecycle coordination for the Continuum unified artifact system.

## 🚀 Usage

### Command Interface
```bash
# Basic usage
continuum session-manager

# With options (customize based on your module)
continuum session-manager --help
continuum session-manager --verbose
```

### Programmatic Usage
```typescript
import { SessionManagerCommand } from './SessionManagerCommand.js';

// Execute the command
const result = await SessionManagerCommand.execute({
  // Add your parameters here
});

console.log(result);
```

## ⚙️ Configuration

```json
{
  "daemon": "session-manager",
  "category": "Core",
  "capabilities": [
    "session-management",
    "artifact-coordination",
    "session-isolation",
    "connection-identity"
  ],
  "dependencies": [
    "kernel-session-command",
    "kernel-daemon-command",
    "continuum-directory-daemon",
    "file-write-command",
    "file-read-command"
  ],
  "interfaces": [
    "daemon-protocol",
    "session-management"
  ],
  "permissions": [
    "session-management",
    "file-system",
    "daemon-communication"
  ]
}
```

## 🧪 Testing

```bash
# Run all tests
npm test

# Run specific test types
npm run test:unit
npm run test:integration

# Validate module compliance
npm run validate
```

## 🏗️ Development

This module follows the Continuum modular architecture:

- **Self-validating**: Module validates its own compliance
- **Middle-out**: Tests from core outward 
- **Object-oriented**: Inherits from base classes
- **Migration-ready**: Can upgrade structure automatically

### Module Structure
```
session-manager/
├── SessionManagerCommand.ts     # Main implementation
├── test/
│   ├── unit/             # Unit tests
│   └── integration/      # Integration tests
├── package.json          # Module configuration
└── README.md            # This file
```

## 🏗️ **Unified Artifact System Architecture**

The SessionManagerDaemon is a **kernel service** that provides session-aware context for all artifact creation across Continuum:

### **Session Context Management**
```typescript
interface SessionContext {
  sessionId: string;           // portal-2025-06-30-1843, git-hook-2025-06-30-1843
  type: SessionType;          // portal | git-hook | interactive | test
  startTime: number;          // Session lifecycle tracking
  artifactCount: number;      // Number of artifacts created
  metadata: {
    user?: string;            // Who initiated the session
    command?: string;         // Which command started it
    branch?: string;          // Git context (for git-hook sessions)
  };
}
```

### **Artifact Integration**
**Every command** that produces artifacts coordinates through this daemon:

```typescript
// Commands send artifact requests to SessionManager
interface ArtifactRequest {
  sessionId: string;           // Auto-provided by SessionManager
  type: ArtifactType;         // screenshot | log | export | config | backup
  content: Buffer | string;   // The actual data
  metadata: {
    command: string;          // Which command created it
    timestamp: number;        // When created
    filename?: string;        // Preferred name
    format?: string;          // png | json | txt | md
    category?: string;        // subcategory within type
  };
}
```

### **Session-Aware Directory Structure**
```
.continuum/
├── sessions/
│   ├── portal-2025-06-30-1843/        ← AI Portal sessions
│   │   └── artifacts/
│   │       ├── screenshots/           ← From ScreenshotCommand
│   │       ├── logs/                  ← From ChatHistoryCommand
│   │       ├── exports/               ← From PreferencesCommand exports
│   │       └── reports/               ← From HealthCommand reports
│   │
│   ├── git-hook-2025-06-30-1843/      ← Git hook validation sessions
│   │   └── artifacts/
│   │       ├── screenshots/           ← Pre-commit validation screenshots
│   │       └── reports/               ← Validation reports
│   │
│   └── interactive-2025-06-30-1843/   ← Manual CLI sessions
│       └── artifacts/
│
├── global/                            ← Cross-session artifacts
│   ├── configs/                       ← Global preferences, settings
│   └── backups/                       ← System backups
```

### **Command Integration Pattern**

**All commands** use this pattern for artifact creation:

```typescript
class ExampleCommand extends DirectCommand {
  protected static async executeOperation(params: any, context?: CommandContext): Promise<CommandResult> {
    // 1. Command performs its logic
    const data = await this.performCommandLogic(params);
    
    // 2. For any artifacts, delegate to kernel services
    return this.createSuccessResult('Command completed', {
      artifact: {
        type: ArtifactType.SCREENSHOT,  // or LOG, EXPORT, REPORT, etc.
        content: data,
        metadata: {
          command: 'example',
          filename: params.filename,
          category: 'validation'
        }
      }
    });
    
    // 3. SessionManager + ContinuumDirectoryDaemon handle the rest:
    //    - Determine session context
    //    - Create appropriate directory structure
    //    - Save with consistent naming
    //    - Track for lifecycle management
  }
}
```

### **Universal Usage**
**Same artifact system** used by:
- ✅ **AI Portal** (`python3 python-client/ai-portal.py --cmd screenshot`)
- ✅ **Git Hooks** (`continuum screenshot --selector="body"`)  
- ✅ **Interactive CLI** (manual commands)
- ✅ **Browser Client** (WebSocket commands)

All produce artifacts in **session-appropriate locations** with **consistent structure** and **automatic lifecycle management**.

## 🪟 **Intelligent Window/Tab Management**

The SessionManagerDaemon provides **smart window and tab orchestration** based on session type and collaboration needs:

### **Window Strategy by Session Type**

```typescript
interface WindowManagementStrategy {
  sessionType: 'portal' | 'devtools' | 'collaborative' | 'git-hook';
  windowAction: 'new-window' | 'new-tab' | 'join-existing' | 'shared-tab';
  tabGrouping: boolean;
  collaborationMode: 'shared-view' | 'parallel-view' | 'isolated';
}
```

### **Session Type Window Behaviors**

**🚀 Portal Sessions:**
```bash
# First portal session
python3 python-client/ai-portal.py --cmd screenshot
# → NEW WINDOW: "Continuum Portal - Human+AI Collaboration"
# → URL: localhost:9000?session=portal-2025-06-30-1905

# Additional portal commands
python3 python-client/ai-portal.py --cmd health
# → NEW TAB in existing Portal window
# → URL: localhost:9000?session=portal-2025-06-30-1905&command=health
```

**🔧 DevTools Sessions:**
```bash
# First DevTools session
continuum devtools --url="localhost:9000" --inspect
# → NEW WINDOW: "DevTools - Continuum Analysis"
# → chrome-devtools://devtools/bundled/inspector.html?ws=localhost:9222/...

# Different page inspection
continuum devtools --url="localhost:9001" --inspect
# → NEW TAB in existing DevTools window
```

**🤝 Collaborative Sessions:**
```bash
# Human initiates collaboration
python3 python-client/ai-portal.py --cmd connect --collaborative
# → NEW WINDOW: "Collaborative Session - portal-collab-1905"
# → URL: localhost:9000?session=portal-collab-1905&mode=collaborative

# AI joins via portal
# → SAME WINDOW, SAME TAB (shared view)
# → URL: localhost:9000?session=portal-collab-1905&participants=human,ai

# Additional browser connection (human opens browser manually)
# → SAME WINDOW, either same tab or parallel tab based on preference
```

### **Human-AI Collaboration Window Management**

**The Portal-Browser Collaboration Pattern:**

```typescript
interface CollaborationConnection {
  sessionId: string;              // portal-collab-2025-06-30-1905
  participants: {
    human: {
      portalConnection: boolean;  // python3 ai-portal.py active
      browserConnection: boolean; // localhost:9000 browser tab open
      preferredView: 'shared' | 'parallel';
    };
    ai: {
      portalAccess: boolean;      // AI has portal command access
      browserAccess: boolean;     // AI can control browser via DevTools
      autonomousMode: boolean;    // AI can take screenshots, inspect elements
    };
  };
  sharedBrowserState: {
    windowId: string;             // Same browser window
    tabId: string;                // Same or parallel tab
    url: string;                  // Synchronized URL
    devToolsAccess: boolean;      // Shared DevTools Protocol access
  };
}
```

**Example: Human+AI Portal Collaboration:**

```bash
# 1. Human starts collaborative session from Portal
python3 python-client/ai-portal.py --cmd connect --mode collaborative
# SessionManager creates:
# → Window: "Human+AI Collaboration - Session 1905"
# → Tab 1: localhost:9000?session=portal-collab-1905&participant=human
# → .continuum/sessions/portal-collab-1905/

# 2. AI joins the collaboration (automatically via portal)
# AI detects human's collaborative session and connects
# SessionManager decides:
# → SAME WINDOW: "Human+AI Collaboration - Session 1905" 
# → SAME TAB: localhost:9000?session=portal-collab-1905&participants=human,ai
# → Shared browser state, shared DevTools access

# 3. Human opens browser manually (or AI suggests it)
# Browser navigates to: localhost:9000?session=portal-collab-1905
# SessionManager recognizes existing session:
# → Uses EXISTING window and tab
# → Human sees same state AI sees
# → All screenshots/commands go to same artifacts location

# 4. Collaborative workflow
# Human (via Portal): python3 ai-portal.py --cmd screenshot --selector=".main-ui"
# AI (via Portal):    Automatically takes screenshot of error states
# Human (via Browser): Manually inspects elements, clicks, navigates
# → All activities synchronized
# → All artifacts saved to .continuum/sessions/portal-collab-1905/artifacts/
# → Perfect collaboration with maintained window state
```

### **Window Lifecycle Management**

```typescript
class SessionWindowManager {
  async ensureCollaborativeWindow(sessionId: string): Promise<WindowInfo> {
    const existingWindows = await this.findSessionWindows(sessionId);
    
    if (existingWindows.length === 0) {
      // Create new collaborative window
      return await this.createWindow({
        title: `Human+AI Collaboration - Session ${sessionId.split('-').pop()}`,
        url: `localhost:9000?session=${sessionId}&mode=collaborative`,
        sessionType: 'collaborative',
        allowJoining: true,
        sharedState: true
      });
    } else {
      // Use existing window, ensure accessibility
      return await this.connectToWindow(existingWindows[0], {
        maintainState: true,
        synchronizeView: true
      });
    }
  }
  
  async handleParticipantJoin(sessionId: string, participant: ParticipantInfo): Promise<TabInfo> {
    const session = await this.getCollaborativeSession(sessionId);
    
    if (participant.preferredMode === 'shared-view') {
      // Same tab, synchronized view
      return await this.shareExistingTab(session.primaryTab, participant);
    } else {
      // Parallel tab in same window
      return await this.createParallelTab(session.windowId, {
        url: `localhost:9000?session=${sessionId}&participant=${participant.id}`,
        syncWith: session.primaryTab
      });
    }
  }
}
```

### **Connection Command Integration**

The `connect` command orchestrates window management for collaborative sessions:

```bash
# Connect command with window management
continuum connect --mode collaborative --window-strategy shared
# → Creates or joins collaborative window
# → Ensures proper tab management
# → Sets up synchronized browser state
# → Configures shared artifact location

# Portal-initiated connection
python3 python-client/ai-portal.py --cmd connect --collaborative --maintain-window
# → SessionManager ensures window continuity
# → AI and human use same browser window/tab
# → Perfect synchronization for screenshot analysis, UI inspection
```

### **Benefits for Human-AI Collaboration**

✅ **Same Browser Window** - Human and AI literally see the same browser state  
✅ **Synchronized Navigation** - URL changes visible to both participants  
✅ **Shared DevTools Access** - Collaborative debugging and inspection  
✅ **Unified Artifact Location** - All screenshots, logs, exports in one place  
✅ **Window Persistence** - Collaborative window maintained across commands  
✅ **Intelligent Tab Management** - New tabs vs shared tabs based on collaboration mode  
✅ **Session Continuity** - Window survives individual command completions  

**This creates a true shared workspace** where human Portal commands and AI autonomous actions operate on the same browser instance, with perfect synchronization and unified artifact management.

## 🔧 Bootstrap Information

This file was auto-generated during module migration. The module now has:

- ✅ Complete package.json with continuum configuration
- ✅ Test directories (unit/integration)
- ✅ TypeScript ES module setup
- ✅ Compliance validation

**Next Steps**: Implement your module logic and update this documentation!