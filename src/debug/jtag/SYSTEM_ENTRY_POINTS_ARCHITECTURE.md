# JTAG System Entry Points & Milestone Architecture

## 🎯 **Entry Points Catalog**

### **1. Primary Development Entry Points**
```bash
npm start                    # Developer starting system for work
npm test                     # Developer running full test suite  
./jtag [command]            # CLI command execution
./continuum [command]       # Alias for jtag
```

### **2. NPM Script Entry Points** 
```bash
npm run system:start        # Background tmux system startup
npm run system:ensure       # Conditional startup (if not running)
npm run system:stop         # System shutdown
npm run system:restart      # Full restart cycle
npm run smart-build         # Intelligent build process
```

### **3. Test Suite Entry Points**
```bash
npm run test:comprehensive  # Full comprehensive test suite
npm run test:single-dependency  # Single test file
npm run test:e2e            # End-to-end tests  
npm run test:*              # 50+ specific test categories
npx tsx tests/specific.test.ts  # Direct test execution
```

### **4. Development Tool Entry Points**
```bash
npm run agent:*             # AI agent interactions
npm run logs:dashboard      # Log monitoring
npm run dev:*               # Development utilities
npm run screenshot          # Direct screenshot
```

### **5. Build & Deployment Entry Points**
```bash
npm run prebuild            # Structure generation
npm run build               # Full build process
npm run pack                # Package creation
npm run version:bump        # Version management
```

### **6. Git Hook Entry Points**
```bash
.git/hooks/pre-commit       # Pre-commit validation
.git/hooks/pre-push         # Pre-push testing
npm run test:precommit      # Git integration testing
```

### **7. External Integration Entry Points**
```bash
Browser: http://localhost:9002      # Browser desktop interface
WebSocket connections               # Real-time command interface
AI Persona commands                 # Autonomous system interaction
Cross-project CLI usage            # Global installation usage
```

## 🏗️ **Current Architecture Problems**

### **❌ Inconsistent Startup Logic**
- `npm start` uses `scripts/system-startup.ts`
- `npm run system:start` uses `scripts/launch-and-capture.ts`  
- `npm run system:run` uses `scripts/launch-active-example.ts`
- CLI commands use `EntryPointAdapter` → different startup path

### **❌ Browser Opening Too Early**
**Critical Issue**: Browser opens before server is ready to serve HTML
- Happens in: `scripts/launch-active-example.ts`
- Result: User sees blank page, must refresh
- **NOT a timeout problem** - this is a **signaling failure**

### **❌ Scattered Milestone Detection**
- Build status: `scripts/smart-build.ts`
- Port readiness: `scripts/signal-system-ready.ts` 
- Bootstrap status: Multiple locations
- Health checking: Various daemon files
- Browser readiness: Inconsistent detection

### **❌ Inconsistent Error Reporting**
- Some paths use `console.error` → stdout only
- Some paths use log files only
- Some paths use both (inconsistently)
- Error context often lost between layers

## 🎯 **Milestone-Based Architecture Design**

### **Core Milestones (Constants)**
```typescript
export const SYSTEM_MILESTONES = {
  // Build Phase
  BUILD_START: 'build_start',
  BUILD_TYPESCRIPT: 'build_typescript_complete',
  BUILD_STRUCTURE: 'build_structure_complete', 
  BUILD_COMPLETE: 'build_complete',
  
  // Deployment Phase  
  DEPLOY_START: 'deploy_start',
  DEPLOY_FILES: 'deploy_files_complete',
  DEPLOY_PORTS: 'deploy_ports_allocated',
  DEPLOY_COMPLETE: 'deploy_complete',
  
  // Server Phase
  SERVER_START: 'server_start',
  SERVER_PROCESS: 'server_process_ready',
  SERVER_WEBSOCKET: 'server_websocket_ready',
  SERVER_HTTP: 'server_http_ready', 
  SERVER_BOOTSTRAP: 'server_bootstrap_complete',
  SERVER_COMMANDS: 'server_commands_loaded',
  SERVER_READY: 'server_ready',
  
  // Browser Phase (CRITICAL - happens AFTER server ready)
  BROWSER_LAUNCH: 'browser_launch_initiated', 
  BROWSER_CONNECTED: 'browser_websocket_connected',
  BROWSER_READY: 'browser_interface_loaded',
  
  // System Phase
  SYSTEM_HEALTHY: 'system_healthy',
  SYSTEM_READY: 'system_ready'
} as const;
```

### **Milestone Dependencies**
```typescript
export const MILESTONE_DEPENDENCIES = {
  [MILESTONES.BUILD_TYPESCRIPT]: [],
  [MILESTONES.BUILD_STRUCTURE]: [MILESTONES.BUILD_TYPESCRIPT],
  [MILESTONES.BUILD_COMPLETE]: [MILESTONES.BUILD_TYPESCRIPT, MILESTONES.BUILD_STRUCTURE],
  
  [MILESTONES.DEPLOY_START]: [MILESTONES.BUILD_COMPLETE],
  [MILESTONES.DEPLOY_COMPLETE]: [MILESTONES.DEPLOY_FILES, MILESTONES.DEPLOY_PORTS],
  
  [MILESTONES.SERVER_PROCESS]: [MILESTONES.DEPLOY_COMPLETE],
  [MILESTONES.SERVER_WEBSOCKET]: [MILESTONES.SERVER_PROCESS],
  [MILESTONES.SERVER_HTTP]: [MILESTONES.SERVER_PROCESS], 
  [MILESTONES.SERVER_BOOTSTRAP]: [MILESTONES.SERVER_WEBSOCKET, MILESTONES.SERVER_HTTP],
  [MILESTONES.SERVER_COMMANDS]: [MILESTONES.SERVER_BOOTSTRAP],
  [MILESTONES.SERVER_READY]: [MILESTONES.SERVER_COMMANDS],
  
  // CRITICAL: Browser launch MUST wait for server ready
  [MILESTONES.BROWSER_LAUNCH]: [MILESTONES.SERVER_READY],
  [MILESTONES.BROWSER_CONNECTED]: [MILESTONES.BROWSER_LAUNCH],
  [MILESTONES.BROWSER_READY]: [MILESTONES.BROWSER_CONNECTED],
  
  [MILESTONES.SYSTEM_READY]: [MILESTONES.SERVER_READY, MILESTONES.BROWSER_READY]
} as const;
```

### **Event System Architecture**
```typescript
export interface MilestoneEvent {
  readonly milestone: string;
  readonly timestamp: number;
  readonly success: boolean;
  readonly error?: string;
  readonly metadata?: Record<string, any>;
  readonly entryPoint: string; // Which entry point triggered this
}

export interface MilestoneProgress {
  readonly total: number;
  readonly completed: number;
  readonly current: string;
  readonly percentage: number;
  readonly estimatedTimeRemaining?: number;
}
```

## 🔧 **Universal Entry Point Architecture**

### **Single Orchestrator Pattern**
```typescript
export class SystemOrchestrator {
  async orchestrate(entryPoint: EntryPointType, options: OrchestrationOptions): Promise<SystemState> {
    // 1. Determine required milestones based on entry point
    const requiredMilestones = this.getRequiredMilestones(entryPoint);
    
    // 2. Check current system state
    const currentState = await this.getCurrentState();
    
    // 3. Calculate missing milestones
    const missingMilestones = this.calculateMissingMilestones(requiredMilestones, currentState);
    
    // 4. Execute milestones in dependency order
    for (const milestone of missingMilestones) {
      await this.executeMilestone(milestone, options);
    }
    
    // 5. Verify final state
    return await this.verifySystemState(requiredMilestones);
  }
}
```

### **Entry Point Specific Requirements**
```typescript
const ENTRY_POINT_REQUIREMENTS = {
  'npm-start': [MILESTONES.SERVER_READY, MILESTONES.BROWSER_READY],
  'npm-test': [MILESTONES.SERVER_READY, MILESTONES.BROWSER_READY], 
  'cli-command': [MILESTONES.SERVER_READY], // Browser optional for CLI
  'single-test': [MILESTONES.SERVER_READY, MILESTONES.BROWSER_READY],
  'git-hook': [MILESTONES.BUILD_COMPLETE, MILESTONES.SERVER_READY],
  'agent-command': [MILESTONES.SERVER_READY, MILESTONES.BROWSER_READY]
} as const;
```

## 📊 **Logging & Error Architecture**

### **Universal Logging Strategy**
```typescript
export class UniversalLogger {
  // ALL entry points log to BOTH stdout AND files
  async log(level: LogLevel, message: string, context: LogContext): Promise<void> {
    // 1. Terminal output (formatted for human)
    console.log(this.formatForTerminal(level, message, context));
    
    // 2. File logging (structured for parsing)  
    await this.writeToLogFile(level, message, context);
    
    // 3. Event emission (for real-time monitoring)
    this.emitLogEvent(level, message, context);
  }
}
```

### **Error Reporting Strategy** 
```typescript
export class ErrorReporter {
  async reportError(error: Error, milestone: string, entryPoint: string): Promise<void> {
    // 1. Immediate terminal feedback
    console.error(`❌ ${milestone} failed in ${entryPoint}: ${error.message}`);
    
    // 2. Detailed log file entry
    await this.logError(error, milestone, entryPoint);
    
    // 3. System state capture for debugging
    await this.captureSystemState(error, milestone);
    
    // 4. Recovery suggestions
    this.suggestRecovery(error, milestone, entryPoint);
  }
}
```

## 🔄 **Implementation Strategy**

### **Phase 1: Milestone Constants & Events**
1. Create milestone constants
2. Implement event system  
3. Add milestone tracking to existing scripts
4. Test with current entry points

### **Phase 2: Universal Orchestrator**
1. Build SystemOrchestrator class
2. Migrate npm start to use orchestrator
3. Migrate npm test to use orchestrator  
4. Test and validate

### **Phase 3: Fix Browser Timing**
1. Ensure browser launch waits for SERVER_READY milestone
2. Add proper browser readiness detection
3. Test across all entry points

### **Phase 4: Universal Logging**
1. Implement UniversalLogger
2. Migrate all entry points to use it
3. Ensure consistent error reporting

### **Phase 5: CLI Integration**
1. Integrate single test execution via `./jtag test file.ts`
2. Ensure CLI commands use same orchestration
3. Test global CLI usage patterns

## ✅ **Success Criteria**

After each phase, verify:
- ✅ `npm start` works correctly  
- ✅ `npm test` works correctly
- ✅ `./jtag test file.ts` works correctly
- ✅ Browser opens ONLY after server ready
- ✅ All errors are logged AND displayed  
- ✅ Progress is visible during all operations
- ✅ System recovery works after failures

## 🎯 **Key Principles**

1. **No Timeouts**: Timeouts represent signaling failures, not solutions
2. **Milestone-Driven**: Every operation waits for explicit milestones
3. **Universal Logging**: All entry points log consistently  
4. **Dependency-Based**: Milestones execute in proper dependency order
5. **Error Transparency**: Failures are immediately visible with context
6. **Entry Point Agnostic**: Same core logic regardless of how system started