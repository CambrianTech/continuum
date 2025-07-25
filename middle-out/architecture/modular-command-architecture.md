# Modular Command Architecture
## 🚀 **SUPERSEDES**: symmetric-daemon-architecture.md

**Status**: ✅ ACTIVE - Current architectural pattern  
**Priority**: FOUNDATIONAL  
**Pattern Stability**: BATTLE-TESTED  
**Updated**: 2025-07-25

## 🎯 CORE PRINCIPLE: SMALL, INDEPENDENT, DISCOVERABLE MODULES

Every command is a **completely independent module** following the **screenshot pattern**. No massive daemons, no god objects, no cross-command dependencies.

## 🔄 **COMMANDS vs DAEMONS: ARCHITECTURAL CLARITY**

### **COMMANDS = STATELESS OPERATIONS**
- **Execute actions** (save, load, click, navigate, compile)
- **Return results** (success/failure, data)
- **No state retention** (fire-and-forget operations)
- **Delegate to daemons** when persistent state is needed

### **DAEMONS = STATEFUL SERVICES**
- **Maintain state** (file handles, connections, caches, sessions)
- **Hold resources** (open files, database pools, WebSocket connections)
- **Manage lifecycle** (startup, shutdown, cleanup, background tasks)
- **Background processing** (file watchers, upload queues, periodic sync)

### **ELEGANT COOPERATION PATTERN**
```typescript
// Commands NEVER bypass daemons for resource access
class FileSaveCommand {
  async execute(params: FileSaveParams) {
    // ALWAYS delegate to daemon for file operations
    // Daemon has smart control over file access, permissions, watching, etc.
    return await this.artifactsDaemon.save(params);
  }
}

// Daemons provide the ONLY interface to system resources
class ArtifactsDaemon {
  private fileHandles = new Map<string, fs.FileHandle>();
  private watchers = new Map<string, fs.FSWatcher>();
  private uploadQueues = new Map<string, UploadQueue>();
  private permissions = new Map<string, FilePermissions>();
  
  async save(params: FileSaveParams) {
    // Smart file access control:
    // - Check permissions
    // - Manage file locking
    // - Handle concurrent access
    // - Setup watching if needed
    // - Queue for backup/sync
    // - Maintain file handle cache
    
    if (!this.permissions.canWrite(params.filepath)) {
      throw new Error('Access denied');
    }
    
    // Centralized, intelligent file management
  }
}
```

### **🚫 CRITICAL RULE: NO DIRECT RESOURCE ACCESS**
- **❌ Commands must NEVER import `fs`, `net`, `child_process`, etc.**
- **❌ Commands must NEVER bypass daemon resource management**  
- **✅ Commands delegate ALL resource operations to appropriate daemons**
- **✅ Daemons provide the ONLY interface to system resources**

**Both patterns are essential and complementary - commands for operations, daemons for state.**

### 🏗️ **Universal Command Structure** (Sacred Pattern)

```
src/debug/jtag/daemons/command-daemon/commands/{command-name}/
├── shared/
│   ├── {CommandName}Types.ts     # ~50 lines - Types and validation only
│   └── README.md                 # Command documentation
├── client/                       # Optional - browser-specific logic
│   └── {CommandName}Client.ts    # ~40 lines - Browser execution
├── server/                       # Always present - server execution
│   └── {CommandName}Command.ts   # ~60 lines - Server implementation
└── tests/                        # Command-specific tests
    └── {command-name}.test.ts    # Pattern compliance validation
```

## 📐 **SIZE CONSTRAINTS** (Architectural Enforcement)

- **Shared types**: 40-60 lines maximum
- **Client implementation**: 40-60 lines maximum  
- **Server implementation**: 50-80 lines maximum
- **Total command module**: <200 lines across all files

**If any file exceeds these limits, it violates the pattern and must be split.**

## 🧬 **COMMAND PATTERN DNA** (Screenshot Template)

### **1. Shared Types Pattern** (`shared/ScreenshotTypes.ts`)

```typescript
import { CommandParams, CommandResult } from '../../shared/command-types';

export class ScreenshotParams extends CommandParams {
  filename?: string;
  querySelector?: string;
  fullPage?: boolean;
  quality?: number;

  constructor(data: Partial<ScreenshotParams> = {}) {
    super();
    Object.assign(this, {
      filename: 'screenshot.png',
      querySelector: 'body',
      fullPage: true,
      quality: 90,
      ...data
    });
  }
}

export class ScreenshotResult extends CommandResult {
  filename: string;
  path: string;
  size: number;
  captured: boolean;

  constructor(data: Partial<ScreenshotResult>) {
    super();
    Object.assign(this, {
      success: false,
      filename: '',
      path: '',
      size: 0,
      captured: false,
      environment: 'server',
      timestamp: new Date().toISOString(),
      ...data
    });
  }
}
```

### **2. Server Implementation Pattern** (`server/ScreenshotCommand.ts`)

```typescript
import { Command } from '../../shared/Command';
import { ScreenshotParams, ScreenshotResult } from '../shared/ScreenshotTypes';

export class ScreenshotCommand extends Command<ScreenshotParams, ScreenshotResult> {
  name = 'screenshot';
  
  async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
    // Single-purpose implementation
    // No business logic beyond this specific command
    // No knowledge of other commands
  }
}
```

### **3. Client Implementation Pattern** (`client/ScreenshotClient.ts` - Optional)

```typescript
import { ScreenshotParams, ScreenshotResult } from '../shared/ScreenshotTypes';

export class ScreenshotClient {
  async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
    // Browser-specific screenshot capture
    // Thin wrapper around browser APIs
    // Routes to server for actual processing
  }
}
```

## 🔄 **PATTERN EXPLOITATION STRATEGY**

### **Classification → Reduction → Extension Cycle**

#### **1. Classification Phase**
- Identify command patterns in existing code
- Extract common structures (params, results, validation)
- Find repeated boilerplate that can be optimized

#### **2. Reduction Phase**  
- Create shared base classes and utilities
- Optimize constructor patterns with Object.assign
- Eliminate code duplication through smart inheritance

#### **3. Extension Phase**
- Apply optimized patterns to new commands
- Generate boilerplate automatically where possible
- Scale pattern to infinite command types

### **Constructor Optimization Pattern**

```typescript
// BEFORE: Verbose parameter assignment
constructor(data: Partial<NavigateParams>) {
  super();
  this.url = data.url || '';
  this.timeout = data.timeout || 30000;
  this.waitForSelector = data.waitForSelector;
}

// AFTER: Object.assign optimization  
constructor(data: Partial<NavigateParams> = {}) {
  super();
  Object.assign(this, {
    url: '',
    timeout: 30000,
    waitForSelector: undefined,
    ...data
  });
}
```

## 🏭 **COMMAND FACTORY & DISCOVERY**

### **Dynamic Command Registration**

```typescript
// CommandFactory auto-discovers all commands
export class CommandFactory {
  private static commands = new Map<string, Command>();
  
  static async discoverCommands(): Promise<void> {
    const commandDirs = await glob('commands/*/server/*Command.ts');
    
    for (const commandPath of commandDirs) {
      const CommandClass = await import(commandPath);
      const command = new CommandClass.default();
      this.commands.set(command.name, command);
    }
  }
  
  static getCommand(name: string): Command | undefined {
    return this.commands.get(name);
  }
}
```

### **Self-Extending Architecture**

Each new command automatically:
- Registers itself with CommandFactory
- Becomes available through JTAG router
- Works with browser and server contexts
- Supports P2P distributed execution

## ✅ **IMPLEMENTED COMMAND EXAMPLES**

### **Foundational Commands** (Battle-tested patterns)
- **`screenshot/`** - Screenshot capture (✅ Template pattern)
- **`navigate/`** - Browser navigation (✅ Following pattern)
- **`click/`** - Element clicking (✅ Following pattern)
- **`type/`** - Text input (✅ Following pattern)

### **Pattern Compliance Verification**
```typescript
describe('Command Pattern Compliance', () => {
  it('should have proper module structure', () => {
    expect(fs.existsSync('commands/screenshot/shared')).toBe(true);
    expect(fs.existsSync('commands/screenshot/server')).toBe(true);
    expect(fs.existsSync('commands/screenshot/tests')).toBe(true);
  });
  
  it('should have size constraints', () => {
    const sharedSize = getFileSize('commands/screenshot/shared/ScreenshotTypes.ts');
    const serverSize = getFileSize('commands/screenshot/server/ScreenshotCommand.ts');
    
    expect(sharedSize).toBeLessThan(80); // lines
    expect(serverSize).toBeLessThan(100); // lines
  });
});
```

## 🚫 **ANTI-PATTERNS** (Architectural Violations)

### **❌ MASSIVE DAEMON FILES**
- BrowserTypes.ts (485 lines) - Should be individual commands
- CompilerTypes.ts (866 lines) - Should be language-specific commands
- DatabaseTypes.ts (500+ lines) - Should be operation-specific commands

### **❌ GOD OBJECT THINKING**
```typescript
// VIOLATION: Massive type collection
interface BrowserDaemonTypes {
  navigate: NavigateParams;
  click: ClickParams;
  type: TypeParams;
  scroll: ScrollParams;
  // ... 50 more commands
}

// CORRECT: Individual command modules
// commands/navigate/shared/NavigateTypes.ts
// commands/click/shared/ClickTypes.ts  
// commands/type/shared/TypeTypes.ts
```

### **❌ CROSS-COMMAND DEPENDENCIES**
```typescript
// VIOLATION: Commands knowing about each other
class NavigateCommand {
  async execute() {
    // Calls click command directly
    await this.clickCommand.execute();
  }
}

// CORRECT: Commands are isolated
class NavigateCommand {
  async execute() {
    // Only navigation logic
    // No knowledge of other commands
  }
}
```

## 🎯 **DEVELOPMENT WORKFLOW**

### **Creating New Commands**

1. **Copy screenshot pattern**:
   ```bash
   cp -r commands/screenshot commands/new-command
   ```

2. **Rename and customize**:
   - Update type names
   - Modify parameters and results
   - Implement command-specific logic

3. **Verify pattern compliance**:
   ```bash
   npm run test:pattern-compliance
   ```

4. **Auto-registration**:
   Command automatically discovered by factory

### **Migration from Violation Code**

1. **Identify command boundaries** in massive files
2. **Extract individual commands** following pattern
3. **Delete violation directories** completely
4. **Verify all functionality** preserved

## 🔮 **FUTURE SCALABILITY**

### **Infinite Command Types**
- Browser automation commands
- File system commands  
- Database operation commands
- AI/ML processing commands
- P2P networking commands
- Compilation commands

### **Marketplace Architecture**
```typescript
// Future: Commands as downloadable modules
await CommandMarketplace.install('advanced-screenshot');
await CommandMarketplace.install('ai-image-processing');
await CommandMarketplace.install('pdf-generation');
```

### **Auto-Generation Opportunities**
- Command scaffolding from OpenAPI specs
- Type generation from JSON schemas
- Test generation from command interfaces
- Documentation generation from code

## 🧪 **TESTING STRATEGY**

### **Layer 1: Pattern Compliance**
```typescript
// Test every command follows the pattern
test('All commands follow modular pattern', () => {
  const commands = discoverCommands();
  
  commands.forEach(command => {
    expect(command).toHaveModularStructure();
    expect(command).toRespectSizeConstraints();
    expect(command).toBeIndependent();
  });
});
```

### **Layer 2: Command Functionality**
```typescript
// Test individual command logic
describe('ScreenshotCommand', () => {
  it('should capture screenshots properly', async () => {
    const command = new ScreenshotCommand();
    const result = await command.execute(new ScreenshotParams());
    
    expect(result.success).toBe(true);
    expect(result.captured).toBe(true);
  });
});
```

### **Layer 3: Factory Discovery**
```typescript
// Test auto-discovery and registration
test('CommandFactory discovers all commands', async () => {
  await CommandFactory.discoverCommands();
  
  expect(CommandFactory.getCommand('screenshot')).toBeDefined();
  expect(CommandFactory.getCommand('navigate')).toBeDefined();
  expect(CommandFactory.getCommand('click')).toBeDefined();
});
```

## 🎊 **SUCCESS METRICS**

### **Architecture Quality**
- ✅ Zero files >100 lines
- ✅ Zero cross-command dependencies
- ✅ All commands follow identical pattern
- ✅ Factory auto-discovery working

### **Development Velocity**
- ✅ New commands created in <30 minutes
- ✅ Pattern violations caught automatically
- ✅ No architectural decision paralysis
- ✅ Copy-paste-customize workflow

### **System Reliability**
- ✅ Commands independently testable
- ✅ Failures isolated to single commands
- ✅ Hot-reload without system restart
- ✅ Zero coupling between commands

---

## 💡 **KEY INSIGHT**

**Modular commands are the most successful pattern we've discovered.** They eliminate architectural complexity through radical simplification - each command is a tiny, focused, independent module that does exactly one thing well.

This isn't just organization - it's **cognitive architecture** that makes the system comprehensible, maintainable, and infinitely extensible.

**The pattern works because it mirrors how humans naturally think about tasks: one action, one outcome, one responsibility.**