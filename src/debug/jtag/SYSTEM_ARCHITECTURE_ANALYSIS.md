# JTAG System Architecture Analysis

## Current Problem Statement

The JTAG system has evolved into a complex web of entry points, startup scripts, and build processes that need consolidation. Multiple paths exist for the same core functionality, leading to:

1. **Multiple startup paths** with different behaviors
2. **Build/rebuild logic scattered** across multiple scripts
3. **Output capture inconsistencies** - sometimes stdout only, sometimes logs only
4. **Error handling variations** across different entry points
5. **Tmux session management** duplicated in multiple places

## Current Entry Points & Their Flows

### 1. Developer Usage (Claude/AI debugging)
```bash
npm start → scripts/system-startup.ts → startSystem() → tmux session
```

### 2. Developer Testing
```bash
npm test → system:ensure → signal:check → system:start → comprehensive tests
```

### 3. Git Hook Validation
```bash
git commit → pre-commit hook → npm run jtag → validation chain
```

### 4. Direct CLI Commands
```bash
./continuum [command] → CLI entry point → potential system startup
```

### 5. Browser/Persona Commands
```bash
Browser → WebSocket → Command routing → potential compilation/restart
```

## Current System Startup Scripts Analysis

### Primary Scripts:
1. **`scripts/system-startup.ts`** - New shared startup (our creation)
2. **`scripts/launch-and-capture.ts`** - Used by `system:start`
3. **`scripts/launch-active-example.ts`** - Actual server launcher
4. **`scripts/test-with-server.ts`** - Test harness with startup
5. **`scripts/signal-system-ready.ts`** - Readiness detection

### Secondary Scripts:
- `scripts/smart-build.ts` - Build intelligence
- `scripts/smart-deploy.ts` - Deployment logic
- `scripts/cleanup-dynamic-ports.ts` - Process cleanup

## Issues Identified

### 1. Build Process Fragmentation
- `smart-build.ts` handles TypeScript compilation
- `npm run prebuild` handles structure generation
- Build triggers scattered across multiple entry points
- No single source of truth for "what needs rebuilding"

### 2. Output Capture Inconsistency
- Some paths use `stdio: 'inherit'` (direct stdout)
- Some paths use log files only
- Some paths capture both stdout AND logs
- No consistent logging strategy across entry points

### 3. Tmux Session Management Duplication
- `system-startup.ts` creates tmux sessions
- `launch-and-capture.ts` creates tmux sessions
- Different session naming strategies
- Different cleanup approaches

### 4. Error Handling Variations
- Some scripts exit on error
- Some scripts continue despite errors
- Different error reporting formats
- Inconsistent timeout handling

### 5. Signal/Readiness Detection Complexity
- Multiple signal file locations
- Different timeout strategies
- Inconsistent health checking
- No unified readiness protocol

## Proposed Centralized Architecture

### Core System Management Service
```
/jtag/system/core/
├── SystemManager.ts          # Master orchestrator
├── BuildManager.ts           # Centralized build logic
├── ProcessManager.ts         # Tmux/process lifecycle
├── ReadinessManager.ts       # Health/signal detection
└── OutputManager.ts          # Consistent logging/stdout
```

### Entry Point Standardization
All entry points should use the same core flow:
1. **Request system state** from SystemManager
2. **Determine required actions** (build, restart, etc.)
3. **Execute through centralized managers**
4. **Return consistent results**

### Output Strategy Unification
- **Development mode**: stdout + logs (for immediate feedback)
- **Test mode**: logs only (for clean test output)
- **CI mode**: structured output (for parsing)
- **All modes**: consistent error reporting

## Immediate Action Items

1. **Document current working behavior** - What exactly works now?
2. **Identify all entry points** - Complete inventory
3. **Map build dependencies** - When does what need rebuilding?
4. **Standardize output capture** - One strategy for all scenarios
5. **Create SystemManager** - Central orchestration point
6. **Migrate entry points** - One by one to use SystemManager
7. **Eliminate duplication** - Remove redundant scripts

## Questions for Investigation

1. **Build Dependencies**: What triggers TypeScript rebuilds? When is `dist/` stale?
2. **Tmux Requirements**: Why do we need persistent sessions? Can we simplify?
3. **Signal Files**: Are multiple signal strategies necessary?
4. **Port Management**: How do we handle port conflicts across contexts?
5. **Error Recovery**: What should happen when builds fail?
6. **Browser Integration**: How do browser commands trigger rebuilds?

## Success Criteria

✅ **Single entry point** for system management  
✅ **Consistent build behavior** across all scenarios  
✅ **Unified output strategy** - see output AND capture logs  
✅ **Robust error handling** with clear recovery paths  
✅ **Simplified tmux management** with consistent cleanup  
✅ **Fast feedback loops** for development iteration  

The goal is to move from "multiple fragmented scripts" to "single robust system orchestrator" that handles all the complexity internally while providing consistent behavior across all entry points.